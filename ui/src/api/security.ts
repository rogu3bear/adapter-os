//! Security API Client
//!
//! Provides API methods for security configuration and key management.
//!
//! Citation: CLAUDE.md - Security Settings (JWT Ed25519, key rotation)

import { logger } from '@/utils/logger';

const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

// Security info response
export interface SecurityInfo {
  jwtMode: 'eddsa' | 'hmac';
  keyFingerprint: string;
  tokenTtlMinutes: number;
  createdAt: string;
  lastRotated?: string;
  requireHttps: boolean;
  productionMode: boolean;
}

// JWT configuration
export interface JwtConfig {
  mode: 'eddsa' | 'hmac';
  ttlMinutes: number;
  requireHttps?: boolean;
}

// Key rotation response
export interface KeyRotationResponse {
  success: boolean;
  newFingerprint: string;
  rotatedAt: string;
  message: string;
}

// Error response from API
export interface ApiErrorResponse {
  error: string;
  details?: Record<string, unknown>;
}

/**
 * Get current security configuration
 *
 * Note: This endpoint may not exist yet (404). Handle gracefully with mock data fallback.
 */
export async function getSecurityInfo(): Promise<SecurityInfo> {
  const url = `${API_BASE_URL}/v1/security/info`;

  try {
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'include',
    });

    if (response.status === 404) {
      logger.warn('Security info endpoint not implemented (404), using mock data', {
        component: 'SecurityAPI',
        operation: 'getSecurityInfo',
        url,
      });

      // Mock data fallback for development
      return {
        jwtMode: 'eddsa',
        keyFingerprint: 'ed25519:dev:mock:fingerprint',
        tokenTtlMinutes: 480,
        createdAt: new Date().toISOString(),
        requireHttps: false,
        productionMode: false,
      };
    }

    if (!response.ok) {
      const errorData = await response.json() as ApiErrorResponse;
      throw new Error(errorData.error || `HTTP ${response.status}`);
    }

    const data = await response.json() as SecurityInfo;

    logger.info('Security info fetched', {
      component: 'SecurityAPI',
      operation: 'getSecurityInfo',
      jwtMode: data.jwtMode,
      hasFingerprint: !!data.keyFingerprint,
    });

    return data;
  } catch (error) {
    logger.error('Failed to fetch security info', {
      component: 'SecurityAPI',
      operation: 'getSecurityInfo',
    }, error instanceof Error ? error : new Error(String(error)));

    // Return mock data on any error for development
    return {
      jwtMode: 'eddsa',
      keyFingerprint: 'ed25519:dev:mock:fingerprint',
      tokenTtlMinutes: 480,
      createdAt: new Date().toISOString(),
      requireHttps: false,
      productionMode: false,
    };
  }
}

/**
 * Update JWT configuration
 *
 * Note: This endpoint may not exist yet (404). Handle gracefully.
 */
export async function updateJwtConfig(config: JwtConfig): Promise<SecurityInfo> {
  const url = `${API_BASE_URL}/v1/security/jwt-config`;

  try {
    const response = await fetch(url, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'include',
      body: JSON.stringify(config),
    });

    if (response.status === 404) {
      logger.warn('JWT config endpoint not implemented (404)', {
        component: 'SecurityAPI',
        operation: 'updateJwtConfig',
        url,
      });

      throw new Error('JWT configuration endpoint not yet implemented');
    }

    if (!response.ok) {
      const errorData = await response.json() as ApiErrorResponse;
      throw new Error(errorData.error || `HTTP ${response.status}`);
    }

    const data = await response.json() as SecurityInfo;

    logger.info('JWT config updated', {
      component: 'SecurityAPI',
      operation: 'updateJwtConfig',
      mode: config.mode,
      ttl: config.ttlMinutes,
    });

    return data;
  } catch (error) {
    logger.error('Failed to update JWT config', {
      component: 'SecurityAPI',
      operation: 'updateJwtConfig',
    }, error instanceof Error ? error : new Error(String(error)));

    throw error;
  }
}

/**
 * Rotate Ed25519 signing keys
 *
 * Note: This endpoint may not exist yet (404). Handle gracefully.
 */
export async function rotateKeys(): Promise<KeyRotationResponse> {
  const url = `${API_BASE_URL}/v1/security/keys/rotate`;

  try {
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'include',
    });

    if (response.status === 404) {
      logger.warn('Key rotation endpoint not implemented (404)', {
        component: 'SecurityAPI',
        operation: 'rotateKeys',
        url,
      });

      throw new Error('Key rotation endpoint not yet implemented');
    }

    if (!response.ok) {
      const errorData = await response.json() as ApiErrorResponse;
      throw new Error(errorData.error || `HTTP ${response.status}`);
    }

    const data = await response.json() as KeyRotationResponse;

    logger.info('Keys rotated successfully', {
      component: 'SecurityAPI',
      operation: 'rotateKeys',
      newFingerprint: data.newFingerprint,
    });

    return data;
  } catch (error) {
    logger.error('Failed to rotate keys', {
      component: 'SecurityAPI',
      operation: 'rotateKeys',
    }, error instanceof Error ? error : new Error(String(error)));

    throw error;
  }
}
