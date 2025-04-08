// src/auth/auth.ts

import { PublicKey, verifyMessage } from '@solana/web3.js';
import { sign, verify, decode, JwtPayload, SignOptions } from 'jsonwebtoken';
import { v4 as uuidv4 } from 'uuid';
import { DateTime } from 'luxon';
import { config } from '../config';
import { logger } from '../logger';

export type AuthNonce = {
  nonce: string;
  publicKey: string;
  expiresAt: Date;
};

export type AuthPayload = {
  publicKey: string;
  role: 'user' | 'validator' | 'admin';
  iss: 'umazen';
  aud: 'umazen';
};

export class AuthError extends Error {
  constructor(
    public code: string,
    message: string,
    public meta?: Record<string, unknown>
  ) {
    super(message);
    Object.setPrototypeOf(this, AuthError.prototype);
  }
}

export class AuthService {
  private nonceStore = new Map<string, AuthNonce>();
  private jwtSecret: string;
  private nonceExpiryMinutes: number;

  constructor() {
    if (!config.JWT_SECRET) {
      throw new AuthError(
        'CONFIG_ERROR',
        'JWT_SECRET environment variable not set'
      );
    }
    this.jwtSecret = config.JWT_SECRET;
    this.nonceExpiryMinutes = config.NONCE_EXPIRY_MINUTES || 5;
  }

  async generateNonce(publicKey: string): Promise<AuthNonce> {
    this.cleanupExpiredNonces();
    
    const existingNonce = [...this.nonceStore.values()]
      .find(n => n.publicKey === publicKey);
    
    if (existingNonce) {
      return existingNonce;
    }

    const nonce = uuidv4();
    const expiresAt = DateTime.now()
      .plus({ minutes: this.nonceExpiryMinutes })
      .toJSDate();

    const authNonce: AuthNonce = {
      nonce,
      publicKey,
      expiresAt
    };

    this.nonceStore.set(nonce, authNonce);
    
    logger.info('Generated new nonce', { publicKey, nonce });
    return authNonce;
  }

  async verifySignature(
    publicKey: string,
    signature: Buffer,
    nonce: string
  ): Promise<{ token: string; payload: AuthPayload }> {
    const storedNonce = this.nonceStore.get(nonce);
    
    if (!storedNonce) {
      throw new AuthError(
        'INVALID_NONCE', 
        'Nonce not found or expired'
      );
    }
    
    if (storedNonce.publicKey !== publicKey) {
      throw new AuthError(
        'PUBKEY_MISMATCH',
        'Public key does not match nonce registration'
      );
    }
    
    if (new Date() > storedNonce.expiresAt) {
      this.nonceStore.delete(nonce);
      throw new AuthError(
        'NONCE_EXPIRED', 
        'Nonce has expired'
      );
    }

    const message = new TextEncoder().encode(
      `Umazen Auth: ${nonce}`
    );

    let isValid: boolean;
    try {
      isValid = verifyMessage(
        message,
        signature,
        publicKey
      );
    } catch (error) {
      throw new AuthError(
        'SIGNATURE_VERIFICATION_FAILED',
        'Error verifying signature',
        { error }
      );
    }

    if (!isValid) {
      throw new AuthError(
        'INVALID_SIGNATURE',
        'Signature verification failed'
      );
    }

    this.nonceStore.delete(nonce);
    
    const payload = this.createJwtPayload(publicKey);
    const token = this.signJwt(payload);

    logger.info('Successful authentication', { publicKey });
    return { token, payload };
  }

  verifyToken(token: string): AuthPayload {
    try {
      const payload = verify(token, this.jwtSecret, {
        audience: 'umazen',
        issuer: 'umazen'
      }) as AuthPayload;

      if (!payload.publicKey || !PublicKey.isOnCurve(payload.publicKey)) {
        throw new AuthError(
          'INVALID_TOKEN',
          'Invalid public key in token'
        );
      }

      return payload;
    } catch (error) {
      if (error instanceof AuthError) throw error;
      
      throw new AuthError(
        'TOKEN_VERIFICATION_FAILED',
        'Failed to verify JWT',
        { error }
      );
    }
  }

  private createJwtPayload(publicKey: string): AuthPayload {
    return {
      publicKey,
      role: this.determineUserRole(publicKey),
      iss: 'umazen',
      aud: 'umazen'
    };
  }

  private determineUserRole(publicKey: string): AuthPayload['role'] {
    if (config.ADMIN_PUBKEYS?.includes(publicKey)) return 'admin';
    if (config.VALIDATOR_PUBKEYS?.includes(publicKey)) return 'validator';
    return 'user';
  }

  private signJwt(payload: AuthPayload): string {
    const options: SignOptions = {
      expiresIn: '8h',
      algorithm: 'HS512'
    };

    return sign(payload, this.jwtSecret, options);
  }

  private cleanupExpiredNonces(): void {
    const now = new Date();
    for (const [nonce, entry] of this.nonceStore.entries()) {
      if (now > entry.expiresAt) {
        this.nonceStore.delete(nonce);
      }
    }
  }
}

// Singleton instance for dependency injection
export const authService = new AuthService();
