//! Cryptographic Utilities - Secure Data Handling for Web3 & AI

import { Buffer } from 'buffer';
import { sha256, sha3_256 } from 'js-sha256';
import { pbkdf2Sync, randomBytes } from 'crypto-browserify';
import { webcrypto } from 'crypto-web'; // Browser-compatible polyfill

type Algorithm = 'AES-GCM' | 'XChaCha20-Poly1305';
type KeyUsage = 'encrypt' | 'decrypt' | 'sign' | 'verify' | 'derive';

const ITERATIONS = 310_000; // OWASP 2023 recommendations
const KEY_LENGTH = 256;
const SALT_LENGTH = 32;
const IV_LENGTH = 12; // AES-GCM standard

export class CryptoUtils {
  // Core Encryption/Decryption
  static async encrypt(
    plaintext: string,
    key: CryptoKey,
    algorithm: Algorithm = 'AES-GCM'
  ): Promise<{ ciphertext: ArrayBuffer; iv: Uint8Array }> {
    const iv = webcrypto.getRandomValues(new Uint8Array(IV_LENGTH));
    const encoded = new TextEncoder().encode(plaintext);
    
    const ciphertext = await webcrypto.subtle.encrypt(
      {
        name: algorithm,
        iv,
        ...(algorithm === 'XChaCha20-Poly1305' && { 
          tagLength: 128 
        })
      },
      key,
      encoded
    );

    return { ciphertext, iv };
  }

  static async decrypt(
    ciphertext: ArrayBuffer,
    key: CryptoKey,
    iv: Uint8Array,
    algorithm: Algorithm = 'AES-GCM'
  ): Promise<string> {
    const plaintext = await webcrypto.subtle.decrypt(
      { name: algorithm, iv },
      key,
      ciphertext
    );

    return new TextDecoder().decode(plaintext);
  }

  // Key Management
  static async generateKey(
    algorithm: Algorithm = 'AES-GCM',
    usages: KeyUsage[] = ['encrypt', 'decrypt']
  ): Promise<CryptoKey> {
    return webcrypto.subtle.generateKey(
      {
        name: algorithm,
        length: KEY_LENGTH,
      },
      true,
      usages
    );
  }

  static async exportKey(key: CryptoKey): Promise<ArrayBuffer> {
    return webcrypto.subtle.exportKey('raw', key);
  }

  static async importKey(
    rawKey: ArrayBuffer,
    algorithm: Algorithm = 'AES-GCM',
    usages: KeyUsage[] = ['encrypt', 'decrypt']
  ): Promise<CryptoKey> {
    return webcrypto.subtle.importKey(
      'raw',
      rawKey,
      { name: algorithm },
      true,
      usages
    );
  }

  // Hashing
  static sha256(data: string): string {
    return sha256.create().update(data).hex();
  }

  static sha3256(data: string): string {
    return sha3_256.create().update(data).hex();
  }

  static async hmacSign(
    secret: string,
    data: string
  ): Promise<ArrayBuffer> {
    const key = await webcrypto.subtle.importKey(
      'raw',
      new TextEncoder().encode(secret),
      { name: 'HMAC', hash: 'SHA-256' },
      false,
      ['sign']
    );

    return webcrypto.subtle.sign(
      'HMAC',
      key,
      new TextEncoder().encode(data)
    );
  }

  // PBKDF2 Key Derivation
  static deriveKey(
    password: string,
    salt: Buffer = randomBytes(SALT_LENGTH)
  ): { key: Buffer; salt: Buffer } {
    const key = pbkdf2Sync(
      password,
      salt,
      ITERATIONS,
      KEY_LENGTH / 8,
      'sha256'
    );

    return { key, salt };
  }

  // Digital Signatures
  static async generateKeyPair(): Promise<CryptoKeyPair> {
    return webcrypto.subtle.generateKey(
      {
        name: 'ECDSA',
        namedCurve: 'P-256',
      },
      true,
      ['sign', 'verify']
    );
  }

  static async signData(
    privateKey: CryptoKey,
    data: ArrayBuffer
  ): Promise<ArrayBuffer> {
    return webcrypto.subtle.sign(
      {
        name: 'ECDSA',
        hash: 'SHA-256',
      },
      privateKey,
      data
    );
  }

  static async verifySignature(
    publicKey: CryptoKey,
    signature: ArrayBuffer,
    data: ArrayBuffer
  ): Promise<boolean> {
    return webcrypto.subtle.verify(
      {
        name: 'ECDSA',
        hash: 'SHA-256',
      },
      publicKey,
      signature,
      data
    );
  }

  // Utility Functions
  static bufferToHex(buffer: ArrayBuffer): string {
    return Buffer.from(buffer).toString('hex');
  }

  static hexToBuffer(hex: string): ArrayBuffer {
    return Buffer.from(hex, 'hex').buffer;
  }

  static generateKeyFingerprint(publicKey: ArrayBuffer): string {
    const hash = sha256.create();
    hash.update(Buffer.from(publicKey));
    return hash.hex().slice(0, 16);
  }

  // Large Data Handling
  static async chunkedEncrypt(
    data: Uint8Array,
    key: CryptoKey,
    chunkSize = 1024 * 1024 // 1MB
  ): Promise<{ iv: Uint8Array; chunks: ArrayBuffer[] }> {
    const iv = webcrypto.getRandomValues(new Uint8Array(IV_LENGTH));
    const chunks = [];

    for (let i = 0; i < data.length; i += chunkSize) {
      const chunk = data.slice(i, i + chunkSize);
      const encrypted = await webcrypto.subtle.encrypt(
        { name: 'AES-GCM', iv },
        key,
        chunk
      );
      chunks.push(encrypted);
    }

    return { iv, chunks };
  }
}

// Type Augmentations
declare global {
  interface Window {
    crypto: webcrypto.Crypto;
  }
}

// WebCrypto Polyfill
if (typeof window !== 'undefined' && !window.crypto) {
  window.crypto = webcrypto as unknown as webcrypto.Crypto;
}
