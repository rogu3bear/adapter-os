import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Use the real client implementation
vi.unmock('@/api/client');

describe('ApiClient transformation integration', () => {
  let apiClient: InstanceType<typeof import('@/api/client').ApiClient>;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    vi.resetModules();
    fetchMock = vi.fn();
    (globalThis as { fetch?: unknown }).fetch = fetchMock;

    const clientModule = await vi.importActual<typeof import('@/api/client')>('@/api/client');
    apiClient = new clientModule.ApiClient('/api');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('request body transformation', () => {
    it('transforms camelCase request body to snake_case', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify({ success: true }),
        json: async () => ({ success: true }),
      });

      const requestBody = {
        userId: 123,
        firstName: 'John',
        userData: {
          emailAddress: 'john@example.com',
          phoneNumber: '555-1234',
        },
      };

      await apiClient.request('/v1/users', {
        method: 'POST',
        body: JSON.stringify(requestBody),
      });

      expect(fetchMock).toHaveBeenCalled();
      const callArgs = fetchMock.mock.calls[0];
      const sentBody = JSON.parse(callArgs[1].body);

      expect(sentBody).toEqual({
        user_id: 123,
        first_name: 'John',
        user_data: {
          email_address: 'john@example.com',
          phone_number: '555-1234',
        },
      });
    });

    it('transforms nested arrays in request body', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify({ success: true }),
        json: async () => ({ success: true }),
      });

      const requestBody = {
        userList: [
          { userId: 1, firstName: 'John' },
          { userId: 2, firstName: 'Jane' },
        ],
      };

      await apiClient.request('/v1/users/batch', {
        method: 'POST',
        body: JSON.stringify(requestBody),
      });

      const callArgs = fetchMock.mock.calls[0];
      const sentBody = JSON.parse(callArgs[1].body);

      expect(sentBody).toEqual({
        user_list: [
          { user_id: 1, first_name: 'John' },
          { user_id: 2, first_name: 'Jane' },
        ],
      });
    });

    it('preserves FormData without transformation', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify({ success: true }),
        json: async () => ({ success: true }),
      });

      const formData = new FormData();
      formData.append('file', new Blob(['test']), 'test.txt');

      await apiClient.request('/v1/upload', {
        method: 'POST',
        body: formData,
      });

      const callArgs = fetchMock.mock.calls[0];
      expect(callArgs[1].body).toBeInstanceOf(FormData);
      expect(callArgs[1].body).toBe(formData);
    });
  });

  describe('response body transformation', () => {
    it('transforms snake_case response to camelCase', async () => {
      const responseData = {
        user_id: 123,
        first_name: 'John',
        last_name: 'Doe',
        user_data: {
          email_address: 'john@example.com',
          phone_number: '555-1234',
        },
      };

      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify(responseData),
        json: async () => responseData,
      });

      const result = await apiClient.request<{
        userId: number;
        firstName: string;
        lastName: string;
        userData: {
          emailAddress: string;
          phoneNumber: string;
        };
      }>('/v1/users/123');

      expect(result).toEqual({
        userId: 123,
        firstName: 'John',
        lastName: 'Doe',
        userData: {
          emailAddress: 'john@example.com',
          phoneNumber: '555-1234',
        },
      });
    });

    it('transforms arrays in response', async () => {
      const responseData = {
        user_list: [
          { user_id: 1, first_name: 'John' },
          { user_id: 2, first_name: 'Jane' },
        ],
      };

      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify(responseData),
        json: async () => responseData,
      });

      const result = await apiClient.request<{
        userList: Array<{ userId: number; firstName: string }>;
      }>('/v1/users');

      expect(result).toEqual({
        userList: [
          { userId: 1, firstName: 'John' },
          { userId: 2, firstName: 'Jane' },
        ],
      });
    });
  });

  describe('error response transformation', () => {
    it('transforms snake_case error response to camelCase', async () => {
      const errorResponse = {
        code: 'INVALID_INPUT',
        message: 'Invalid user data',
        detail: 'Email address is required',
        request_id: 'req-123',
        details: {
          field_name: 'email_address',
          error_type: 'required',
        },
      };

      fetchMock.mockResolvedValue({
        ok: false,
        status: 400,
        statusText: 'Bad Request',
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify(errorResponse),
        json: async () => errorResponse,
      });

      try {
        await apiClient.request('/v1/users', {
          method: 'POST',
          body: JSON.stringify({ userId: 123 }),
        });
        expect.fail('Should have thrown an error');
      } catch (error: unknown) {
        const apiError = error as {
          code?: string;
          message: string;
          detail?: string;
          requestId?: string;
          details?: Record<string, unknown>;
        };

        expect(apiError.code).toBe('INVALID_INPUT');
        expect(apiError.message).toBe('Invalid user data');
        expect(apiError.detail).toBe('Email address is required');
        expect(apiError.requestId).toBe('req-123');
        expect(apiError.details).toEqual({
          fieldName: 'email_address',
          errorType: 'required',
        });
      }
    });
  });

  describe('requestList transformation', () => {
    it('transforms snake_case array response to camelCase', async () => {
      const responseData = [
        { user_id: 1, first_name: 'John' },
        { user_id: 2, first_name: 'Jane' },
      ];

      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify(responseData),
        json: async () => responseData,
      });

      const result = await apiClient.requestList<{ userId: number; firstName: string }>(
        '/v1/users'
      );

      expect(result).toEqual([
        { userId: 1, firstName: 'John' },
        { userId: 2, firstName: 'Jane' },
      ]);
    });

    it('transforms paginated response to camelCase', async () => {
      const responseData = {
        data: [
          { user_id: 1, first_name: 'John' },
          { user_id: 2, first_name: 'Jane' },
        ],
        total: 2,
        page: 1,
        per_page: 10,
      };

      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'Content-Type': 'application/json' }),
        text: async () => JSON.stringify(responseData),
        json: async () => responseData,
      });

      const result = await apiClient.requestList<{ userId: number; firstName: string }>(
        '/v1/users'
      );

      expect(result).toEqual([
        { userId: 1, firstName: 'John' },
        { userId: 2, firstName: 'Jane' },
      ]);
    });
  });
});
