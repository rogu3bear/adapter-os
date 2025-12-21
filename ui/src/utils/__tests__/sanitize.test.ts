import { describe, it, expect } from 'vitest';
import { sanitizeFormValues, getFieldNames } from '@/utils/sanitize';

describe('sanitize', () => {
  describe('sanitizeFormValues', () => {
    it('should redact password fields', () => {
      const input = { email: 'user@example.com', password: 'secret123' };
      const result = sanitizeFormValues(input);

      expect(result.email).toBe('user@example.com');
      expect(result.password).toBe('[REDACTED]');
    });

    it('should redact all sensitive field patterns', () => {
      const input = {
        username: 'john',
        password: 'pass123',
        secret: 'mysecret',
        token: 'jwt-token',
        apiKey: 'key123',
        credential: 'cred123',
        totp: '123456',
      };

      const result = sanitizeFormValues(input);

      expect(result.username).toBe('john');
      expect(result.password).toBe('[REDACTED]');
      expect(result.secret).toBe('[REDACTED]');
      expect(result.token).toBe('[REDACTED]');
      expect(result.apiKey).toBe('[REDACTED]');
      expect(result.credential).toBe('[REDACTED]');
      expect(result.totp).toBe('[REDACTED]');
    });

    it('should handle case-insensitive field matching', () => {
      const input = {
        PASSWORD: 'secret',
        Secret: 'value',
        API_KEY: 'key',
      };

      const result = sanitizeFormValues(input);

      expect(result.PASSWORD).toBe('[REDACTED]');
      expect(result.Secret).toBe('[REDACTED]');
      expect(result.API_KEY).toBe('[REDACTED]');
    });

    it('should handle additional sensitive fields parameter', () => {
      const input = {
        username: 'john',
        sessionId: 'sess123',
        customSecret: 'value',
      };

      const result = sanitizeFormValues(input, ['session', 'custom']);

      expect(result.username).toBe('john');
      expect(result.sessionId).toBe('[REDACTED]');
      expect(result.customSecret).toBe('[REDACTED]');
    });

    it('should handle nested objects', () => {
      const input = {
        user: {
          email: 'user@example.com',
          password: 'secret123',
        },
        metadata: {
          timestamp: '2025-01-01',
          apiKey: 'key123',
        },
      };

      const result = sanitizeFormValues(input);

      expect(result.user).toEqual({
        email: 'user@example.com',
        password: '[REDACTED]',
      });
      expect(result.metadata).toEqual({
        timestamp: '2025-01-01',
        apiKey: '[REDACTED]',
      });
    });

    it('should preserve non-sensitive fields', () => {
      const input = {
        email: 'user@example.com',
        username: 'john',
        age: 30,
        active: true,
      };

      const result = sanitizeFormValues(input);

      expect(result).toEqual(input);
    });

    it('should handle empty objects', () => {
      const input = {};
      const result = sanitizeFormValues(input);

      expect(result).toEqual({});
    });

    it('should handle fields with substring matches', () => {
      const input = {
        userPassword: 'secret',
        confirmPassword: 'secret',
        passwordHash: 'hash',
        resetToken: 'token123',
        authHeader: 'Bearer xyz',
      };

      const result = sanitizeFormValues(input);

      expect(result.userPassword).toBe('[REDACTED]');
      expect(result.confirmPassword).toBe('[REDACTED]');
      expect(result.passwordHash).toBe('[REDACTED]');
      expect(result.resetToken).toBe('[REDACTED]');
      expect(result.authHeader).toBe('[REDACTED]');
    });

    it('should not redact fields with partial non-sensitive matches', () => {
      const input = {
        passport: 'A123456', // Contains 'pass' but not 'password'
      };

      const result = sanitizeFormValues(input);

      // Note: 'passport' does NOT contain 'password' as a substring, so it won't be redacted
      // Our sensitive fields list contains 'password', not 'pass'
      expect(result.passport).toBe('A123456');
    });
  });

  describe('getFieldNames', () => {
    it('should return field names from object', () => {
      const input = { email: 'test', password: 'secret', totp: '123' };
      const result = getFieldNames(input);

      expect(result).toEqual(['email', 'password', 'totp']);
    });

    it('should handle empty objects', () => {
      const input = {};
      const result = getFieldNames(input);

      expect(result).toEqual([]);
    });

    it('should return all keys regardless of sensitivity', () => {
      const input = {
        username: 'john',
        password: 'secret',
        age: 30,
      };
      const result = getFieldNames(input);

      expect(result).toEqual(['username', 'password', 'age']);
    });
  });
});
