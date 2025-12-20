import { describe, it, expect } from 'vitest';
import {
  toCamelCase,
  toSnakeCase,
  transformAndValidate,
  prepareRequest,
  toCamelCaseBatch,
  toSnakeCaseBatch,
  createTransformer,
  isSnakeCase,
  isCamelCase,
} from '@/api/transformers';
import { z } from 'zod';

describe('transformers', () => {
  describe('toCamelCase', () => {
    describe('string key transformation', () => {
      it('transforms simple snake_case keys', () => {
        const input = { user_name: 'alice' };
        const result = toCamelCase(input);
        expect(result).toEqual({ userName: 'alice' });
      });

      it('transforms multiple snake_case keys', () => {
        const input = {
          user_name: 'alice',
          created_at: '2025-01-01',
          is_active: true,
        };
        const result = toCamelCase(input);
        expect(result).toEqual({
          userName: 'alice',
          createdAt: '2025-01-01',
          isActive: true,
        });
      });

      it('transforms keys with numbers following underscore', () => {
        const input = { user_id_123: 'test', created_at_2025: 'value' };
        const result = toCamelCase(input);
        // Note: includes numbers in the transformation
        expect(result).toEqual({
          userId123: 'test',
          createdAt2025: 'value',
        });
      });

      it('handles single character keys', () => {
        const input = { a: 1, b_c: 2 };
        const result = toCamelCase(input);
        expect(result).toEqual({ a: 1, bC: 2 });
      });

      it('handles empty keys', () => {
        const input = { '': 'empty' };
        const result = toCamelCase(input);
        expect(result).toEqual({ '': 'empty' });
      });

      it('handles already camelCase keys', () => {
        const input = { userName: 'alice', userId: 123 };
        const result = toCamelCase(input);
        // Keys are transformed regardless of current format
        expect(result).toEqual({ userName: 'alice', userId: 123 });
      });
    });

    describe('nested object transformation', () => {
      it('transforms nested objects', () => {
        const input = {
          user_data: {
            first_name: 'Alice',
            last_name: 'Smith',
          },
        };
        const result = toCamelCase(input);
        expect(result.userData.firstName).toBe('Alice');
        expect(result.userData.lastName).toBe('Smith');
      });

      it('transforms deeply nested objects', () => {
        const input = {
          level_one: {
            level_two: {
              level_three: {
                deep_value: 'nested',
              },
            },
          },
        };
        const result = toCamelCase(input);
        expect(result.levelOne.levelTwo.levelThree.deepValue).toBe('nested');
      });

      it('transforms mixed nested structures', () => {
        const input = {
          simple_key: 'value',
          nested_obj: {
            inner_key: 'inner',
          },
          nested_array: [{ item_id: '1' }],
        };
        const result = toCamelCase(input);
        expect(result.simpleKey).toBe('value');
        expect(result.nestedObj.innerKey).toBe('inner');
        expect(result.nestedArray[0].itemId).toBe('1');
      });
    });

    describe('array transformation', () => {
      it('transforms arrays of objects', () => {
        const input = {
          items: [
            { item_id: '1', item_name: 'first' },
            { item_id: '2', item_name: 'second' },
          ],
        };
        const result = toCamelCase(input);
        expect(result.items[0].itemId).toBe('1');
        expect(result.items[0].itemName).toBe('first');
        expect(result.items[1].itemId).toBe('2');
        expect(result.items[1].itemName).toBe('second');
      });

      it('transforms nested arrays', () => {
        const input = {
          matrix: [[{ cell_value: 1 }]],
        };
        const result = toCamelCase(input);
        expect(result.matrix[0][0].cellValue).toBe(1);
      });

      it('transforms arrays with mixed types', () => {
        const input = {
          mixed_array: ['string', 123, { nested_key: 'value' }, null, true],
        };
        const result = toCamelCase(input);
        expect(result.mixedArray[0]).toBe('string');
        expect(result.mixedArray[1]).toBe(123);
        expect((result.mixedArray[2] as { nestedKey: string }).nestedKey).toBe('value');
        expect(result.mixedArray[3]).toBeNull();
        expect(result.mixedArray[4]).toBe(true);
      });

      it('handles empty arrays', () => {
        const input = { empty_array: [] };
        const result = toCamelCase(input);
        expect(result.emptyArray).toEqual([]);
      });

      it('does not transform string values in arrays', () => {
        const input = {
          tags: ['admin_user', 'power_user'],
        };
        const result = toCamelCase(input);
        // String values are not transformed, only object keys
        expect(result.tags).toEqual(['admin_user', 'power_user']);
      });
    });

    describe('primitive value handling', () => {
      it('handles null', () => {
        const result = toCamelCase(null);
        expect(result).toBeNull();
      });

      it('handles undefined', () => {
        const result = toCamelCase(undefined);
        expect(result).toBeUndefined();
      });

      it('handles boolean values', () => {
        const input = { is_active: true, is_deleted: false };
        const result = toCamelCase(input);
        expect(result.isActive).toBe(true);
        expect(result.isDeleted).toBe(false);
      });

      it('handles number values', () => {
        const input = { user_count: 42, price_value: 3.14 };
        const result = toCamelCase(input);
        expect(result.userCount).toBe(42);
        expect(result.priceValue).toBe(3.14);
      });

      it('handles string values', () => {
        const input = { user_name: 'alice', empty_string: '' };
        const result = toCamelCase(input);
        expect(result.userName).toBe('alice');
        expect(result.emptyString).toBe('');
      });

      it('handles null values in objects', () => {
        const input = { nullable_field: null };
        const result = toCamelCase(input);
        expect(result.nullableField).toBeNull();
      });

      it('handles undefined values in objects', () => {
        const input = { undefined_field: undefined };
        const result = toCamelCase(input);
        expect(result.undefinedField).toBeUndefined();
      });
    });

    describe('special object types', () => {
      it('preserves Date objects', () => {
        const date = new Date('2025-01-01T00:00:00Z');
        const input = { created_at: date };
        const result = toCamelCase(input);
        expect(result.createdAt).toBe(date);
        expect(result.createdAt).toBeInstanceOf(Date);
      });

      it('preserves File objects', () => {
        const file = new File(['content'], 'test.txt', { type: 'text/plain' });
        const input = { uploaded_file: file };
        const result = toCamelCase(input);
        expect(result.uploadedFile).toBe(file);
        expect(result.uploadedFile).toBeInstanceOf(File);
      });

      it('preserves Blob objects', () => {
        const blob = new Blob(['content'], { type: 'text/plain' });
        const input = { data_blob: blob };
        const result = toCamelCase(input);
        expect(result.dataBlob).toBe(blob);
        expect(result.dataBlob).toBeInstanceOf(Blob);
      });

      it('preserves FormData objects', () => {
        const formData = new FormData();
        formData.append('test', 'value');
        const input = { form_data: formData };
        const result = toCamelCase(input);
        expect(result.formData).toBe(formData);
        expect(result.formData).toBeInstanceOf(FormData);
      });

      it('preserves RegExp objects', () => {
        const regex = /test/gi;
        const input = { pattern: regex };
        const result = toCamelCase(input);
        expect(result.pattern).toBe(regex);
        expect(result.pattern).toBeInstanceOf(RegExp);
      });

      it('preserves Map objects', () => {
        const map = new Map([['key', 'value']]);
        const input = { data_map: map };
        const result = toCamelCase(input);
        expect(result.dataMap).toBe(map);
        expect(result.dataMap).toBeInstanceOf(Map);
      });

      it('preserves Set objects', () => {
        const set = new Set([1, 2, 3]);
        const input = { data_set: set };
        const result = toCamelCase(input);
        expect(result.dataSet).toBe(set);
        expect(result.dataSet).toBeInstanceOf(Set);
      });
    });

    describe('edge cases', () => {
      it('handles empty objects', () => {
        const input = {};
        const result = toCamelCase(input);
        expect(result).toEqual({});
      });

      it('handles objects with single key', () => {
        const input = { single_key: 'value' };
        const result = toCamelCase(input);
        expect(result).toEqual({ singleKey: 'value' });
      });

      it('handles objects with numeric keys', () => {
        const input = { '123': 'numeric', user_name: 'alice' };
        const result = toCamelCase(input);
        expect(result['123']).toBe('numeric');
        expect(result.userName).toBe('alice');
      });

      it('handles very long snake_case keys', () => {
        const input = {
          this_is_a_very_long_key_with_many_underscores_in_it: 'value',
        };
        const result = toCamelCase(input);
        expect(result.thisIsAVeryLongKeyWithManyUnderscoresInIt).toBe('value');
      });
    });

    describe('real-world API response scenarios', () => {
      it('transforms typical user response', () => {
        const input = {
          user_id: '123',
          user_name: 'alice',
          email_address: 'alice@example.com',
          created_at: '2025-01-01',
          is_active: true,
        };
        const result = toCamelCase(input);
        expect(result).toEqual({
          userId: '123',
          userName: 'alice',
          emailAddress: 'alice@example.com',
          createdAt: '2025-01-01',
          isActive: true,
        });
      });

      it('transforms paginated list response', () => {
        const input = {
          items: [
            { adapter_id: '1', adapter_name: 'first' },
            { adapter_id: '2', adapter_name: 'second' },
          ],
          total_count: 2,
          page_size: 10,
          current_page: 1,
        };
        const result = toCamelCase(input);
        expect(result.items.length).toBe(2);
        expect(result.items[0].adapterId).toBe('1');
        expect(result.totalCount).toBe(2);
        expect(result.pageSize).toBe(10);
        expect(result.currentPage).toBe(1);
      });

      it('transforms nested metadata response', () => {
        const input = {
          adapter_id: 'abc',
          metadata: {
            training_params: {
              learning_rate: 0.001,
              batch_size: 32,
            },
            model_info: {
              base_model: 'qwen',
              adapter_version: '1.0',
            },
          },
        };
        const result = toCamelCase(input);
        expect(result.adapterId).toBe('abc');
        expect(result.metadata.trainingParams.learningRate).toBe(0.001);
        expect(result.metadata.trainingParams.batchSize).toBe(32);
        expect(result.metadata.modelInfo.baseModel).toBe('qwen');
        expect(result.metadata.modelInfo.adapterVersion).toBe('1.0');
      });
    });
  });

  describe('toSnakeCase', () => {
    describe('string key transformation', () => {
      it('transforms simple camelCase keys', () => {
        const input = { userName: 'alice' };
        const result = toSnakeCase(input);
        expect(result).toEqual({ user_name: 'alice' });
      });

      it('transforms multiple camelCase keys', () => {
        const input = {
          userName: 'alice',
          createdAt: '2025-01-01',
          isActive: true,
        };
        const result = toSnakeCase(input);
        expect(result).toEqual({
          user_name: 'alice',
          created_at: '2025-01-01',
          is_active: true,
        });
      });

      it('transforms keys with consecutive capitals', () => {
        const input = { userID: '123', apiURL: 'http://example.com' };
        const result = toSnakeCase(input);
        expect(result).toEqual({
          user_i_d: '123',
          api_u_r_l: 'http://example.com',
        });
      });

      it('handles single character keys', () => {
        const input = { a: 1, bC: 2 };
        const result = toSnakeCase(input);
        expect(result).toEqual({ a: 1, b_c: 2 });
      });

      it('handles already snake_case keys', () => {
        const input = { user_name: 'alice', user_id: 123 };
        const result = toSnakeCase(input);
        // Keys are transformed regardless of current format
        expect(result).toEqual({ user_name: 'alice', user_id: 123 });
      });

      it('handles mixed case with numbers', () => {
        const input = { userId123: 'test', apiKey2: 'key' };
        const result = toSnakeCase(input);
        expect(result).toEqual({
          user_id123: 'test',
          api_key2: 'key',
        });
      });
    });

    describe('nested object transformation', () => {
      it('transforms nested objects', () => {
        const input = {
          userData: {
            firstName: 'Alice',
            lastName: 'Smith',
          },
        };
        const result = toSnakeCase(input);
        expect(result.user_data.first_name).toBe('Alice');
        expect(result.user_data.last_name).toBe('Smith');
      });

      it('transforms deeply nested objects', () => {
        const input = {
          levelOne: {
            levelTwo: {
              levelThree: {
                deepValue: 'nested',
              },
            },
          },
        };
        const result = toSnakeCase(input);
        expect(result.level_one.level_two.level_three.deep_value).toBe('nested');
      });
    });

    describe('array transformation', () => {
      it('transforms arrays of objects', () => {
        const input = {
          items: [
            { itemId: '1', itemName: 'first' },
            { itemId: '2', itemName: 'second' },
          ],
        };
        const result = toSnakeCase(input);
        expect(result.items[0].item_id).toBe('1');
        expect(result.items[0].item_name).toBe('first');
        expect(result.items[1].item_id).toBe('2');
        expect(result.items[1].item_name).toBe('second');
      });

      it('transforms nested arrays', () => {
        const input = {
          matrix: [[{ cellValue: 1 }]],
        };
        const result = toSnakeCase(input);
        expect(result.matrix[0][0].cell_value).toBe(1);
      });

      it('handles empty arrays', () => {
        const input = { emptyArray: [] };
        const result = toSnakeCase(input);
        expect(result.empty_array).toEqual([]);
      });
    });

    describe('primitive value handling', () => {
      it('handles null', () => {
        const result = toSnakeCase(null);
        expect(result).toBeNull();
      });

      it('handles undefined', () => {
        const result = toSnakeCase(undefined);
        expect(result).toBeUndefined();
      });

      it('handles boolean values', () => {
        const input = { isActive: true, isDeleted: false };
        const result = toSnakeCase(input);
        expect(result.is_active).toBe(true);
        expect(result.is_deleted).toBe(false);
      });

      it('handles number values', () => {
        const input = { userCount: 42, priceValue: 3.14 };
        const result = toSnakeCase(input);
        expect(result.user_count).toBe(42);
        expect(result.price_value).toBe(3.14);
      });
    });

    describe('special object types', () => {
      it('preserves Date objects', () => {
        const date = new Date('2025-01-01T00:00:00Z');
        const input = { createdAt: date };
        const result = toSnakeCase(input);
        expect(result.created_at).toBe(date);
        expect(result.created_at).toBeInstanceOf(Date);
      });

      it('preserves File objects', () => {
        const file = new File(['content'], 'test.txt', { type: 'text/plain' });
        const input = { uploadedFile: file };
        const result = toSnakeCase(input);
        expect(result.uploaded_file).toBe(file);
        expect(result.uploaded_file).toBeInstanceOf(File);
      });

      it('preserves Blob objects', () => {
        const blob = new Blob(['content'], { type: 'text/plain' });
        const input = { dataBlob: blob };
        const result = toSnakeCase(input);
        expect(result.data_blob).toBe(blob);
        expect(result.data_blob).toBeInstanceOf(Blob);
      });
    });

    describe('edge cases', () => {
      it('handles empty objects', () => {
        const input = {};
        const result = toSnakeCase(input);
        expect(result).toEqual({});
      });

      it('handles very long camelCase keys', () => {
        const input = {
          thisIsAVeryLongKeyWithManyWordsInIt: 'value',
        };
        const result = toSnakeCase(input);
        expect(result.this_is_a_very_long_key_with_many_words_in_it).toBe('value');
      });
    });

    describe('real-world API request scenarios', () => {
      it('transforms typical create user request', () => {
        const input = {
          userName: 'alice',
          emailAddress: 'alice@example.com',
          isActive: true,
        };
        const result = toSnakeCase(input);
        expect(result).toEqual({
          user_name: 'alice',
          email_address: 'alice@example.com',
          is_active: true,
        });
      });

      it('transforms training parameters request', () => {
        const input = {
          adapterId: 'abc',
          trainingParams: {
            learningRate: 0.001,
            batchSize: 32,
            maxSteps: 1000,
          },
        };
        const result = toSnakeCase(input);
        expect(result.adapter_id).toBe('abc');
        expect(result.training_params.learning_rate).toBe(0.001);
        expect(result.training_params.batch_size).toBe(32);
        expect(result.training_params.max_steps).toBe(1000);
      });
    });
  });

  describe('transformAndValidate', () => {
    it('validates and transforms valid data', () => {
      const schema = z.object({
        user_name: z.string(),
        user_id: z.number(),
      });

      const data = {
        user_name: 'alice',
        user_id: 123,
      };

      const result = transformAndValidate(schema, data);
      expect(result).toEqual({
        userName: 'alice',
        userId: 123,
      });
    });

    it('throws on invalid data', () => {
      const schema = z.object({
        user_name: z.string(),
        user_id: z.number(),
      });

      const invalidData = {
        user_name: 'alice',
        // missing user_id
      };

      expect(() => transformAndValidate(schema, invalidData)).toThrow();
    });

    it('transforms nested validated data', () => {
      const schema = z.object({
        user_data: z.object({
          first_name: z.string(),
          last_name: z.string(),
        }),
      });

      const data = {
        user_data: {
          first_name: 'Alice',
          last_name: 'Smith',
        },
      };

      const result = transformAndValidate(schema, data);
      expect(result.userData.firstName).toBe('Alice');
      expect(result.userData.lastName).toBe('Smith');
    });
  });

  describe('prepareRequest', () => {
    it('transforms data without validation', () => {
      const data = {
        userName: 'alice',
        userId: 123,
      };

      const result = prepareRequest(data);
      expect(result).toEqual({
        user_name: 'alice',
        user_id: 123,
      });
    });

    it('validates and transforms when schema provided', () => {
      const schema = z.object({
        userName: z.string(),
        userId: z.number(),
      });

      const data = {
        userName: 'alice',
        userId: 123,
      };

      const result = prepareRequest(data, schema);
      expect(result).toEqual({
        user_name: 'alice',
        user_id: 123,
      });
    });

    it('throws when schema validation fails', () => {
      const schema = z.object({
        userName: z.string(),
        userId: z.number(),
      });

      const invalidData = {
        userName: 'alice',
        // missing userId
      };

      expect(() => prepareRequest(invalidData, schema)).toThrow();
    });
  });

  describe('batch transformers', () => {
    describe('toCamelCaseBatch', () => {
      it('transforms array of objects', () => {
        const input = [
          { user_id: '1', user_name: 'alice' },
          { user_id: '2', user_name: 'bob' },
        ];

        const result = toCamelCaseBatch(input);
        expect(result).toEqual([
          { userId: '1', userName: 'alice' },
          { userId: '2', userName: 'bob' },
        ]);
      });

      it('handles empty array', () => {
        const result = toCamelCaseBatch([]);
        expect(result).toEqual([]);
      });
    });

    describe('toSnakeCaseBatch', () => {
      it('transforms array of objects', () => {
        const input = [
          { userId: '1', userName: 'alice' },
          { userId: '2', userName: 'bob' },
        ];

        const result = toSnakeCaseBatch(input);
        expect(result).toEqual([
          { user_id: '1', user_name: 'alice' },
          { user_id: '2', user_name: 'bob' },
        ]);
      });

      it('handles empty array', () => {
        const result = toSnakeCaseBatch([]);
        expect(result).toEqual([]);
      });
    });
  });

  describe('createTransformer', () => {
    it('creates reusable transformer', () => {
      type User = {
        user_id: number;
        user_name: string;
      };

      const transformer = createTransformer<User>();

      const apiData = { user_id: 1, user_name: 'alice' };
      const camelCased = transformer.toCamelCase(apiData);
      expect(camelCased).toEqual({ userId: 1, userName: 'alice' });

      const backToSnake = transformer.toSnakeCase(camelCased);
      expect(backToSnake).toEqual({ user_id: 1, user_name: 'alice' });
    });

    it('supports batch transformations', () => {
      type User = {
        user_id: number;
      };

      const transformer = createTransformer<User>();

      const users = [{ user_id: 1 }, { user_id: 2 }];
      const camelCased = transformer.toCamelCaseBatch(users);
      expect(camelCased).toEqual([{ userId: 1 }, { userId: 2 }]);
    });
  });

  describe('type guards', () => {
    describe('isSnakeCase', () => {
      it('returns true for snake_case objects', () => {
        expect(isSnakeCase({ user_name: 'alice' })).toBe(true);
        expect(isSnakeCase({ user_id: 1, created_at: '2025' })).toBe(true);
      });

      it('returns false for camelCase objects', () => {
        expect(isSnakeCase({ userName: 'alice' })).toBe(false);
        expect(isSnakeCase({ userId: 1 })).toBe(false);
      });

      it('returns false for mixed case objects', () => {
        expect(isSnakeCase({ user_id: 1, userName: 'alice' })).toBe(false);
      });

      it('returns false for non-objects', () => {
        expect(isSnakeCase(null)).toBe(false);
        expect(isSnakeCase(undefined)).toBe(false);
        expect(isSnakeCase('string')).toBe(false);
        expect(isSnakeCase(123)).toBe(false);
      });

      it('checks nested objects recursively', () => {
        expect(
          isSnakeCase({
            user_data: {
              first_name: 'alice',
            },
          })
        ).toBe(true);

        expect(
          isSnakeCase({
            user_data: {
              firstName: 'alice', // nested camelCase
            },
          })
        ).toBe(false);
      });

      it('checks arrays of objects', () => {
        expect(
          isSnakeCase({
            items: [{ item_id: 1 }, { item_id: 2 }],
          })
        ).toBe(true);

        expect(
          isSnakeCase({
            items: [{ itemId: 1 }, { itemId: 2 }],
          })
        ).toBe(false);
      });
    });

    describe('isCamelCase', () => {
      it('returns true for camelCase objects', () => {
        expect(isCamelCase({ userName: 'alice' })).toBe(true);
        expect(isCamelCase({ userId: 1, createdAt: '2025' })).toBe(true);
      });

      it('returns false for snake_case objects', () => {
        expect(isCamelCase({ user_name: 'alice' })).toBe(false);
        expect(isCamelCase({ user_id: 1 })).toBe(false);
      });

      it('returns false for mixed case objects', () => {
        expect(isCamelCase({ userId: 1, user_name: 'alice' })).toBe(false);
      });

      it('returns false for non-objects', () => {
        expect(isCamelCase(null)).toBe(false);
        expect(isCamelCase(undefined)).toBe(false);
        expect(isCamelCase('string')).toBe(false);
        expect(isCamelCase(123)).toBe(false);
      });

      it('checks nested objects recursively', () => {
        expect(
          isCamelCase({
            userData: {
              firstName: 'alice',
            },
          })
        ).toBe(true);

        expect(
          isCamelCase({
            userData: {
              first_name: 'alice', // nested snake_case
            },
          })
        ).toBe(false);
      });

      it('checks arrays of objects', () => {
        expect(
          isCamelCase({
            items: [{ itemId: 1 }, { itemId: 2 }],
          })
        ).toBe(true);

        expect(
          isCamelCase({
            items: [{ item_id: 1 }, { item_id: 2 }],
          })
        ).toBe(false);
      });
    });
  });

  describe('round-trip transformation', () => {
    it('snake -> camel -> snake preserves structure', () => {
      const original = {
        user_name: 'alice',
        created_at: '2025-01-01',
        nested_obj: {
          inner_key: 'value',
        },
      };
      const camel = toCamelCase(original);
      const backToSnake = toSnakeCase(camel);
      expect(backToSnake).toEqual(original);
    });

    it('camel -> snake -> camel preserves structure', () => {
      const original = {
        userName: 'alice',
        createdAt: '2025-01-01',
        nestedObj: {
          innerKey: 'value',
        },
      };
      const snake = toSnakeCase(original);
      const backToCamel = toCamelCase(snake);
      expect(backToCamel).toEqual(original);
    });

    it('preserves complex nested structures through round-trip', () => {
      const original = {
        user_data: {
          profile_info: {
            first_name: 'Alice',
            last_name: 'Smith',
          },
          account_settings: {
            is_active: true,
            notification_enabled: false,
          },
        },
        item_list: [
          { item_id: '1', item_name: 'first' },
          { item_id: '2', item_name: 'second' },
        ],
      };
      const camel = toCamelCase(original);
      const backToSnake = toSnakeCase(camel);
      expect(backToSnake).toEqual(original);
    });
  });

  describe('performance and edge cases', () => {
    it('handles large objects efficiently', () => {
      const largeObject: Record<string, unknown> = {};
      for (let i = 0; i < 1000; i++) {
        largeObject[`field_${i}`] = `value_${i}`;
      }

      const result = toCamelCase(largeObject);
      expect(Object.keys(result as object).length).toBe(1000);
      // Check first and last fields
      const resultObj = result as Record<string, unknown>;
      expect(resultObj.field0).toBe('value_0');
      expect(resultObj.field999).toBe('value_999');
    });

    it('handles deeply nested objects', () => {
      let deepObject: Record<string, unknown> = { value: 'deep' };
      for (let i = 0; i < 50; i++) {
        deepObject = { nested_level: deepObject };
      }

      const result = toCamelCase(deepObject);
      expect(result).toBeDefined();
      expect(result).toHaveProperty('nestedLevel');
    });

    it('handles objects with many array elements', () => {
      const input = {
        large_array: Array.from({ length: 100 }, (_, i) => ({
          item_id: `${i}`,
          item_value: i,
        })),
      };

      const result = toCamelCase(input);
      const largeArray = (result as { largeArray: Array<{ itemId: string; itemValue: number }> })
        .largeArray;
      expect(largeArray.length).toBe(100);
      expect(largeArray[0].itemId).toBe('0');
      expect(largeArray[99].itemId).toBe('99');
    });
  });
});
