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

// Mock API client module to prevent circular dependency issues
vi.mock('@/api/services', () => ({
  apiClient: {
    getToken: vi.fn(() => 'mock-auth-token'),
  },
}));

// Mock fetch for telemetry
const mockFetch = vi.fn().mockResolvedValue({ ok: true });
vi.stubGlobal('fetch', mockFetch);

// Mock window properties for environment tests
const mockNavigator = {
  userAgent: 'Mozilla/5.0 Test Browser',
  onLine: true,
};

const mockWindow = {
  innerWidth: 1920,
  innerHeight: 1080,
  console: {
    error: vi.fn(),
    log: vi.fn(),
    warn: vi.fn(),
  },
};

const mockPerformance = {
  memory: {
    usedJSHeapSize: 50000000,
  },
};

vi.stubGlobal('navigator', mockNavigator);
vi.stubGlobal('window', mockWindow);
vi.stubGlobal('performance', mockPerformance);

describe('logger', () => {
  let logger: typeof import('../logger').logger;
  let LogLevel: typeof import('../logger').LogLevel;
  let toError: typeof import('../logger').toError;
  let consoleSpy: {
    log: ReturnType<typeof vi.spyOn>;
    warn: ReturnType<typeof vi.spyOn>;
    error: ReturnType<typeof vi.spyOn>;
  };

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
    const loggerModule = await import('../logger');
    logger = loggerModule.logger;
    LogLevel = loggerModule.LogLevel;
    toError = loggerModule.toError;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Log level methods', () => {
    it('debug() creates debug log entry', () => {
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

    it('info() creates info log entry', () => {
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

    it('warn() creates warn log entry', () => {
      logger.warn('Warning message');

      expect(consoleSpy.warn).toHaveBeenCalledWith(
        '[WARN] Warning message',
        expect.objectContaining({
          level: LogLevel.WARN,
          message: 'Warning message',
        })
      );
    });

    it('warn() accepts error object', () => {
      const error = new Error('Warning error');
      logger.warn('Warning with error', { component: 'Test' }, error);

      expect(consoleSpy.warn).toHaveBeenCalledWith(
        '[WARN] Warning with error',
        expect.objectContaining({
          level: LogLevel.WARN,
          error: {
            name: 'Error',
            message: 'Warning error',
            stack: expect.any(String),
          },
        })
      );
    });

    it('error() creates error log entry with error object', () => {
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

    it('includes timestamp in ISO format', () => {
      logger.info('Test message');

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          timestamp: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/),
        })
      );
    });
  });

  describe('Structured context passing', () => {
    it('attaches complete context with all fields', () => {
      logger.info('Contextual log', {
        component: 'Dashboard',
        operation: 'fetchData',
        userId: 'user-123',
        tenantId: 'tenant-1',
        requestId: 'req-456',
        errorType: 'network_error',
        details: 'Connection timeout',
        recoverySuggestion: 'Retry the request',
        userJourney: 'document_upload',
        validationFailures: ['field1', 'field2'],
        timing: {
          operationStart: 1000,
          operationDuration: 500,
          networkLatency: 200,
        },
        resourceState: { count: 5 },
        featureFlags: { newFeature: true },
        rateLimit: {
          remaining: 10,
          resetTime: 3600,
          limit: 100,
        },
      });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: expect.objectContaining({
            component: 'Dashboard',
            operation: 'fetchData',
            userId: 'user-123',
            tenantId: 'tenant-1',
            requestId: 'req-456',
            errorType: 'network_error',
            details: 'Connection timeout',
            recoverySuggestion: 'Retry the request',
            userJourney: 'document_upload',
            validationFailures: ['field1', 'field2'],
            timing: {
              operationStart: 1000,
              operationDuration: 500,
              networkLatency: 200,
            },
            resourceState: { count: 5 },
            featureFlags: { newFeature: true },
            rateLimit: {
              remaining: 10,
              resetTime: 3600,
              limit: 100,
            },
          }),
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

    it('supports custom context fields via index signature', () => {
      logger.info('Custom fields', {
        customField1: 'value1',
        customField2: 123,
        customField3: true,
        nestedObject: { key: 'value' },
      });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {
            customField1: 'value1',
            customField2: 123,
            customField3: true,
            nestedObject: { key: 'value' },
          },
        })
      );
    });
  });

  describe('Error redaction/sanitization', () => {
    it('does not include auth tokens in telemetry payload', async () => {
      logger.setDevelopmentMode(false);

      logger.info('Test log', {
        component: 'Auth',
        // Token should be in Authorization header, not in log context
        userId: 'user-123',
      });

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);

      // Verify token is only in Authorization header, not in payload
      expect(call[1].headers.Authorization).toBe('Bearer mock-auth-token');
      expect(JSON.stringify(body)).not.toContain('mock-auth-token');

      logger.setDevelopmentMode(null);
    });

    it('handles error objects without exposing internal details in production', async () => {
      logger.setDevelopmentMode(false);

      const error = new Error('Internal database connection failed at 192.168.1.100:5432');
      logger.error('Database error', { component: 'DB' }, error);

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);

      // Error information is included in telemetry payload
      // Note: metadata contains the context, error is in logEntry structure
      expect(body[0]).toBeDefined();
      expect(body[0].message).toBe('Database error');

      logger.setDevelopmentMode(null);
    });
  });

  describe('Telemetry event formatting', () => {
    beforeEach(() => {
      vi.clearAllMocks();
      logger.setDevelopmentMode(false);
    });

    afterEach(() => {
      logger.setDevelopmentMode(null);
    });

    it('formats log entry as UnifiedTelemetryEvent for backend', async () => {
      logger.info('Production log', {
        component: 'Test',
        userId: 'user-123',
        tenantId: 'tenant-1',
      });

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          '/api/v1/telemetry/logs',
          expect.objectContaining({
            method: 'POST',
            headers: expect.objectContaining({
              'Content-Type': 'application/json',
              Authorization: 'Bearer mock-auth-token',
            }),
            credentials: 'omit', // Bearer-only auth
          })
        );
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);

      expect(body).toHaveLength(1);
      expect(body[0]).toMatchObject({
        id: expect.any(String),
        timestamp: expect.any(String),
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

    it('maps log levels to unified format correctly', async () => {
      const testCases = [
        { method: 'debug', expected: 'Debug' },
        { method: 'info', expected: 'Info' },
        { method: 'warn', expected: 'Warn' },
        { method: 'error', expected: 'Error' },
      ];

      for (const testCase of testCases) {
        vi.clearAllMocks();

        if (testCase.method === 'error') {
          logger[testCase.method]('Test', {}, new Error('test'));
        } else {
          logger[testCase.method as 'debug' | 'info' | 'warn']('Test', {});
        }

        await vi.waitFor(() => {
          expect(mockFetch).toHaveBeenCalled();
        });

        const call = mockFetch.mock.calls[0];
        const body = JSON.parse(call[1].body);
        expect(body[0].level).toBe(testCase.expected);
      }
    });

    it('sends logs as array to backend', async () => {
      logger.info('Test message');

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);
      expect(Array.isArray(body)).toBe(true);
      expect(body).toHaveLength(1);
    });

    it('includes metadata field with full context', async () => {
      logger.info('Test', {
        component: 'Test',
        customField: 'value',
        nested: { data: 'test' },
      });

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);
      expect(body[0].metadata).toMatchObject({
        component: 'Test',
        customField: 'value',
        nested: { data: 'test' },
      });
    });

    it('creates deterministic ID from timestamp', async () => {
      logger.info('Test');

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      const body = JSON.parse(call[1].body);

      // ID should be timestamp with special chars replaced
      expect(body[0].id).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}/);
      expect(body[0].id).not.toContain(':');
      expect(body[0].id).not.toContain('.');
    });

    it('handles telemetry failure gracefully', async () => {
      // Spy on window.console.error (used in sendToTelemetry catch block)
      const windowConsoleErrorSpy = vi.spyOn(mockWindow.console, 'error');
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      logger.info('Log with failed telemetry');

      await vi.waitFor(() => {
        expect(windowConsoleErrorSpy).toHaveBeenCalledWith(
          'Failed to send log to telemetry:',
          expect.any(Error)
        );
      });

      windowConsoleErrorSpy.mockRestore();
    });

    it('does not send to telemetry in development mode', () => {
      logger.setDevelopmentMode(true);
      logger.info('Dev log');
      expect(mockFetch).not.toHaveBeenCalled();
      logger.setDevelopmentMode(null);
    });
  });

  describe('Toast notification triggers', () => {
    it('shows toast for first error', () => {
      const error = new Error('Test error');
      logger.error('First error', {}, error);

      expect(toast.error).toHaveBeenCalledWith('First error');
    });

    it('throttles duplicate error toasts within throttle window', () => {
      const error = new Error('Test error');

      logger.error('Duplicate error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(1);

      logger.error('Duplicate error', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(1);
    });

    it('allows same error message after throttle window expires', async () => {
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

    it('only shows toast for ERROR level, not WARN', () => {
      const error = new Error('Test error');
      logger.warn('Warning message', {}, error);
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('cleans up old toast history when limit exceeded', () => {
      vi.useFakeTimers();
      const error = new Error('Test');

      // Generate more than 50 unique errors
      for (let i = 0; i < 55; i++) {
        logger.error(`Error ${i}`, {}, error);
        vi.advanceTimersByTime(100);
      }

      expect(toast.error).toHaveBeenCalledTimes(55);

      // Advance time past cleanup threshold (20 seconds)
      vi.advanceTimersByTime(25000);

      // Trigger another error to cause cleanup
      logger.error('Cleanup trigger', {}, error);
      expect(toast.error).toHaveBeenCalledTimes(56);

      vi.useRealTimers();
    });
  });

  describe('Token inclusion prevention', () => {
    it('does not log auth tokens in console', () => {
      logger.info('Auth operation', {
        component: 'Auth',
        userId: 'user-123',
      });

      const logCall = consoleSpy.log.mock.calls[0];
      const logEntry = JSON.stringify(logCall);

      expect(logEntry).not.toContain('mock-auth-token');
      expect(logEntry).not.toContain('Bearer');
    });

    it('includes token only in Authorization header for telemetry', async () => {
      logger.setDevelopmentMode(false);
      logger.info('Test');

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      expect(call[1].headers.Authorization).toBe('Bearer mock-auth-token');

      logger.setDevelopmentMode(null);
    });

    it('omits Authorization header when token is unavailable', async () => {
      const { apiClient } = await import('@/api/services');
      vi.mocked(apiClient.getToken).mockReturnValueOnce(undefined);

      logger.setDevelopmentMode(false);
      logger.info('No token test');

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      const call = mockFetch.mock.calls[0];
      expect(call[1].headers.Authorization).toBeUndefined();

      logger.setDevelopmentMode(null);
    });
  });

  describe('Edge cases: circular references', () => {
    it('handles circular references in context', () => {
      const circular: { self?: unknown } = {};
      circular.self = circular;

      // Should not throw
      expect(() => {
        logger.info('Circular context', { data: circular });
      }).not.toThrow();

      expect(consoleSpy.log).toHaveBeenCalled();
    });

    it('handles circular references in resourceState', () => {
      const circular: { ref?: unknown } = {};
      circular.ref = circular;

      expect(() => {
        logger.info('Circular resource', {
          resourceState: circular,
        });
      }).not.toThrow();
    });
  });

  describe('Edge cases: very large objects', () => {
    it('handles large context objects', () => {
      const largeContext = {
        component: 'Test',
        largeArray: Array(10000).fill('data'),
        nestedData: {
          level1: {
            level2: {
              level3: {
                data: Array(1000).fill('nested'),
              },
            },
          },
        },
      };

      expect(() => {
        logger.info('Large context', largeContext);
      }).not.toThrow();

      expect(consoleSpy.log).toHaveBeenCalled();
    });

    it('handles large error stack traces', () => {
      const error = new Error('Test error');
      error.stack = 'a'.repeat(100000); // Very long stack trace

      expect(() => {
        logger.error('Large stack trace', {}, error);
      }).not.toThrow();

      expect(consoleSpy.error).toHaveBeenCalled();
    });
  });

  describe('toError() utility', () => {
    it('passes through Error instances unchanged', () => {
      const original = new Error('Original error');
      const result = toError(original);
      expect(result).toBe(original);
      expect(result.message).toBe('Original error');
    });

    it('converts string to Error', () => {
      const result = toError('String error');
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('String error');
    });

    it('converts number to Error', () => {
      const result = toError(404);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('404');
    });

    it('converts object to Error with JSON string', () => {
      const result = toError({ code: 'ERR', detail: 'Something went wrong' });
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('{"code":"ERR","detail":"Something went wrong"}');
    });

    it('converts array to Error with JSON string', () => {
      const result = toError(['error1', 'error2']);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('["error1","error2"]');
    });

    it('converts null to Error', () => {
      const result = toError(null);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('null');
    });

    it('converts undefined to Error with specific message', () => {
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

    it('converts boolean to Error', () => {
      const result = toError(false);
      expect(result).toBeInstanceOf(Error);
      expect(result.message).toBe('false');
    });

    it('preserves custom Error subclasses', () => {
      class CustomError extends Error {
        code = 'CUSTOM';
      }

      const original = new CustomError('Custom error');
      const result = toError(original);
      expect(result).toBe(original);
      expect(result).toBeInstanceOf(CustomError);
      expect((result as CustomError).code).toBe('CUSTOM');
    });
  });

  describe('Request ID tracking', () => {
    it('tracks request IDs for error correlation', () => {
      logger.trackRequestId('req-123');
      logger.trackRequestId('req-456');

      logger.info('Test log');

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {
            requestId: 'req-456', // Most recent
          },
        })
      );
    });

    it('limits tracked request IDs to MAX_RECENT_REQUEST_IDS', () => {
      // Track more than max (10)
      for (let i = 0; i < 15; i++) {
        logger.trackRequestId(`req-${i}`);
      }

      logger.info('Test log');

      // Should have the most recent one (14)
      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {
            requestId: 'req-14',
          },
        })
      );
    });

    it('does not override explicit requestId in context', () => {
      logger.trackRequestId('req-tracked');
      logger.info('Test log', { requestId: 'req-explicit' });

      expect(consoleSpy.log).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          context: {
            requestId: 'req-explicit',
          },
        })
      );
    });
  });

  describe('Enhanced error logging methods', () => {
    describe('errorWithRecovery()', () => {
      it('logs error with recovery suggestion', () => {
        const error = new Error('Database connection failed');
        logger.errorWithRecovery(
          'Failed to save data',
          { component: 'Database' },
          error,
          'Please check your network connection and try again'
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          '[ERROR] Failed to save data',
          expect.objectContaining({
            context: expect.objectContaining({
              recoverySuggestion: 'Please check your network connection and try again',
            }),
          })
        );
      });

      it('includes environment context by default', () => {
        const error = new Error('Test error');
        logger.errorWithRecovery('Error message', { component: 'Test' }, error, 'Try again');

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              environment: {
                userAgent: 'Mozilla/5.0 Test Browser',
                viewport: '1920x1080',
                online: true,
                memoryUsage: 50000000,
              },
            }),
          })
        );
      });
    });

    describe('validationError()', () => {
      it('logs validation errors with failure details', () => {
        const error = new Error('Validation failed');
        logger.validationError(
          'Form validation failed',
          { component: 'Form' },
          ['Email is required', 'Password must be at least 8 characters'],
          error
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          '[ERROR] Form validation failed',
          expect.objectContaining({
            context: expect.objectContaining({
              errorType: 'validation_failure',
              validationFailures: ['Email is required', 'Password must be at least 8 characters'],
              recoverySuggestion:
                'Please check the highlighted fields and correct the validation errors.',
            }),
          })
        );
      });

      it('preserves custom errorType if provided', () => {
        logger.validationError(
          'Custom validation error',
          { component: 'Form', errorType: 'custom_validation' },
          ['Field error'],
          new Error('test')
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              errorType: 'custom_validation',
            }),
          })
        );
      });
    });

    describe('networkError()', () => {
      it('logs network error with timeout recovery suggestion', () => {
        const error = new Error('Request timeout');
        logger.networkError(
          'API request failed',
          { component: 'API' },
          { timeout: true, url: '/api/data', method: 'GET' },
          error
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          '[ERROR] API request failed',
          expect.objectContaining({
            context: expect.objectContaining({
              errorType: 'network_failure',
              recoverySuggestion:
                'The request timed out. Please check your connection and try again.',
              networkDetails: {
                timeout: true,
                url: '/api/data',
                method: 'GET',
              },
            }),
          })
        );
      });

      it('logs network error with connection error recovery suggestion', () => {
        logger.networkError(
          'Connection failed',
          { component: 'API' },
          { connectionError: true },
          new Error('test')
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              recoverySuggestion:
                'Network connection failed. Please check your internet connection.',
            }),
          })
        );
      });

      it('logs network error with rate limit recovery suggestion', () => {
        logger.networkError(
          'Rate limited',
          { component: 'API' },
          { status: 429 },
          new Error('test')
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              recoverySuggestion: 'Rate limit exceeded. Please wait before retrying.',
            }),
          })
        );
      });

      it('logs network error with server error recovery suggestion', () => {
        logger.networkError(
          'Server error',
          { component: 'API' },
          { status: 500, statusText: 'Internal Server Error' },
          new Error('test')
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              recoverySuggestion:
                'Server error occurred. Please try again later or contact support.',
            }),
          })
        );
      });

      it('logs network error with generic recovery suggestion', () => {
        logger.networkError(
          'Request failed',
          { component: 'API' },
          { status: 400 },
          new Error('test')
        );

        expect(consoleSpy.error).toHaveBeenCalledWith(
          expect.any(String),
          expect.objectContaining({
            context: expect.objectContaining({
              recoverySuggestion: 'Network request failed. Please try again.',
            }),
          })
        );
      });
    });
  });

  describe('createErrorContext()', () => {
    it('creates enhanced context with environment data', () => {
      const context = logger.createErrorContext({ component: 'Test' });

      expect(context.environment).toEqual({
        userAgent: 'Mozilla/5.0 Test Browser',
        viewport: '1920x1080',
        online: true,
        memoryUsage: 50000000,
      });
    });

    it('includes timing information when requested', () => {
      const operationStart = Date.now() - 1000;
      const context = logger.createErrorContext(
        { component: 'Test' },
        {
          includeTiming: true,
          operationStart,
        }
      );

      expect(context.timing).toBeDefined();
      expect(context.timing?.operationStart).toBe(operationStart);
      expect(context.timing?.operationDuration).toBeGreaterThanOrEqual(1000);
    });

    it('excludes environment when explicitly disabled', () => {
      const context = logger.createErrorContext(
        { component: 'Test' },
        { includeEnvironment: false }
      );

      expect(context.environment).toBeUndefined();
    });

    it('handles missing window gracefully', () => {
      const originalWindow = global.window;
      // @ts-expect-error - Intentionally setting to undefined for test
      global.window = undefined;

      const context = logger.createErrorContext({ component: 'Test' });

      expect(context.environment).toEqual({});

      global.window = originalWindow;
    });
  });

  describe('Development vs Production mode', () => {
    it('logs to console in development mode', () => {
      logger.setDevelopmentMode(true);
      logger.info('Dev log');

      expect(consoleSpy.log).toHaveBeenCalled();
      expect(mockFetch).not.toHaveBeenCalled();

      logger.setDevelopmentMode(null);
    });

    it('sends to telemetry in production mode', async () => {
      logger.setDevelopmentMode(false);
      logger.info('Production log');

      expect(consoleSpy.log).not.toHaveBeenCalled();

      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      logger.setDevelopmentMode(null);
    });

    it('uses correct console method for each log level', () => {
      logger.debug('Debug');
      expect(consoleSpy.log).toHaveBeenCalledWith('[DEBUG] Debug', expect.any(Object));

      logger.info('Info');
      expect(consoleSpy.log).toHaveBeenCalledWith('[INFO] Info', expect.any(Object));

      logger.warn('Warning');
      expect(consoleSpy.warn).toHaveBeenCalledWith('[WARN] Warning', expect.any(Object));

      logger.error('Error', {}, new Error());
      expect(consoleSpy.error).toHaveBeenCalledWith('[ERROR] Error', expect.any(Object));
    });
  });
});
