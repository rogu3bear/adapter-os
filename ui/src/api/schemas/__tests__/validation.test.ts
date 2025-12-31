import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { z } from 'zod';
import {
  safeParseApiResponse,
  parseApiResponse,
  safeParseApiArray,
  validateResponse,
  type ValidateResponseOptions,
} from '../validation';

describe('validation', () => {
  // Mock console.error to capture error logs
  const consoleErrorSpy = vi.spyOn(console, 'error');

  beforeEach(() => {
    consoleErrorSpy.mockClear();
  });

  afterEach(() => {
    consoleErrorSpy.mockReset();
  });

  describe('safeParseApiResponse', () => {
    describe('successful validation', () => {
      it('parses valid data and returns typed result', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const data = { id: '123', name: 'Test' };
        const result = safeParseApiResponse(schema, data);

        expect(result).toEqual({ id: '123', name: 'Test' });
      });

      it('parses nested objects correctly', () => {
        const schema = z.object({
          user: z.object({
            firstName: z.string(),
            lastName: z.string(),
          }),
          metadata: z.object({
            createdAt: z.string(),
          }),
        });

        const data = {
          user: { firstName: 'Alice', lastName: 'Smith' },
          metadata: { createdAt: '2025-01-01' },
        };
        const result = safeParseApiResponse(schema, data);

        expect(result).toEqual(data);
        expect(result?.user.firstName).toBe('Alice');
      });

      it('parses arrays within objects', () => {
        const schema = z.object({
          items: z.array(z.object({ id: z.number() })),
          total: z.number(),
        });

        const data = {
          items: [{ id: 1 }, { id: 2 }, { id: 3 }],
          total: 3,
        };
        const result = safeParseApiResponse(schema, data);

        expect(result).toEqual(data);
        expect(result?.items.length).toBe(3);
      });

      it('handles optional fields correctly', () => {
        const schema = z.object({
          required: z.string(),
          optional: z.string().optional(),
        });

        const dataWithOptional = { required: 'value', optional: 'present' };
        const dataWithoutOptional = { required: 'value' };

        expect(safeParseApiResponse(schema, dataWithOptional)).toEqual(dataWithOptional);
        expect(safeParseApiResponse(schema, dataWithoutOptional)).toEqual(dataWithoutOptional);
      });

      it('handles nullable fields correctly', () => {
        const schema = z.object({
          value: z.string().nullable(),
        });

        const withValue = { value: 'test' };
        const withNull = { value: null };

        expect(safeParseApiResponse(schema, withValue)).toEqual(withValue);
        expect(safeParseApiResponse(schema, withNull)).toEqual(withNull);
      });

      it('coerces types when schema allows it', () => {
        const schema = z.object({
          count: z.coerce.number(),
          active: z.coerce.boolean(),
        });

        const data = { count: '42', active: 'true' };
        const result = safeParseApiResponse(schema, data);

        expect(result).toEqual({ count: 42, active: true });
      });
    });

    describe('validation failures', () => {
      it('returns null for invalid data', () => {
        const schema = z.object({
          id: z.string(),
          count: z.number(),
        });

        const invalidData = { id: 123, count: 'not a number' };
        const result = safeParseApiResponse(schema, invalidData);

        expect(result).toBeNull();
      });

      it('returns null for missing required fields', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const incompleteData = { id: '123' };
        const result = safeParseApiResponse(schema, incompleteData);

        expect(result).toBeNull();
      });

      it('returns null for null input', () => {
        const schema = z.object({ id: z.string() });
        const result = safeParseApiResponse(schema, null);

        expect(result).toBeNull();
      });

      it('returns null for undefined input', () => {
        const schema = z.object({ id: z.string() });
        const result = safeParseApiResponse(schema, undefined);

        expect(result).toBeNull();
      });

      it('returns null for primitive input when expecting object', () => {
        const schema = z.object({ id: z.string() });

        expect(safeParseApiResponse(schema, 'string')).toBeNull();
        expect(safeParseApiResponse(schema, 42)).toBeNull();
        expect(safeParseApiResponse(schema, true)).toBeNull();
      });
    });

    describe('error logging', () => {
      it('logs validation errors without context', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        safeParseApiResponse(schema, invalidData);

        expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.any(Array),
            data: invalidData,
          })
        );
      });

      it('logs validation errors with context', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        safeParseApiResponse(schema, invalidData, 'GET /api/users');

        expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (GET /api/users):',
          expect.objectContaining({
            errors: expect.any(Array),
            data: invalidData,
          })
        );
      });

      it('does not log on successful validation', () => {
        const schema = z.object({ id: z.string() });
        const validData = { id: '123' };

        safeParseApiResponse(schema, validData);

        expect(consoleErrorSpy).not.toHaveBeenCalled();
      });

      it('logs detailed error issues', () => {
        const schema = z.object({
          id: z.string(),
          count: z.number().min(0),
        });

        const invalidData = { id: 123, count: -5 };
        safeParseApiResponse(schema, invalidData);

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                path: expect.arrayContaining(['id']),
              }),
              expect.objectContaining({
                path: expect.arrayContaining(['count']),
              }),
            ]),
          })
        );
      });
    });

    describe('type narrowing', () => {
      it('narrows type to schema output on success', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        type Expected = z.infer<typeof schema>;
        const data: unknown = { id: '123', name: 'Test' };
        const result = safeParseApiResponse(schema, data);

        if (result !== null) {
          // TypeScript should allow these accesses
          const id: string = result.id;
          const name: string = result.name;
          expect(id).toBe('123');
          expect(name).toBe('Test');
        }
      });

      it('correctly types discriminated unions', () => {
        const successSchema = z.object({
          status: z.literal('success'),
          data: z.object({ value: z.number() }),
        });

        const errorSchema = z.object({
          status: z.literal('error'),
          message: z.string(),
        });

        const responseSchema = z.union([successSchema, errorSchema]);

        const successData = { status: 'success', data: { value: 42 } };
        const result = safeParseApiResponse(responseSchema, successData);

        expect(result).not.toBeNull();
        if (result && result.status === 'success') {
          expect(result.data.value).toBe(42);
        }
      });
    });
  });

  describe('parseApiResponse', () => {
    describe('successful validation', () => {
      it('parses valid data and returns typed result', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const data = { id: '123', name: 'Test' };
        const result = parseApiResponse(schema, data);

        expect(result).toEqual({ id: '123', name: 'Test' });
      });

      it('handles complex schemas', () => {
        const schema = z.object({
          users: z.array(
            z.object({
              id: z.string(),
              email: z.string().email(),
              role: z.enum(['admin', 'user', 'viewer']),
            })
          ),
          pagination: z.object({
            page: z.number(),
            pageSize: z.number(),
            total: z.number(),
          }),
        });

        const data = {
          users: [
            { id: '1', email: 'admin@example.com', role: 'admin' },
            { id: '2', email: 'user@example.com', role: 'user' },
          ],
          pagination: { page: 1, pageSize: 10, total: 2 },
        };

        const result = parseApiResponse(schema, data);
        expect(result.users.length).toBe(2);
        expect(result.pagination.total).toBe(2);
      });
    });

    describe('validation failures', () => {
      it('throws ZodError for invalid data', () => {
        const schema = z.object({
          id: z.string(),
        });

        const invalidData = { id: 123 };

        expect(() => parseApiResponse(schema, invalidData)).toThrow();
      });

      it('throws ZodError for missing required fields', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const incompleteData = { id: '123' };

        expect(() => parseApiResponse(schema, incompleteData)).toThrow();
      });

      it('throws with specific error message for type mismatch', () => {
        const schema = z.object({
          count: z.number(),
        });

        const invalidData = { count: 'not a number' };

        expect(() => parseApiResponse(schema, invalidData)).toThrow();
      });

      it('logs error before throwing', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        expect(() => parseApiResponse(schema, invalidData)).toThrow();
        expect(consoleErrorSpy).toHaveBeenCalled();
      });

      it('logs error with context before throwing', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        expect(() => parseApiResponse(schema, invalidData, 'POST /api/users')).toThrow();
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (POST /api/users):',
          expect.any(Error)
        );
      });
    });

    describe('error response parsing', () => {
      it('validates API error response format', () => {
        const errorResponseSchema = z.object({
          error: z.object({
            code: z.string(),
            message: z.string(),
            details: z.object({ field: z.string(), reason: z.string() }).optional(),
          }),
        });

        const errorResponse = {
          error: {
            code: 'VALIDATION_ERROR',
            message: 'Invalid input',
            details: { field: 'email', reason: 'Invalid format' },
          },
        };

        const result = parseApiResponse(errorResponseSchema, errorResponse);
        expect(result.error.code).toBe('VALIDATION_ERROR');
        expect(result.error.message).toBe('Invalid input');
      });

      it('throws for malformed error responses', () => {
        const errorResponseSchema = z.object({
          error: z.object({
            code: z.string(),
            message: z.string(),
          }),
        });

        const malformedError = {
          error: {
            code: 123, // should be string
            // missing message
          },
        };

        expect(() => parseApiResponse(errorResponseSchema, malformedError)).toThrow();
      });
    });
  });

  describe('safeParseApiArray', () => {
    describe('successful array parsing', () => {
      it('parses array of valid objects', () => {
        const itemSchema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const data = [
          { id: '1', name: 'First' },
          { id: '2', name: 'Second' },
          { id: '3', name: 'Third' },
        ];

        const result = safeParseApiArray(itemSchema, data);

        expect(result).toHaveLength(3);
        expect(result[0]).toEqual({ id: '1', name: 'First' });
        expect(result[2]).toEqual({ id: '3', name: 'Third' });
      });

      it('handles empty arrays', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, []);

        expect(result).toEqual([]);
      });

      it('handles arrays with single item', () => {
        const itemSchema = z.object({ id: z.string() });
        const data = [{ id: '1' }];

        const result = safeParseApiArray(itemSchema, data);

        expect(result).toHaveLength(1);
        expect(result[0]).toEqual({ id: '1' });
      });
    });

    describe('filtering invalid items', () => {
      it('filters out invalid items and keeps valid ones', () => {
        const itemSchema = z.object({
          id: z.string(),
          count: z.number(),
        });

        const data = [
          { id: '1', count: 10 }, // valid
          { id: '2', count: 'invalid' }, // invalid - count should be number
          { id: '3', count: 30 }, // valid
          { id: 4, count: 40 }, // invalid - id should be string
          { id: '5', count: 50 }, // valid
        ];

        const result = safeParseApiArray(itemSchema, data);

        expect(result).toHaveLength(3);
        expect(result.map((r) => r.id)).toEqual(['1', '3', '5']);
      });

      it('logs errors for each invalid item', () => {
        const itemSchema = z.object({
          id: z.string(),
        });

        const data = [{ id: '1' }, { id: 123 }, { id: '3' }, { id: 456 }];

        safeParseApiArray(itemSchema, data, 'items');

        // Should log errors for indices 1 and 3
        expect(consoleErrorSpy).toHaveBeenCalledTimes(2);
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (items[1]):',
          expect.any(Object)
        );
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (items[3]):',
          expect.any(Object)
        );
      });

      it('returns empty array when all items are invalid', () => {
        const itemSchema = z.object({
          id: z.string(),
        });

        const data = [{ id: 123 }, { id: 456 }, { id: 789 }];

        const result = safeParseApiArray(itemSchema, data);

        expect(result).toEqual([]);
      });
    });

    describe('non-array input handling', () => {
      it('returns empty array for null input', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, null);

        expect(result).toEqual([]);
      });

      it('returns empty array for undefined input', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, undefined);

        expect(result).toEqual([]);
      });

      it('returns empty array for object input', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, { id: '1' });

        expect(result).toEqual([]);
      });

      it('returns empty array for string input', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, 'not an array');

        expect(result).toEqual([]);
      });

      it('returns empty array for number input', () => {
        const itemSchema = z.object({ id: z.string() });
        const result = safeParseApiArray(itemSchema, 42);

        expect(result).toEqual([]);
      });

      it('logs error when input is not an array', () => {
        const itemSchema = z.object({ id: z.string() });

        safeParseApiArray(itemSchema, { id: '1' }, 'users');

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (users): Expected array, got:',
          'object'
        );
      });

      it('logs error without context when input is not an array', () => {
        const itemSchema = z.object({ id: z.string() });

        safeParseApiArray(itemSchema, 'string');

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]: Expected array, got:',
          'string'
        );
      });
    });
  });

  describe('validateResponse', () => {
    describe('standard validation', () => {
      it('validates and returns data when valid', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const data = { id: '123', name: 'Test' };
        const result = validateResponse(schema, data);

        expect(result).toEqual(data);
      });

      it('returns null for invalid data', () => {
        const schema = z.object({
          id: z.string(),
        });

        const invalidData = { id: 123 };
        const result = validateResponse(schema, invalidData);

        expect(result).toBeNull();
      });

      it('uses context for error logging', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        validateResponse(schema, invalidData, { context: 'GET /api/resource' });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (GET /api/resource):',
          expect.any(Object)
        );
      });
    });

    describe('empty response handling with allowEmpty', () => {
      it('returns empty object as-is when allowEmpty is true', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const emptyData = {};
        const result = validateResponse(schema, emptyData, { allowEmpty: true });

        expect(result).toEqual({});
      });

      it('validates non-empty data even when allowEmpty is true', () => {
        const schema = z.object({
          id: z.string(),
        });

        const data = { id: '123' };
        const result = validateResponse(schema, data, { allowEmpty: true });

        expect(result).toEqual({ id: '123' });
      });

      it('validates data normally when allowEmpty is false', () => {
        const schema = z.object({
          id: z.string(),
        });

        const emptyData = {};
        const result = validateResponse(schema, emptyData, { allowEmpty: false });

        expect(result).toBeNull();
      });

      it('validates data normally when allowEmpty is not specified', () => {
        const schema = z.object({
          id: z.string(),
        });

        const emptyData = {};
        const result = validateResponse(schema, emptyData);

        expect(result).toBeNull();
      });
    });

    describe('isEmptyObject detection', () => {
      it('treats {} as empty', () => {
        const schema = z.object({ id: z.string() });
        const result = validateResponse(schema, {}, { allowEmpty: true });
        expect(result).toEqual({});
      });

      it('does not treat null as empty object', () => {
        const schema = z.object({ id: z.string() });
        const result = validateResponse(schema, null, { allowEmpty: true });
        expect(result).toBeNull();
      });

      it('does not treat arrays as empty objects', () => {
        const schema = z.object({ id: z.string() });
        const result = validateResponse(schema, [], { allowEmpty: true });
        expect(result).toBeNull();
      });

      it('does not treat object with properties as empty', () => {
        const schema = z.object({
          id: z.string(),
          name: z.string(),
        });

        const dataWithProps = { id: '123' }; // incomplete but not empty
        const result = validateResponse(schema, dataWithProps, { allowEmpty: true });

        // Should validate, and fail because name is missing
        expect(result).toBeNull();
      });

      it('does not treat primitives as empty objects', () => {
        const schema = z.object({ id: z.string() });

        expect(validateResponse(schema, '', { allowEmpty: true })).toBeNull();
        expect(validateResponse(schema, 0, { allowEmpty: true })).toBeNull();
        expect(validateResponse(schema, false, { allowEmpty: true })).toBeNull();
      });
    });

    describe('options parameter handling', () => {
      it('handles empty options object', () => {
        const schema = z.object({ id: z.string() });
        const data = { id: '123' };

        const result = validateResponse(schema, data, {});

        expect(result).toEqual({ id: '123' });
      });

      it('handles options with only context', () => {
        const schema = z.object({ id: z.string() });
        const invalidData = { id: 123 };

        validateResponse(schema, invalidData, { context: 'test-endpoint' });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (test-endpoint):',
          expect.any(Object)
        );
      });

      it('handles options with only allowEmpty', () => {
        const schema = z.object({ id: z.string() });
        const emptyData = {};

        const result = validateResponse(schema, emptyData, { allowEmpty: true });

        expect(result).toEqual({});
      });

      it('handles options with both context and allowEmpty', () => {
        const schema = z.object({ id: z.string() });

        // Valid empty response
        expect(validateResponse(schema, {}, { allowEmpty: true, context: 'empty-test' })).toEqual(
          {}
        );

        // Invalid non-empty response with context
        validateResponse(schema, { id: 123 }, { allowEmpty: true, context: 'error-test' });
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error] (error-test):',
          expect.any(Object)
        );
      });
    });
  });

  describe('validation error messages', () => {
    describe('Zod error issue formats', () => {
      it('captures invalid_type errors', () => {
        const schema = z.object({
          id: z.string(),
        });

        safeParseApiResponse(schema, { id: 123 });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                code: 'invalid_type',
                expected: 'string',
              }),
            ]),
          })
        );
      });

      it('captures missing field errors', () => {
        const schema = z.object({
          requiredField: z.string(),
        });

        safeParseApiResponse(schema, {});

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                code: 'invalid_type',
                path: ['requiredField'],
              }),
            ]),
          })
        );
      });

      it('captures enum validation errors', () => {
        const schema = z.object({
          status: z.enum(['active', 'inactive', 'pending']),
        });

        safeParseApiResponse(schema, { status: 'unknown' });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                // Zod v4 uses 'invalid_value' for enum validation errors
                code: 'invalid_value',
              }),
            ]),
          })
        );
      });

      it('captures string constraint errors', () => {
        const schema = z.object({
          email: z.string().email(),
          url: z.string().url(),
          uuid: z.string().uuid(),
        });

        safeParseApiResponse(schema, {
          email: 'not-an-email',
          url: 'not-a-url',
          uuid: 'not-a-uuid',
        });

        expect(consoleErrorSpy).toHaveBeenCalled();
        const call = consoleErrorSpy.mock.calls[0];
        const errorData = call[1] as { errors: Array<{ code: string }> };
        expect(errorData.errors.length).toBeGreaterThanOrEqual(3);
      });

      it('captures number constraint errors', () => {
        const schema = z.object({
          positiveNum: z.number().positive(),
          minNum: z.number().min(10),
          maxNum: z.number().max(100),
        });

        safeParseApiResponse(schema, {
          positiveNum: -5,
          minNum: 5,
          maxNum: 150,
        });

        expect(consoleErrorSpy).toHaveBeenCalled();
        const call = consoleErrorSpy.mock.calls[0];
        const errorData = call[1] as { errors: Array<{ code: string }> };
        expect(errorData.errors.length).toBe(3);
      });

      it('captures array length constraint errors', () => {
        const schema = z.object({
          items: z.array(z.string()).min(2).max(5),
        });

        safeParseApiResponse(schema, { items: ['only-one'] });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                code: 'too_small',
                minimum: 2,
              }),
            ]),
          })
        );
      });
    });

    describe('nested path errors', () => {
      it('includes full path for nested object errors', () => {
        const schema = z.object({
          user: z.object({
            profile: z.object({
              email: z.string().email(),
            }),
          }),
        });

        safeParseApiResponse(schema, {
          user: {
            profile: {
              email: 'invalid',
            },
          },
        });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                path: ['user', 'profile', 'email'],
              }),
            ]),
          })
        );
      });

      it('includes array index in path for array item errors', () => {
        const schema = z.object({
          items: z.array(z.object({ id: z.string() })),
        });

        safeParseApiResponse(schema, {
          items: [{ id: '1' }, { id: 2 }, { id: '3' }],
        });

        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[API Validation Error]:',
          expect.objectContaining({
            errors: expect.arrayContaining([
              expect.objectContaining({
                path: ['items', 1, 'id'],
              }),
            ]),
          })
        );
      });
    });
  });

  describe('real-world API response scenarios', () => {
    describe('adapter list response', () => {
      const adapterSchema = z.object({
        adapterId: z.string(),
        name: z.string(),
        status: z.enum(['ready', 'training', 'error']),
        createdAt: z.string(),
      });

      const adapterListSchema = z.object({
        adapters: z.array(adapterSchema),
        total: z.number(),
        page: z.number(),
        pageSize: z.number(),
      });

      it('validates valid adapter list response', () => {
        const response = {
          adapters: [
            { adapterId: 'a-1', name: 'Adapter 1', status: 'ready', createdAt: '2025-01-01' },
            { adapterId: 'a-2', name: 'Adapter 2', status: 'training', createdAt: '2025-01-02' },
          ],
          total: 2,
          page: 1,
          pageSize: 10,
        };

        const result = safeParseApiResponse(adapterListSchema, response);
        expect(result).not.toBeNull();
        expect(result?.adapters.length).toBe(2);
      });

      it('returns null for invalid adapter status', () => {
        const response = {
          adapters: [
            {
              adapterId: 'a-1',
              name: 'Adapter 1',
              status: 'unknown-status',
              createdAt: '2025-01-01',
            },
          ],
          total: 1,
          page: 1,
          pageSize: 10,
        };

        const result = safeParseApiResponse(adapterListSchema, response);
        expect(result).toBeNull();
      });
    });

    describe('training job response', () => {
      const trainingJobSchema = z.object({
        jobId: z.string(),
        status: z.enum(['pending', 'running', 'completed', 'failed']),
        progress: z.number().min(0).max(100),
        startedAt: z.string().optional(),
        completedAt: z.string().optional(),
        error: z.string().optional(),
      });

      it('validates running job', () => {
        const response = {
          jobId: 'job-123',
          status: 'running',
          progress: 45,
          startedAt: '2025-01-01T10:00:00Z',
        };

        const result = safeParseApiResponse(trainingJobSchema, response);
        expect(result).not.toBeNull();
        expect(result?.progress).toBe(45);
      });

      it('validates completed job', () => {
        const response = {
          jobId: 'job-123',
          status: 'completed',
          progress: 100,
          startedAt: '2025-01-01T10:00:00Z',
          completedAt: '2025-01-01T12:00:00Z',
        };

        const result = safeParseApiResponse(trainingJobSchema, response);
        expect(result).not.toBeNull();
        expect(result?.status).toBe('completed');
      });

      it('validates failed job with error', () => {
        const response = {
          jobId: 'job-123',
          status: 'failed',
          progress: 0,
          error: 'Out of memory',
        };

        const result = safeParseApiResponse(trainingJobSchema, response);
        expect(result).not.toBeNull();
        expect(result?.error).toBe('Out of memory');
      });

      it('returns null for invalid progress', () => {
        const response = {
          jobId: 'job-123',
          status: 'running',
          progress: 150, // Invalid: exceeds 100
        };

        const result = safeParseApiResponse(trainingJobSchema, response);
        expect(result).toBeNull();
      });
    });

    describe('inference response', () => {
      const inferResponseSchema = z.object({
        content: z.string(),
        tokensGenerated: z.number(),
        latencyMs: z.number(),
        adaptersUsed: z.array(z.string()),
        metadata: z
          .object({
            modelId: z.string(),
            temperature: z.number(),
          })
          .optional(),
      });

      it('validates inference response with metadata', () => {
        const response = {
          content: 'Generated text here',
          tokensGenerated: 150,
          latencyMs: 450,
          adaptersUsed: ['adapter-1', 'adapter-2'],
          metadata: {
            modelId: 'llama-7b',
            temperature: 0.7,
          },
        };

        const result = safeParseApiResponse(inferResponseSchema, response);
        expect(result).not.toBeNull();
        expect(result?.metadata?.modelId).toBe('llama-7b');
      });

      it('validates inference response without metadata', () => {
        const response = {
          content: 'Generated text here',
          tokensGenerated: 150,
          latencyMs: 450,
          adaptersUsed: [],
        };

        const result = safeParseApiResponse(inferResponseSchema, response);
        expect(result).not.toBeNull();
        expect(result?.metadata).toBeUndefined();
      });
    });

    describe('204 No Content response handling', () => {
      const deleteResponseSchema = z.object({
        success: z.boolean(),
        deletedAt: z.string(),
      });

      it('handles empty response with allowEmpty', () => {
        const emptyResponse = {};
        const result = validateResponse(deleteResponseSchema, emptyResponse, { allowEmpty: true });
        expect(result).toEqual({});
      });

      it('validates actual response data when present', () => {
        const response = { success: true, deletedAt: '2025-01-01T10:00:00Z' };
        const result = validateResponse(deleteResponseSchema, response, { allowEmpty: true });
        expect(result).toEqual(response);
      });
    });
  });
});
