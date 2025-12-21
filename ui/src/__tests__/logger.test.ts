import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { toast } from 'sonner';

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock fetch for telemetry
const mockFetch = vi.fn().mockResolvedValue({ ok: true });
vi.stubGlobal('fetch', mockFetch);

describe('logger', () => {
  let logger: typeof import('../utils/logger').logger;
  let LogLevel: typeof import('../utils/logger').LogLevel;
  let toError: typeof import('../utils/logger').toError;
  let consoleSpy: { log: ReturnType<typeof vi.spyOn>; warn: ReturnType<typeof vi.spyOn>; error: ReturnType<typeof vi.spyOn> };

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();

    // Spy on console methods
    consoleSpy = {
      log: vi.spyOn(console, 'log').mockImplementation(() => {}),
      warn: vi.spyOn(console, 'warn').mockImplementation(() => {}),
      error: vi.spyOn(console, 'error').mockImplementation(() => {}),
    };

    // Import fresh module for each test
    const loggerModule = await import('../utils/logger');
    logger = loggerModule.logger;
    LogLevel = loggerModule.LogLevel;
    toError = loggerModule.toError;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Logger creates entries with correct levels', () => {
    it('creates debug log entry', () => {
      logger.debug('Debug message', { component: 'Test' });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        '[DEBUG] Debug message',
        expect.objectContaining({
          level: LogLevel.DEBUG,
          message: 'Debug message',
          context: { component: 'Test' },
        })
      );
    });

    it('creates info log entry', () => {
      logger.info('Info message', { operation: 'test' });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        '[INFO] Info message',
        expect.objectContaining({
          level: LogLevel.INFO,
          message: 'Info message',
          context: { operation: 'test' },
        })
      );
    });

    it('creates warn log entry', () => {
      logger.warn('Warning message');

      expect(consoleSpy.warn).toHaveBeenCalledWith(
        '[WARN] Warning message',
        expect.objectContaining({
          level: LogLevel.WARN,
          message: 'Warning message',
        })
      );
    });

    it('creates error log entry with error object', () => {
      const error = new Error('Test error');
      logger.error('Error message', { component: 'Test' }, error);

      expect(consoleSpy.error).toHaveBeenCalledWith(
        '[ERROR] Error message',
        expect.objectContaining({
          level: LogLevel.ERROR,
          message: 'Error message',
          error: {
            name: 'Error',
            message: 'Test error',
            stack: expect.any(String),
          },
        })
      );
    });

    it('includes timestamp in log entries', () => {
      logger.info('Test message');

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          timestamp: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/),
        })
      );
    });
  });

  describe('Error toast throttling', () => {
    it('shows toast for first error', () => {
      const error = new Error('Test error');
      logger.error('First error', {}, error);

      expect(toast.error).toHaveBeenCalledWith('First error');
    });

    it('throttles duplicate error toasts', () => {
      const error = new Error('Test error');

      // First error shows toast
      logger.error('Duplicate error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(1);

      // Second identical error within throttle window is suppressed
      logger.error('Duplicate error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(1);
    });

    it('allows same error after throttle window', async () => {
      vi.useFakeTimers();
      const error = new Error('Test error');

      logger.error('Timed error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(1);

      // Advance past throttle window (10 seconds)
      vi.advanceTimersByTime(11000);

      logger.error('Timed error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(2);

      vi.useRealTimers();
    });

    it('suppresses toasts for background operations', () => {
      const error = new Error('Background error');

      // Background operations should not show toast
      logger.error('Fetch error', { operation: 'fetchNotifications' }, error);
      expect(toast.error).not.toHaveBeenCalled();

      logger.error('SSE error', { operation: 'sse_init' }, error);
      expect(toast.error).not.toHaveBeenCalled();

      logger.error('Storage error', { operation: 'storage_listener' }, error);
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('does not show toast for errors without error object', () => {
      logger.error('Error without object');
      expect(toast.error).not.toHaveBeenCalled();
    });
  });

  describe('Telemetry integration', () => {
    beforeEach(() => {
      vi.clearAllMocks();

      // Set production mode
      logger.setDevelopmentMode(false);
    });

    afterEach(() => {
      // Reset to dev mode
      logger.setDevelopmentMode(null);
    });

    it('sends logs to telemetry endpoint in production', async () => {
      logger.info('Production log', { component: 'Test', userId: 'user-123', tenantId: 'tenant-1' });

      // Wait for async fetch
      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          '/api/v1/telemetry/logs',
          expect.objectContaining({
            method: 'POST',
            headers: expect.objectContaining({ 'Content-Type': 'application/json' }),
            credentials: 'omit', // Bearer-only auth
          })
        );
      });

      // Verify telemetry event format
      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);
      expect(body).toHaveLength(1);
      expect(body[0]).toMatchObject({
        event_type: 'client_log',
        level: 'Info',
        message: 'Production log',
        component: 'Test',
        user_id: 'user-123',
        tenant_id: 'tenant-1',
      });
    });

    it('sends error logs with client_error event type', async () => {
      const error = new Error('Test error');
      logger.error('Error log', { component: 'Test' }, error);

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);
      expect(body[0].event_type).toBe('client_error');
      expect(body[0].level).toBe('Error');
    });

    it('handles telemetry failure gracefully', async () => {
      const consoleErrorSpy = vi.spyOn(window.console, 'error').mockImplementation(() => {});
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      logger.info('Log with failed telemetry');

      await vi.waitFor(() => {
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          'Failed to send log to telemetry:',
          expect.any(Error)
        );
      });

      consoleErrorSpy.mockRestore();
    });
  });

  describe('Development mode enables debug logs', () => {
    it('logs debug messages to console in development', () => {
      logger.debug('Debug in dev');
      expect(consoleSpy.log).toHaveBeenCalled();
    });

    it('does not call telemetry in development', () => {
      logger.info('Dev log');
      expect(mockFetch).not.toHaveBeenCalled();
    });
  });

  describe('Production mode filters debug logs', () => {
    beforeEach(() => {
      vi.clearAllMocks();

      // Set production mode
      logger.setDevelopmentMode(false);
    });

    afterEach(() => {
      // Reset to dev mode
      logger.setDevelopmentMode(null);
    });

    it('does not log to console in production', () => {
      logger.info('Production log');
      expect(consoleSpy.log).not.toHaveBeenCalled();
      expect(consoleSpy.warn).not.toHaveBeenCalled();
      expect(consoleSpy.error).not.toHaveBeenCalled();
    });
  });

  describe('Context is properly attached to log entries', () => {
    it('attaches all context fields', () => {
      logger.info('Contextual log', {
        component: 'Dashboard',
        operation: 'fetchData',
        userId: 'user-123',
        tenantId: 'tenant-1',
        requestId: 'req-456',
        customField: 'custom-value',
      });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {
            component: 'Dashboard',
            operation: 'fetchData',
            userId: 'user-123',
            tenantId: 'tenant-1',
            requestId: 'req-456',
            customField: 'custom-value',
          },
        })
      );
    });

    it('handles empty context', () => {
      logger.info('No context');

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {},
        })
      );
    });

    it('handles undefined context', () => {
      logger.info('Undefined context', undefined);

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {},
        })
      );
    });
  });

  describe('Toast cleanup removes old entries', () => {
    it('cleans up old error toast entries when limit exceeded', () => {
      vi.useFakeTimers();
      const error = new Error('Test');

      // Generate more than 50 unique errors
      for (let i = 0; i < 55; i++) {
        logger.error(`Error ${i}`, {}, error);
        vi.advanceTimersByTime(100); // Small time between each
      }

      // All errors should have been logged (toast shown)
      expect(toast.error).toHaveBeenCalledTimes(55);

      // Advance time past cleanup threshold (20 seconds)
      vi.advanceTimersByTime(25000);

      // Trigger another error to cause cleanup
      logger.error('Cleanup trigger', {}, error);

      // The old entries should have been cleaned up
      // (Internal implementation detail - we just verify it doesn't crash)
      expect(toast.error).toHaveBeenCalledTimes(56);

      vi.useRealTimers();
    });
  });

  describe('toError() utility converts unknowns to Error', () => {
    it('passes through Error instances', () => {
      const original = new Error('Original error');
      const result = toError(original);
      expect(result).toBe(original);
    });

    it('converts string to Error', () => {
      const result = toError('String error');
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('String error');
    });

    it('converts object to Error with JSON string', () => {
      const result = toError({ code: 'ERR', detail: 'Something went wrong' });
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('{"code":"ERR","detail":"Something went wrong"}');
    });

    it('converts number to Error', () => {
      const result = toError(404);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('404');
    });

    it('converts null to Error', () => {
      const result = toError(null);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('null');
    });

    it('converts undefined to Error', () => {
      const result = toError(undefined);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('undefined');
    });

    it('handles circular references gracefully', () => {
      const circular: { self?: unknown } = {};
      circular.self = circular;

      const result = toError(circular);
      expect(result).toBeInstanceOf(Error);
      // Falls back to String() when JSON.stringify fails
      expect(result.message).toBe('[object Object]');
    });

    it('converts array to Error with JSON string', () => {
      const result = toError(['error1', 'error2']);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('["error1","error2"]');
    });
  });
});
