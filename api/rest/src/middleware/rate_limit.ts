// src/middleware/rate_limit.ts

import { Request, Response, NextFunction } from 'express';
import { Redis } from 'ioredis';
import { config } from '../config';
import { logger } from '../logger';
import { createHash } from 'crypto';
import { RateLimitError } from '../errors';

type RateLimitStrategy = 'fixed' | 'sliding' | 'token_bucket';
type IdentifierType = 'ip' | 'userId' | 'apiKey' | 'publicKey';

interface RateLimitRule {
  strategy: RateLimitStrategy;
  identifier: IdentifierType;
  limit: number;
  windowMs: number;
  bucketCapacity?: number; // For token bucket
  tokensPerInterval?: number; // For token bucket
}

interface RateLimitResult {
  allowed: boolean;
  limit: number;
  remaining: number;
  reset: number;
}

interface RateLimitStore {
  get(key: string): Promise<number | null>;
  set(key: string, value: number, ttlMs: number): Promise<void>;
  incr(key: string, ttlMs: number): Promise<number>;
}

class MemoryStore implements RateLimitStore {
  private hits = new Map<string, { count: number; expiresAt: number }>();
  private interval: NodeJS.Timeout;

  constructor(cleanupIntervalMs = 60_000) {
    this.interval = setInterval(() => this.cleanup(), cleanupIntervalMs);
  }

  async get(key: string): Promise<number | null> {
    const entry = this.hits.get(key);
    if (!entry || Date.now() > entry.expiresAt) return null;
    return entry.count;
  }

  async set(key: string, value: number, ttlMs: number): Promise<void> {
    this.hits.set(key, {
      count: value,
      expiresAt: Date.now() + ttlMs,
    });
  }

  async incr(key: string, ttlMs: number): Promise<number> {
    const entry = this.hits.get(key) || { count: 0, expiresAt: 0 };
    
    if (Date.now() > entry.expiresAt) {
      entry.count = 0;
      entry.expiresAt = Date.now() + ttlMs;
    }

    entry.count++;
    this.hits.set(key, entry);
    return entry.count;
  }

  private cleanup() {
    const now = Date.now();
    for (const [key, entry] of this.hits.entries()) {
      if (now > entry.expiresAt) this.hits.delete(key);
    }
  }

  stop() {
    clearInterval(this.interval);
  }
}

class RedisStore implements RateLimitStore {
  constructor(private client: Redis) {}

  async get(key: string): Promise<number | null> {
    const count = await this.client.get(key);
    return count ? parseInt(count, 10) : null;
  }

  async set(key: string, value: number, ttlMs: number): Promise<void> {
    await this.client.set(key, value, 'PX', ttlMs);
  }

  async incr(key: string, ttlMs: number): Promise<number> {
    const luaScript = `
      local current = redis.call('GET', KEYS[1])
      if current == false then
        redis.call('SET', KEYS[1], 1, 'PX', ARGV[1])
        return 1
      end
      return redis.call('INCR', KEYS[1])
    `;
    
    const result = await this.client.eval(luaScript, 1, key, ttlMs);
    return parseInt(result as string, 10);
  }
}

export class RateLimiter {
  private store: RateLimitStore;
  private rules: RateLimitRule[];

  constructor(rules: RateLimitRule[], store?: RateLimitStore) {
    this.rules = rules;
    this.store = store || new MemoryStore();
  }

  async check(
    identifier: string,
    rule: RateLimitRule
  ): Promise<RateLimitResult> {
    const key = this.generateKey(identifier, rule);
    
    switch (rule.strategy) {
      case 'fixed':
        return this.fixedWindow(key, rule);
      case 'sliding':
        return this.slidingWindow(key, rule);
      case 'token_bucket':
        return this.tokenBucket(key, rule);
      default:
        throw new Error('Invalid rate limit strategy');
    }
  }

  private generateKey(identifier: string, rule: RateLimitRule): string {
    const prefix = `rate_limit:${rule.strategy}:${rule.identifier}`;
    return `${prefix}:${createHash('sha256').update(identifier).digest('hex')}`;
  }

  private async fixedWindow(
    key: string,
    rule: RateLimitRule
  ): Promise<RateLimitResult> {
    const count = (await this.store.incr(key, rule.windowMs)) || 0;
    const reset = Math.ceil((Date.now() + rule.windowMs) / 1000);
    
    return {
      allowed: count <= rule.limit,
      limit: rule.limit,
      remaining: Math.max(rule.limit - count, 0),
      reset,
    };
  }

