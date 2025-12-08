import { describe, it, expect, vi, beforeEach } from 'vitest';

const captureExceptionMock = vi.fn();
const loggerLogMock = vi.fn();
const toErrorMock = vi.fn((input: unknown) =>
  input instanceof Error ? input : new Error(String(input))
);

vi.mock('@/stores/errorStore', () => ({
  captureException: captureExceptionMock,
}));

vi.mock('@/utils/logger', () => ({
  logger: {
    log: loggerLogMock,
  },
  LogLevel: {
    ERROR: 'error',
    WARN: 'warn',
    INFO: 'info',
  },
  toError: toErrorMock,
}));

describe('logUIError', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('normalizes errors, logs with context, and captures exception extras', async () => {
    const { logUIError, logUIWarning } = await import('@/lib/logUIError');
    const error = new Error('boom');

    logUIError(error, {
      scope: 'page',
      route: '/dev/api-errors',
      pageKey: 'dev-errors',
      component: 'DevErrorsTest',
    });

    expect(toErrorMock).toHaveBeenCalledWith(error);
    expect(loggerLogMock).toHaveBeenCalledWith(
      'error',
      'UI error',
      expect.objectContaining({
        scope: 'page',
        route: '/dev/api-errors',
        pageKey: 'dev-errors',
        component: 'DevErrorsTest',
        severity: 'error',
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
        severity: 'error',
        userMessageKey: undefined,
      },
    });

    loggerLogMock.mockClear();
    captureExceptionMock.mockClear();

    logUIWarning('warn-case', {
      scope: 'section',
      component: 'DevErrorsTest',
      userMessageKey: 'ui.warning.retry',
    });

    expect(loggerLogMock).toHaveBeenCalledWith(
      'warn',
      'UI warning',
      expect.objectContaining({
        scope: 'section',
        component: 'DevErrorsTest',
        severity: 'warning',
      }),
      expect.any(Error)
    );
    expect(captureExceptionMock).toHaveBeenCalledWith(
      expect.any(Error),
      expect.objectContaining({
        extra: expect.objectContaining({
          severity: 'warning',
          userMessageKey: 'ui.warning.retry',
        }),
      })
    );
  });
});

