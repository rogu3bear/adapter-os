import React, { useEffect } from 'react';
import { render, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ErrorStoreProvider, useErrorStore } from '@/stores/errorStore';

// Use the real client implementation
vi.unmock('@/api/client');

describe('ApiClient error handling', () => {
  let apiClient: typeof import('@/api/client').apiClient;
  let fetchMock: ReturnType<typeof vi.fn>;
  let originalEnv: typeof import.meta.env;

  beforeEach(async () => {
    vi.resetModules();
    fetchMock = vi.fn();
    (globalThis as { fetch?: unknown }).fetch = fetchMock;

    // Force dev-mode so captureException runs
    originalEnv = (import.meta as { env: typeof import.meta.env }).env;
    (import.meta as { env: typeof import.meta.env }).env = {
      ...originalEnv,
      DEV: true,
    };

    const clientModule = await vi.importActual<typeof import('@/api/client')>('@/api/client');
    apiClient = clientModule.apiClient;
  });

  afterEach(() => {
    (import.meta as { env: typeof import.meta.env }).env = originalEnv;
    vi.restoreAllMocks();
  });

  function StoreReader({ onReady }: { onReady: (store: ReturnType<typeof useErrorStore>) => void }) {
    const store = useErrorStore();
    useEffect(() => {
      onReady(store);
    }, [store, onReady]);
    return null;
  }

  it('parses ApiErrorBody and propagates request_id into ApiError and error store', async () => {
    const requestId = 'req-123';
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 400,
      statusText: 'Bad Request',
      json: () =>
        Promise.resolve({
          code: 'INVALID_INPUT',
          message: 'Invalid payload',
          detail: 'bad',
          request_id: requestId,
        }),
      headers: new Headers({ 'X-Request-ID': requestId }),
    });

    let storeRef: ReturnType<typeof useErrorStore> | null = null;
    render(
      <ErrorStoreProvider>
        <StoreReader onReady={(store) => (storeRef = store)} />
      </ErrorStoreProvider>
    );

    await waitFor(() => expect(storeRef).not.toBeNull());

    const errorStoreModule = await import('@/stores/errorStore');
    const captureSpy = vi
      .spyOn(errorStoreModule, 'captureException')
      .mockImplementation((error, context) => {
        return (
          storeRef?.captureError({
            message: (error as Error).message,
            code: (error as { code?: string }).code,
            httpStatus: (error as { status?: number }).status,
            component: context?.component,
            operation: context?.operation,
            context: context?.extra,
          }) ?? null
        );
      });

    let thrown: unknown;
    try {
      await apiClient.request('/v1/test');
    } catch (err) {
      thrown = err;
    }

    const apiError = thrown as { code?: string; requestId?: string; detail?: string; status?: number };
    expect(apiError.code).toBe('INVALID_INPUT');
    expect(apiError.requestId).toBe(requestId);
    expect(apiError.status).toBe(400);

    await waitFor(() => {
      expect(storeRef?.errors.length).toBeGreaterThan(0);
    });

    const captured = storeRef?.errors[0];
    expect(captured?.code).toBe('INVALID_INPUT');
    expect(captured?.context?.requestId).toBe(requestId);
    expect(captureSpy).toHaveBeenCalledWith(expect.any(Error), {
      component: 'ApiClient',
      operation: expect.stringContaining('GET /v1/test'),
      extra: expect.objectContaining({ requestId }),
    });
  });
});
