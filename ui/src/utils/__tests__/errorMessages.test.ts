import { describe, it, expect } from 'vitest';
import {
  getUserFriendlyError,
  enhanceError,
  isTransientError,
  isNonRetryableError,
  isTimeoutError,
  type ErrorContext,
  type UserFriendlyError,
} from '../errorMessages';

describe('errorMessages', () => {
  describe('getUserFriendlyError', () => {
    describe('error code mapping', () => {
      it('returns correct error for NETWORK_ERROR code', () => {
        const result = getUserFriendlyError('NETWORK_ERROR');
        expect(result.title).toBe('Connection Problem');
        expect(result.variant).toBe('warning');
        expect(result.actionText).toBe('Try Again');
      });

      it('returns correct error for TIMEOUT code', () => {
        const result = getUserFriendlyError('TIMEOUT');
        expect(result.title).toBe('Request Timed Out');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for RATE_LIMIT code', () => {
        const result = getUserFriendlyError('RATE_LIMIT');
        expect(result.title).toBe('Too Many Requests');
        expect(result.variant).toBe('info');
      });

      it('returns correct error for UNAUTHORIZED code', () => {
        const result = getUserFriendlyError('UNAUTHORIZED');
        expect(result.title).toBe('Authentication Required');
        expect(result.actionText).toBe('Log In');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for FORBIDDEN code', () => {
        const result = getUserFriendlyError('FORBIDDEN');
        expect(result.title).toBe('Permission Denied');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for SESSION_EXPIRED code', () => {
        const result = getUserFriendlyError('SESSION_EXPIRED');
        expect(result.title).toBe('Session Expired');
        expect(result.actionText).toBe('Log In');
      });

      it('returns correct error for INSUFFICIENT_MEMORY code', () => {
        const result = getUserFriendlyError('INSUFFICIENT_MEMORY');
        expect(result.title).toBe('Not Enough Memory');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for OUT_OF_MEMORY code', () => {
        const result = getUserFriendlyError('OUT_OF_MEMORY');
        expect(result.title).toBe('Not Enough Memory');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for ADAPTER_NOT_FOUND code', () => {
        const result = getUserFriendlyError('ADAPTER_NOT_FOUND');
        expect(result.title).toBe('Adapter Not Found');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for INTERNAL_SERVER_ERROR code', () => {
        const result = getUserFriendlyError('INTERNAL_SERVER_ERROR');
        expect(result.title).toBe('Server Error');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for SERVICE_UNAVAILABLE code', () => {
        const result = getUserFriendlyError('SERVICE_UNAVAILABLE');
        expect(result.title).toBe('Service Unavailable');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for TRAINING_FAILED code', () => {
        const result = getUserFriendlyError('TRAINING_FAILED');
        expect(result.title).toBe('Training Failed');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for MODEL_NOT_FOUND code', () => {
        const result = getUserFriendlyError('MODEL_NOT_FOUND');
        expect(result.title).toBe('Model Not Found');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for FILE_TOO_LARGE code', () => {
        const result = getUserFriendlyError('FILE_TOO_LARGE');
        expect(result.title).toBe('File Too Large');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for UPLOAD_FAILED code', () => {
        const result = getUserFriendlyError('UPLOAD_FAILED');
        expect(result.title).toBe('Upload Failed');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for NO_WORKERS code', () => {
        const result = getUserFriendlyError('NO_WORKERS');
        expect(result.title).toBe('No Workers Available');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for MAINTENANCE code', () => {
        const result = getUserFriendlyError('MAINTENANCE');
        expect(result.title).toBe('Maintenance In Progress');
        expect(result.variant).toBe('info');
      });

      it('returns correct error for PARSE_ERROR code', () => {
        const result = getUserFriendlyError('PARSE_ERROR');
        expect(result.title).toBe('Invalid Server Response');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for MIGRATION_INVALID code', () => {
        const result = getUserFriendlyError('MIGRATION_INVALID');
        expect(result.title).toBe('Database Migration Error');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for CACHE_STALE code', () => {
        const result = getUserFriendlyError('CACHE_STALE');
        expect(result.title).toBe('Cache Data Stale');
        expect(result.variant).toBe('info');
      });

      it('returns correct error for STREAM_DISCONNECTED code', () => {
        const result = getUserFriendlyError('STREAM_DISCONNECTED');
        expect(result.title).toBe('Stream Disconnected');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for DNS_RESOLUTION_FAILED code', () => {
        const result = getUserFriendlyError('DNS_RESOLUTION_FAILED');
        expect(result.title).toBe('DNS Resolution Failed');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for STORAGE_QUOTA_EXCEEDED code', () => {
        const result = getUserFriendlyError('STORAGE_QUOTA_EXCEEDED');
        expect(result.title).toBe('Storage Quota Exceeded');
        expect(result.variant).toBe('error');
      });
    });

    describe('context-aware messages', () => {
      it('includes retryAfter in RATE_LIMIT message when provided', () => {
        const context: ErrorContext = { retryAfter: 30 };
        const result = getUserFriendlyError('RATE_LIMIT', undefined, context);
        expect(result.message).toContain('30 seconds');
      });

      it('uses generic message for RATE_LIMIT when retryAfter not provided', () => {
        const result = getUserFriendlyError('RATE_LIMIT');
        expect(result.message).toContain('wait a moment');
      });

      it('includes memory details in INSUFFICIENT_MEMORY when provided', () => {
        const context: ErrorContext = { memoryRequired: 2048, memoryAvailable: 1024 };
        const result = getUserFriendlyError('INSUFFICIENT_MEMORY', undefined, context);
        expect(result.message).toContain('2048MB');
        expect(result.message).toContain('1024MB');
      });

      it('uses generic message for INSUFFICIENT_MEMORY when memory details not provided', () => {
        const result = getUserFriendlyError('INSUFFICIENT_MEMORY');
        expect(result.message).toContain('unloading some adapters');
      });

      it('includes adapterId in ADAPTER_NOT_FOUND message when provided', () => {
        const context: ErrorContext = { adapterId: 'my-custom-adapter' };
        const result = getUserFriendlyError('ADAPTER_NOT_FOUND', undefined, context);
        expect(result.message).toContain('my-custom-adapter');
      });

      it('uses generic message for ADAPTER_NOT_FOUND when adapterId not provided', () => {
        const result = getUserFriendlyError('ADAPTER_NOT_FOUND');
        expect(result.message).not.toContain('""');
        expect(result.message).toBe('The requested adapter was not found.');
      });

      it('includes modelId in MODEL_NOT_FOUND message when provided', () => {
        const context: ErrorContext = { modelId: 'llama-7b' };
        const result = getUserFriendlyError('MODEL_NOT_FOUND', undefined, context);
        expect(result.message).toContain('llama-7b');
      });

      it('includes fileSize in FILE_TOO_LARGE message when provided', () => {
        const context: ErrorContext = { fileSize: 104857600 }; // 100MB
        const result = getUserFriendlyError('FILE_TOO_LARGE', undefined, context);
        expect(result.message).toContain('100MB');
      });

      it('includes retryAfter in THUNDERING_HERD_REJECTED when provided', () => {
        const context: ErrorContext = { retryAfter: 5 };
        const result = getUserFriendlyError('THUNDERING_HERD_REJECTED', undefined, context);
        expect(result.message).toContain('5 seconds');
      });

      it('includes retryAfter in CACHE_STALE message when provided', () => {
        const context: ErrorContext = { retryAfter: 10 };
        const result = getUserFriendlyError('CACHE_STALE', undefined, context);
        expect(result.message).toContain('10 seconds');
      });

      it('includes retryAfter in STREAM_DISCONNECTED message when provided', () => {
        const context: ErrorContext = { retryAfter: 3 };
        const result = getUserFriendlyError('STREAM_DISCONNECTED', undefined, context);
        expect(result.message).toContain('3 seconds');
      });
    });

    describe('HTTP status fallback', () => {
      it('returns correct error for 400 status', () => {
        const result = getUserFriendlyError(undefined, 400);
        expect(result.title).toBe('Bad Request');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 401 status', () => {
        const result = getUserFriendlyError(undefined, 401);
        expect(result.title).toBe('Authentication Required');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 403 status', () => {
        const result = getUserFriendlyError(undefined, 403);
        expect(result.title).toBe('Permission Denied');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 404 status', () => {
        const result = getUserFriendlyError(undefined, 404);
        expect(result.title).toBe('Not Found');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 409 status', () => {
        const result = getUserFriendlyError(undefined, 409);
        expect(result.title).toBe('Conflict');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 429 status', () => {
        const result = getUserFriendlyError(undefined, 429);
        expect(result.title).toBe('Too Many Requests');
        expect(result.variant).toBe('info');
      });

      it('returns correct error for 500 status', () => {
        const result = getUserFriendlyError(undefined, 500);
        expect(result.title).toBe('Server Error');
        expect(result.variant).toBe('error');
      });

      it('returns correct error for 502 status', () => {
        const result = getUserFriendlyError(undefined, 502);
        expect(result.title).toBe('Bad Gateway');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 503 status', () => {
        const result = getUserFriendlyError(undefined, 503);
        expect(result.title).toBe('Service Unavailable');
        expect(result.variant).toBe('warning');
      });

      it('returns correct error for 504 status', () => {
        const result = getUserFriendlyError(undefined, 504);
        expect(result.title).toBe('Gateway Timeout');
        expect(result.variant).toBe('warning');
      });

      it('includes retryAfter in 429 message when provided', () => {
        const context: ErrorContext = { retryAfter: 60 };
        const result = getUserFriendlyError(undefined, 429, context);
        expect(result.message).toContain('60 seconds');
      });
    });

    describe('fallback for unknown errors', () => {
      it('returns generic error for unknown error code', () => {
        const result = getUserFriendlyError('UNKNOWN_ERROR_CODE');
        expect(result.title).toBe('Something went wrong');
        expect(result.variant).toBe('error');
        expect(result.actionText).toBe('Try Again');
        expect(result.helpUrl).toBe('/docs/support');
      });

      it('returns generic error for unknown HTTP status', () => {
        const result = getUserFriendlyError(undefined, 418); // I'm a teapot
        expect(result.title).toBe('Something went wrong');
        expect(result.variant).toBe('error');
      });

      it('returns generic error when no code or status provided', () => {
        const result = getUserFriendlyError();
        expect(result.title).toBe('Something went wrong');
        expect(result.variant).toBe('error');
      });

      it('returns generic error for undefined error code and undefined status', () => {
        const result = getUserFriendlyError(undefined, undefined);
        expect(result.title).toBe('Something went wrong');
        expect(result.variant).toBe('error');
      });
    });

    describe('error code takes precedence over HTTP status', () => {
      it('uses error code when both error code and HTTP status are provided', () => {
        const result = getUserFriendlyError('UNAUTHORIZED', 500);
        expect(result.title).toBe('Authentication Required');
        expect(result.variant).toBe('warning');
      });

      it('falls back to HTTP status when error code is unknown', () => {
        const result = getUserFriendlyError('UNKNOWN_CODE', 403);
        expect(result.title).toBe('Permission Denied');
        expect(result.variant).toBe('warning');
      });
    });

    describe('message formatting consistency', () => {
      const allKnownCodes = [
        'NETWORK_ERROR', 'TIMEOUT', 'RATE_LIMIT', 'UNAUTHORIZED', 'FORBIDDEN',
        'SESSION_EXPIRED', 'INSUFFICIENT_MEMORY', 'OUT_OF_MEMORY', 'MIGRATION_INVALID',
        'TRACE_WRITE_FAILED', 'RECEIPT_MISMATCH', 'POLICY_DIVERGENCE', 'BACKEND_FALLBACK',
        'TENANT_ACCESS_DENIED', 'LOAD_FAILED', 'DISK_FULL', 'RESOURCE_BUSY',
        'ADAPTER_NOT_FOUND', 'ADAPTER_ALREADY_LOADED', 'ADAPTER_LOAD_FAILED', 'ADAPTER_CORRUPTED',
        'TRAINING_FAILED', 'DATASET_TRUST_BLOCKED', 'DATASET_TRUST_NEEDS_APPROVAL',
        'INVALID_TRAINING_DATA', 'TRAINING_TIMEOUT',
        'MODEL_NOT_FOUND', 'MODEL_BUSY', 'MODEL_LOAD_FAILED',
        'FILE_TOO_LARGE', 'INVALID_FILE_FORMAT', 'UPLOAD_FAILED',
        'INFERENCE_FAILED', 'INVALID_PROMPT',
        'INTERNAL_SERVER_ERROR', 'SERVICE_UNAVAILABLE', 'SYSTEM_NOT_READY',
        'NO_WORKERS', 'NO_WORKER_AVAILABLE', 'MAINTENANCE',
        'PARSE_ERROR', 'RESPONSE_FORMAT_ERROR',
        'WORKER_UNAVAILABLE', 'LOADING_TIMEOUT', 'INITIAL_LOAD_TIMEOUT',
        'NO_WORKERS_AVAILABLE', 'DRAINING',
        'MIGRATION_FILE_MISSING', 'MIGRATION_CHECKSUM_MISMATCH', 'MIGRATION_OUT_OF_ORDER',
        'DOWN_MIGRATION_BLOCKED', 'SCHEMA_VERSION_MISMATCH', 'SCHEMA_VERSION_AHEAD',
        'CACHE_STALE', 'CACHE_EVICTION', 'CACHE_KEY_NONDETERMINISTIC',
        'CACHE_SERIALIZATION_ERROR', 'CACHE_INVALIDATION_FAILED',
        'RATE_LIMITER_NOT_CONFIGURED', 'INVALID_RATE_LIMIT_CONFIG', 'THUNDERING_HERD_REJECTED',
        'CONFIG_FILE_NOT_FOUND', 'CONFIG_FILE_PERMISSION_DENIED', 'CONFIG_SCHEMA_VIOLATION',
        'EMPTY_ENV_OVERRIDE', 'BLANK_SECRET',
        'TOOLCHAIN_MISMATCH', 'STALE_BUILD_CACHE',
        'DNS_RESOLUTION_FAILED', 'TLS_CERTIFICATE_ERROR', 'PROXY_CONNECTION_FAILED',
        'STREAM_DISCONNECTED', 'BUFFER_OVERFLOW', 'EVENT_GAP_DETECTED',
        'STORAGE_QUOTA_EXCEEDED', 'STATIC_ASSET_NOT_FOUND',
        'CSP_VIOLATION',
        'DEPRECATED_FLAG', 'OUTPUT_FORMAT_MISMATCH', 'INVALID_INPUT_ENCODING', 'INVALID_RETRY_ATTEMPT',
      ];

      it.each(allKnownCodes)('error code %s returns a valid UserFriendlyError', (code) => {
        const result = getUserFriendlyError(code);
        expect(result).toHaveProperty('title');
        expect(result).toHaveProperty('message');
        expect(result).toHaveProperty('variant');
        expect(typeof result.title).toBe('string');
        expect(typeof result.message).toBe('string');
        expect(result.title.length).toBeGreaterThan(0);
        expect(result.message.length).toBeGreaterThan(0);
        expect(['error', 'warning', 'info']).toContain(result.variant);
      });

      it.each(allKnownCodes)('error code %s has actionText', (code) => {
        const result = getUserFriendlyError(code);
        expect(result.actionText).toBeDefined();
        expect(typeof result.actionText).toBe('string');
        expect(result.actionText!.length).toBeGreaterThan(0);
      });

      it.each(allKnownCodes)('error code %s has helpUrl', (code) => {
        const result = getUserFriendlyError(code);
        expect(result.helpUrl).toBeDefined();
        expect(typeof result.helpUrl).toBe('string');
        expect(result.helpUrl).toMatch(/^\/docs\//);
      });

      it('all error titles start with capital letter', () => {
        const codes = allKnownCodes.slice(0, 20); // Test a subset
        for (const code of codes) {
          const result = getUserFriendlyError(code);
          expect(result.title[0]).toBe(result.title[0].toUpperCase());
        }
      });

      it('all error messages end with proper punctuation', () => {
        const codes = allKnownCodes.slice(0, 20); // Test a subset
        for (const code of codes) {
          const result = getUserFriendlyError(code);
          // Messages should end with period or end with dynamic content (like "seconds.")
          expect(result.message).toMatch(/[.!?]$/);
        }
      });
    });
  });

  describe('enhanceError', () => {
    it('creates enhanced error from error with code', () => {
      const originalError = { code: 'UNAUTHORIZED' };
      const result = enhanceError(originalError);

      expect(result).toBeInstanceOf(Error);
      expect(result.name).toBe('UserFriendlyError');
      expect(result.userFriendly.title).toBe('Authentication Required');
      expect(result.originalError).toBe(originalError);
    });

    it('creates enhanced error from error with failure_code', () => {
      const originalError = { failure_code: 'NETWORK_ERROR' };
      const result = enhanceError(originalError) as ReturnType<typeof enhanceError> & { failure_code?: string };

      expect(result.userFriendly.title).toBe('Connection Problem');
      expect(result.failure_code).toBe('NETWORK_ERROR');
    });

    it('prefers failure_code over code', () => {
      const originalError = { failure_code: 'NETWORK_ERROR', code: 'UNAUTHORIZED' };
      const result = enhanceError(originalError);

      expect(result.userFriendly.title).toBe('Connection Problem');
    });

    it('creates enhanced error from error with status', () => {
      const originalError = { status: 404 };
      const result = enhanceError(originalError) as ReturnType<typeof enhanceError> & { status?: number };

      expect(result.userFriendly.title).toBe('Not Found');
      expect(result.status).toBe(404);
    });

    it('uses backend message when more specific than template', () => {
      const originalError = {
        code: 'INTERNAL_SERVER_ERROR',
        message: 'Database connection pool exhausted'
      };
      const result = enhanceError(originalError);

      expect(result.message).toBe('Database connection pool exhausted');
      expect(result.userFriendly.message).toBe('Database connection pool exhausted');
    });

    it('ignores HTTP status messages from backend', () => {
      const originalError = {
        code: 'INTERNAL_SERVER_ERROR',
        message: 'HTTP 500 Internal Server Error'
      };
      const result = enhanceError(originalError);

      // Should not use the HTTP status message
      expect(result.message).not.toContain('HTTP 500');
    });

    it('preserves context in enhanced error', () => {
      const originalError = { code: 'RATE_LIMIT' };
      const context: ErrorContext = { retryAfter: 30 };
      const result = enhanceError(originalError, context);

      expect(result.userFriendly.message).toContain('30 seconds');
    });

    it('handles unknown error gracefully', () => {
      const originalError = { unknownField: 'value' };
      const result = enhanceError(originalError);

      expect(result.userFriendly.title).toBe('Something went wrong');
      expect(result.originalError).toBe(originalError);
    });

    it('throws when passed null error (implementation does not guard against null)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => enhanceError(null)).toThrow();
    });

    it('throws when passed undefined error (implementation does not guard against undefined)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => enhanceError(undefined)).toThrow();
    });

    it('sets code property from original error', () => {
      const originalError = { code: 'ADAPTER_NOT_FOUND', status: 404 };
      const result = enhanceError(originalError) as ReturnType<typeof enhanceError> & { code?: string; status?: number };

      expect(result.code).toBe('ADAPTER_NOT_FOUND');
      expect(result.status).toBe(404);
    });
  });

  describe('isTransientError', () => {
    describe('transient error codes', () => {
      const transientCodes = [
        'NETWORK_ERROR',
        'TIMEOUT',
        'RATE_LIMIT',
        'RESOURCE_BUSY',
        'SERVICE_UNAVAILABLE',
        'MAINTENANCE',
        'STREAM_DISCONNECTED',
        'BUFFER_OVERFLOW',
        'EVENT_GAP_DETECTED',
        'CACHE_STALE',
        'CACHE_EVICTION',
        'CACHE_INVALIDATION_FAILED',
        'THUNDERING_HERD_REJECTED',
        'DNS_RESOLUTION_FAILED',
        'PROXY_CONNECTION_FAILED',
      ];

      it.each(transientCodes)('%s is recognized as transient error', (code) => {
        const error = { code };
        expect(isTransientError(error)).toBe(true);
      });

      it.each(transientCodes)('%s with failure_code is recognized as transient error', (code) => {
        const error = { failure_code: code };
        expect(isTransientError(error)).toBe(true);
      });
    });

    describe('non-transient error codes', () => {
      const nonTransientCodes = [
        'UNAUTHORIZED',
        'FORBIDDEN',
        'ADAPTER_NOT_FOUND',
        'ADAPTER_CORRUPTED',
        'INVALID_FILE_FORMAT',
        'INVALID_PROMPT',
        'BLANK_SECRET',
      ];

      it.each(nonTransientCodes)('%s is not recognized as transient error', (code) => {
        const error = { code };
        expect(isTransientError(error)).toBe(false);
      });
    });

    describe('transient HTTP status codes', () => {
      const transientStatuses = [429, 500, 502, 503, 504];

      it.each(transientStatuses)('status %i is recognized as transient error', (status) => {
        const error = { status };
        expect(isTransientError(error)).toBe(true);
      });
    });

    describe('non-transient HTTP status codes', () => {
      const nonTransientStatuses = [400, 401, 403, 404, 409];

      it.each(nonTransientStatuses)('status %i is not recognized as transient error', (status) => {
        const error = { status };
        expect(isTransientError(error)).toBe(false);
      });
    });

    it('returns false for error without code or status', () => {
      const error = { message: 'Some error' };
      expect(isTransientError(error)).toBe(false);
    });

    it('throws when passed null (implementation does not guard against null)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => isTransientError(null)).toThrow();
    });

    it('throws when passed undefined (implementation does not guard against undefined)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => isTransientError(undefined)).toThrow();
    });

    it('checks both code and status (status takes precedence when transient)', () => {
      // When status is transient (503), the function returns true even if code is not transient
      // This is because the implementation uses || (OR) logic
      const error = { code: 'UNAUTHORIZED', status: 503 };
      expect(isTransientError(error)).toBe(true);
    });

    it('falls back to status when code is not transient but present', () => {
      // Error code check returns false, then status check returns true
      const error = { code: 'UNKNOWN_CODE', status: 503 };
      expect(isTransientError(error)).toBe(true);
    });
  });

  describe('isNonRetryableError', () => {
    describe('non-retryable error codes', () => {
      const nonRetryableCodes = [
        'UNAUTHORIZED',
        'FORBIDDEN',
        'SESSION_EXPIRED',
        'CONFIG_SCHEMA_VIOLATION',
        'INVALID_PROMPT',
        'INVALID_FILE_FORMAT',
        'INVALID_TRAINING_DATA',
        'INVALID_INPUT_ENCODING',
        'ADAPTER_NOT_FOUND',
        'MODEL_NOT_FOUND',
        'ADAPTER_CORRUPTED',
        'MIGRATION_CHECKSUM_MISMATCH',
        'MIGRATION_OUT_OF_ORDER',
        'DOWN_MIGRATION_BLOCKED',
        'BLANK_SECRET',
        'CONFIG_FILE_PERMISSION_DENIED',
        'RATE_LIMITER_NOT_CONFIGURED',
        'POLICY_DIVERGENCE',
        'TENANT_ACCESS_DENIED',
      ];

      it.each(nonRetryableCodes)('%s is recognized as non-retryable error', (code) => {
        const error = { code };
        expect(isNonRetryableError(error)).toBe(true);
      });

      it.each(nonRetryableCodes)('%s with failure_code is recognized as non-retryable error', (code) => {
        const error = { failure_code: code };
        expect(isNonRetryableError(error)).toBe(true);
      });
    });

    describe('retryable error codes', () => {
      const retryableCodes = [
        'NETWORK_ERROR',
        'TIMEOUT',
        'RATE_LIMIT',
        'SERVICE_UNAVAILABLE',
        'MAINTENANCE',
        'STREAM_DISCONNECTED',
      ];

      it.each(retryableCodes)('%s is not recognized as non-retryable error', (code) => {
        const error = { code };
        expect(isNonRetryableError(error)).toBe(false);
      });
    });

    it('returns false for error without code', () => {
      const error = { status: 401 };
      expect(isNonRetryableError(error)).toBe(false);
    });

    it('returns false for unknown error code', () => {
      const error = { code: 'UNKNOWN_ERROR_CODE' };
      expect(isNonRetryableError(error)).toBe(false);
    });

    it('throws when passed null (implementation does not guard against null)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => isNonRetryableError(null)).toThrow();
    });

    it('throws when passed undefined (implementation does not guard against undefined)', () => {
      // The implementation does not guard against null/undefined - this documents actual behavior
      expect(() => isNonRetryableError(undefined)).toThrow();
    });
  });

  describe('isTimeoutError', () => {
    it('returns true for TimeoutError name', () => {
      const error = new Error('Request failed');
      error.name = 'TimeoutError';
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for AbortError name', () => {
      const error = new Error('Request aborted');
      error.name = 'AbortError';
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for message containing "timeout"', () => {
      const error = new Error('Connection timeout');
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for message containing "timed out"', () => {
      const error = new Error('Request timed out');
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for message containing "Request timeout"', () => {
      const error = new Error('Request timeout');
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for message containing "ETIMEDOUT"', () => {
      const error = new Error('connect ETIMEDOUT 10.0.0.1:443');
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns true for message containing "ESOCKETTIMEDOUT"', () => {
      const error = new Error('ESOCKETTIMEDOUT');
      expect(isTimeoutError(error)).toBe(true);
    });

    it('returns false for regular error', () => {
      const error = new Error('Something went wrong');
      expect(isTimeoutError(error)).toBe(false);
    });

    it('returns false for network error', () => {
      const error = new Error('Network request failed');
      expect(isTimeoutError(error)).toBe(false);
    });

    it('returns false for error with unrelated name', () => {
      const error = new Error('Failed');
      error.name = 'NetworkError';
      expect(isTimeoutError(error)).toBe(false);
    });

    it('is case-sensitive for message matching', () => {
      // The implementation uses includes() which is case-sensitive
      const upperCaseError = new Error('TIMEOUT occurred');
      expect(isTimeoutError(upperCaseError)).toBe(false);

      const lowerCaseError = new Error('timeout occurred');
      expect(isTimeoutError(lowerCaseError)).toBe(true);
    });
  });

  describe('error classification interaction', () => {
    it('transient and non-retryable are mutually exclusive for most errors', () => {
      // UNAUTHORIZED is non-retryable but not transient
      const authError = { code: 'UNAUTHORIZED' };
      expect(isTransientError(authError)).toBe(false);
      expect(isNonRetryableError(authError)).toBe(true);

      // NETWORK_ERROR is transient but not non-retryable
      const networkError = { code: 'NETWORK_ERROR' };
      expect(isTransientError(networkError)).toBe(true);
      expect(isNonRetryableError(networkError)).toBe(false);
    });

    it('unknown errors are neither transient nor non-retryable', () => {
      const unknownError = { code: 'COMPLETELY_UNKNOWN_ERROR' };
      expect(isTransientError(unknownError)).toBe(false);
      expect(isNonRetryableError(unknownError)).toBe(false);
    });

    it('errors without code or status are neither transient nor non-retryable', () => {
      const bareError = { message: 'Something happened' };
      expect(isTransientError(bareError)).toBe(false);
      expect(isNonRetryableError(bareError)).toBe(false);
    });
  });
});
