import { describe, it, expect, vi, beforeEach } from 'vitest';

const captureExceptionMock = vi.fn();
const loggerErrorMock = vi.fn();
const toErrorMock = vi.fn((input: unknown) =>
  input instanceof Error ? input : new Error(String(input))
);

vi.mock('@/stores/errorStore', () => ({
  captureException: captureExceptionMock,
}));

vi.mock('@/utils/logger', () => ({
  logger: {
    error: loggerErrorMock,
  },
  toError: toErrorMock,
}));

describe('logUIError', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('normalizes errors, logs with context, and captures exception extras', async () => {
    const { logUIError } = await import('@/lib/logUIError');
    const error = new Error('boom');

    logUIError(error, {
      scope: 'page',
      route: '/dev/api-errors',
      pageKey: 'dev-errors',
      component: 'DevErrorsTest',
    });

    expect(toErrorMock).toHaveBeenCalledWith(error);
    expect(loggerErrorMock).toHaveBeenCalledWith(
      'UI error',
      expect.objectContaining({
        scope: 'page',
        route: '/dev/api-errors',
        pageKey: 'dev-errors',
        component: 'DevErrorsTest',
      }),
      error
    );
    expect(captureExceptionMock).toHaveBeenCalledWith(error, {
      component: 'DevErrorsTest',
      operation: 'page',
      extra: {
        route: '/dev/api-errors',
        pageKey: 'dev-errors',
        scope: 'page',
      },
    });
  });
});

