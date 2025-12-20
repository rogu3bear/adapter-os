import { describe, it, expect } from 'vitest';
import { toCamelCase, toSnakeCase } from '@/api/transformers';

describe('API Transformers', () => {
  describe('toCamelCase', () => {
    it('transforms snake_case to camelCase', () => {
      const input = {
        user_id: 1,
        first_name: 'John',
        last_name: 'Doe',
      };

      const result = toCamelCase(input);

      expect(result).toEqual({
        userId: 1,
        firstName: 'John',
        lastName: 'Doe',
      });
    });

    it('handles nested objects', () => {
      const input = {
        user_data: {
          first_name: 'John',
          contact_info: {
            email_address: 'john@example.com',
          },
        },
      };

      const result = toCamelCase(input);

      expect(result).toEqual({
        userData: {
          firstName: 'John',
          contactInfo: {
            emailAddress: 'john@example.com',
          },
        },
      });
    });

    it('handles arrays of objects', () => {
      const input = {
        user_list: [
          { user_id: 1, first_name: 'John' },
          { user_id: 2, first_name: 'Jane' },
        ],
      };

      const result = toCamelCase(input);

      expect(result).toEqual({
        userList: [
          { userId: 1, firstName: 'John' },
          { userId: 2, firstName: 'Jane' },
        ],
      });
    });

    it('preserves null and undefined', () => {
      const input = {
        null_value: null,
        undefined_value: undefined,
      };

      const result = toCamelCase(input);

      expect(result).toEqual({
        nullValue: null,
        undefinedValue: undefined,
      });
    });

    it('preserves Date objects', () => {
      const date = new Date('2024-01-01');
      const input = {
        created_at: date,
      };

      const result = toCamelCase(input);

      expect(result.createdAt).toBe(date);
      expect(result.createdAt).toBeInstanceOf(Date);
    });

    it('handles empty objects', () => {
      const result = toCamelCase({});
      expect(result).toEqual({});
    });

    it('handles primitives', () => {
      expect(toCamelCase(null)).toBe(null);
      expect(toCamelCase(undefined)).toBe(undefined);
      expect(toCamelCase(42)).toBe(42);
      expect(toCamelCase('string')).toBe('string');
    });
  });

  describe('toSnakeCase', () => {
    it('transforms camelCase to snake_case', () => {
      const input = {
        userId: 1,
        firstName: 'John',
        lastName: 'Doe',
      };

      const result = toSnakeCase(input);

      expect(result).toEqual({
        user_id: 1,
        first_name: 'John',
        last_name: 'Doe',
      });
    });

    it('handles nested objects', () => {
      const input = {
        userData: {
          firstName: 'John',
          contactInfo: {
            emailAddress: 'john@example.com',
          },
        },
      };

      const result = toSnakeCase(input);

      expect(result).toEqual({
        user_data: {
          first_name: 'John',
          contact_info: {
            email_address: 'john@example.com',
          },
        },
      });
    });

    it('handles arrays of objects', () => {
      const input = {
        userList: [
          { userId: 1, firstName: 'John' },
          { userId: 2, firstName: 'Jane' },
        ],
      };

      const result = toSnakeCase(input);

      expect(result).toEqual({
        user_list: [
          { user_id: 1, first_name: 'John' },
          { user_id: 2, first_name: 'Jane' },
        ],
      });
    });

    it('preserves null and undefined', () => {
      const input = {
        nullValue: null,
        undefinedValue: undefined,
      };

      const result = toSnakeCase(input);

      expect(result).toEqual({
        null_value: null,
        undefined_value: undefined,
      });
    });

    it('preserves Date objects', () => {
      const date = new Date('2024-01-01');
      const input = {
        createdAt: date,
      };

      const result = toSnakeCase(input);

      expect(result.created_at).toBe(date);
      expect(result.created_at).toBeInstanceOf(Date);
    });

    it('handles empty objects', () => {
      const result = toSnakeCase({});
      expect(result).toEqual({});
    });

    it('handles primitives', () => {
      expect(toSnakeCase(null)).toBe(null);
      expect(toSnakeCase(undefined)).toBe(undefined);
      expect(toSnakeCase(42)).toBe(42);
      expect(toSnakeCase('string')).toBe('string');
    });
  });

  describe('roundtrip transformations', () => {
    it('snake -> camel -> snake preserves structure', () => {
      const original = {
        user_id: 1,
        user_data: {
          first_name: 'John',
          last_name: 'Doe',
        },
      };

      const camel = toCamelCase(original);
      const backToSnake = toSnakeCase(camel);

      expect(backToSnake).toEqual(original);
    });

    it('camel -> snake -> camel preserves structure', () => {
      const original = {
        userId: 1,
        userData: {
          firstName: 'John',
          lastName: 'Doe',
        },
      };

      const snake = toSnakeCase(original);
      const backToCamel = toCamelCase(snake);

      expect(backToCamel).toEqual(original);
    });
  });
});