  private async slidingWindow(
    key: string,
    rule: RateLimitRule
  ): Promise<RateLimitResult> {
    const now = Date.now();
    const trimTime = now - rule.windowMs;

    const luaScript = `
      local key = KEYS[1]
      local now = ARGV[1]
      local trimTime = ARGV[2]
      local window = ARGV[3]
      
      redis.call('ZREMRANGEBYSCORE', key, 0, trimTime)
      redis.call('ZADD', key, now, now)
      redis.call('EXPIRE', key, window / 1000)
      return redis.call('ZCARD', key)
    `;

    const count = await this.store.client.eval(
      luaScript,
      1,
      key,
      now,
      trimTime,
      rule.windowMs
    );

    const currentCount = parseInt(count as string, 10);
    const oldest = await this.store.client.zrange(key, 0, 0);
    const reset = oldest.length 
      ? Math.ceil((parseInt(oldest[0]) + rule.windowMs) / 1000)
      : Math.ceil((now + rule.windowMs) / 1000);

    return {
      allowed: currentCount <= rule.limit,
      limit: rule.limit,
      remaining: Math.max(rule.limit - currentCount, 0),
      reset,
    };
  }

  private async tokenBucket(
    key: string,
    rule: RateLimitRule
  ): Promise<RateLimitResult> {
    if (!rule.bucketCapacity || !rule.tokensPerInterval) {
      throw new Error('Token bucket requires capacity and tokens per interval');
    }

    const luaScript = `
      local key = KEYS[1]
      local now = ARGV[1]
      local capacity = ARGV[2]
      local tokensPerInterval = ARGV[3]
      local window = ARGV[4]
      
      local bucket = redis.call('HMGET', key, 'tokens', 'lastRefill')
      local tokens = tonumber(bucket[1]) or capacity
      local lastRefill = tonumber(bucket[2]) or now
      
      local timePassed = now - lastRefill
      local intervalsPassed = math.floor(timePassed / window)
      
      if intervalsPassed > 0 then
        local newTokens = intervalsPassed * tokensPerInterval
        tokens = math.min(tokens + newTokens, capacity)
        lastRefill = lastRefill + (intervalsPassed * window)
      end
      
      if tokens >= 1 then
        tokens = tokens - 1
        redis.call('HMSET', key, 'tokens', tokens, 'lastRefill', lastRefill)
        redis.call('PEXPIRE', key, window * 2)
        return {1, tokens}
      else
        return {0, tokens}
      end
    `;

    const [allowed, tokens] = await this.store.client.eval(
      luaScript,
      1,
      key,
      Date.now(),
      rule.bucketCapacity,
      rule.tokensPerInterval,
      rule.windowMs
    ) as [number, number];

    const reset = Math.ceil((Date.now() + rule.windowMs) / 1000);

    return {
      allowed: allowed === 1,
      limit: rule.bucketCapacity,
      remaining: tokens,
      reset,
    };
  }
}

export const rateLimiter = new RateLimiter([
  {
    strategy: 'fixed',
    identifier: 'ip',
    limit: config.RATE_LIMIT_IP,
    windowMs: 60_000,
  },
  {
    strategy: 'token_bucket',
    identifier: 'userId',
    limit: config.RATE_LIMIT_USER,
    windowMs: 60_000,
    bucketCapacity: 100,
    tokensPerInterval: 10,
  },
], config.REDIS_URL ? new RedisStore(new Redis(config.REDIS_URL)) : undefined);

export const rateLimitMiddleware = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const identifier = getIdentifier(req);
    const results = await Promise.all(
      rateLimiter.rules.map(rule => 
        rateLimiter.check(identifier[rule.identifier], rule)
      )
    );

    const worstCase = results.reduce((prev, current) => 
      (!prev || current.remaining < prev.remaining) ? current : prev
    );

    if (!worstCase.allowed) {
      res.setHeader('X-RateLimit-Limit', worstCase.limit.toString());
      res.setHeader('X-RateLimit-Remaining', worstCase.remaining.toString());
      res.setHeader('X-RateLimit-Reset', worstCase.reset.toString());
      throw new RateLimitError('Too many requests');
    }

    next();
  } catch (error) {
    next(error);
  }
};

function getIdentifier(req: Request): Record<IdentifierType, string> {
  return {
    ip: req.ip,
    userId: req.user?.id || '',
    apiKey: req.headers['x-api-key']?.toString() || '',
    publicKey: req.user?.publicKey || '',
  };
}
