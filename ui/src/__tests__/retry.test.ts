import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  retryWithBackoff,
  retryWithTimeout,
  createRetryWrapper,
  CircuitBreaker,
  CircuitState,
  DEFAULT_RETRY_CONFIG,
} from '../utils/retry';

// Mock dependencies
vi.mock('../utils/errorMessages', () => ({
  isTransientError: (error: any) => {
    return error?.message?.includes('transient') || error?.status === 503;
  },
}));

vi.mock('../utils/logger', () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
  },
}));

// Mock the retry notification manager
vi.mock('../components/ui/retry-notification', () => ({
  retryNotificationManager: {
    show: vi.fn(),
  },
}));

describe('retryWithBackoff', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('should return immediately on successful operation', async () => {
    const operation = vi.fn().mockResolvedValue('success');

    const resultPromise = retryWithBackoff(operation);
    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(result).toEqual({
      success: true,
      value: 'success',
      attempts: 1,
    });
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it('should retry on transient errors with backoff', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('transient error'))
      .mockRejectedValueOnce(new Error('transient error'))
      .mockResolvedValue('success');

    const resultPromise = retryWithBackoff(operation, {
      maxAttempts: 3,
      baseDelay: 100,
      jitter: 0,
    });

    // First attempt fails
    await vi.advanceTimersByTimeAsync(0);

    // Wait for first delay (100ms)
    await vi.advanceTimersByTimeAsync(100);

    // Second attempt fails, wait for second delay (200ms)
    await vi.advanceTimersByTimeAsync(200);

    // Third attempt succeeds
    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(result).toEqual({
      success: true,
      value: 'success',
      attempts: 3,
    });
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it('should throw error when max retries exceeded', async () => {
    const error = new Error('transient error');
    const operation = vi.fn().mockRejectedValue(error);

    const resultPromise = retryWithBackoff(operation, {
      maxAttempts: 3,
      baseDelay: 100,
      jitter: 0,
    });

    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(result).toEqual({
      success: false,
      error: error,
      attempts: 3,
    });
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it('should not retry non-transient errors', async () => {
    const error = new Error('permanent error');
    const operation = vi.fn().mockRejectedValue(error);

    const resultPromise = retryWithBackoff(operation, {
      maxAttempts: 3,
    });

    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(result).toEqual({
      success: false,
      error: error,
      attempts: 1,
    });
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it('should apply jitter to delays', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('transient error'))
      .mockResolvedValue('success');

    // Mock Math.random to return specific value
    const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.5);

    const resultPromise = retryWithBackoff(operation, {
      maxAttempts: 2,
      baseDelay: 1000,
      jitter: 0.2, // ±20% jitter
    });

    await vi.runAllTimersAsync();
    await resultPromise;

    // With random = 0.5, jitter offset = 1000 * 0.2 * (0.5 * 2 - 1) = 0
    // So delay should be exactly 1000ms
    expect(operation).toHaveBeenCalledTimes(2);
    randomSpy.mockRestore();
  });

  it('should use custom error detection callback', async () => {
    const customRetryable = vi.fn().mockReturnValue(true);
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('custom error'))
      .mockResolvedValue('success');

    const resultPromise = retryWithBackoff(operation, {
      maxAttempts: 2,
      baseDelay: 100,
      jitter: 0,
      retryableErrors: customRetryable,
    });

    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(customRetryable).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(operation).toHaveBeenCalledTimes(2);
  });

  it('should call onRetry callback on each retry', async () => {
    const onRetry = vi.fn();
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('transient error'))
      .mockResolvedValue('success');

    const resultPromise = retryWithBackoff(
      operation,
      {
        maxAttempts: 2,
        baseDelay: 100,
        jitter: 0,
      },
      onRetry
    );

    await vi.runAllTimersAsync();
    await resultPromise;

    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry).toHaveBeenCalledWith(1, expect.any(Error), 100);
  });

  it('should respect maxDelay cap', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('transient error'))
      .mockRejectedValueOnce(new Error('transient error'))
      .mockResolvedValue('success');

    const onRetry = vi.fn();
    const resultPromise = retryWithBackoff(
      operation,
      {
        maxAttempts: 3,
        baseDelay: 5000,
        maxDelay: 1000,
        backoffMultiplier: 10,
        jitter: 0,
      },
      onRetry
    );

    await vi.runAllTimersAsync();
    await resultPromise;

    // Both delays should be capped at maxDelay (1000ms)
    expect(onRetry).toHaveBeenNthCalledWith(1, 1, expect.any(Error), 1000);
    expect(onRetry).toHaveBeenNthCalledWith(2, 2, expect.any(Error), 1000);
  });
});

describe('retryWithTimeout', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('should timeout long operations', async () => {
    const operation = vi.fn().mockImplementation(() => {
      return new Promise((resolve) => {
        setTimeout(() => resolve('done'), 60000);
      });
    });

    const resultPromise = retryWithTimeout(operation, { maxAttempts: 1 }, 1000);

    await vi.advanceTimersByTimeAsync(1000);
    const result = await resultPromise;

    expect(result.success).toBe(false);
    expect(result.error.message).toBe('Operation timed out');
  });

  it('should succeed if operation completes within timeout', async () => {
    const operation = vi.fn().mockImplementation(() => {
      return new Promise((resolve) => {
        setTimeout(() => resolve('success'), 100);
      });
    });

    const resultPromise = retryWithTimeout(operation, { maxAttempts: 1 }, 1000);

    await vi.advanceTimersByTimeAsync(100);
    const result = await resultPromise;

    expect(result).toEqual({
      success: true,
      value: 'success',
      attempts: 1,
    });
  });
});

describe('createRetryWrapper', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('should return value on success', async () => {
    const wrapper = createRetryWrapper({ maxAttempts: 2 });
    const operation = vi.fn().mockResolvedValue('result');

    const resultPromise = wrapper(operation);
    await vi.runAllTimersAsync();
    const result = await resultPromise;

    expect(result).toBe('result');
  });

  it('should throw error after max retries', async () => {
    const wrapper = createRetryWrapper({ maxAttempts: 2, baseDelay: 100, jitter: 0 });
    const error = new Error('transient error');
    const operation = vi.fn().mockRejectedValue(error);

    const resultPromise = wrapper(operation);
    // Attach rejection handler before running timers to prevent unhandled rejection
    const assertionPromise = expect(resultPromise).rejects.toThrow('transient error');
    await vi.runAllTimersAsync();

    await assertionPromise;
  });
});

describe('CircuitBreaker', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('should start in closed state', () => {
    const breaker = new CircuitBreaker();
    expect(breaker.getState()).toBe(CircuitState.Closed);
  });

  it('should allow successful operations', async () => {
    const breaker = new CircuitBreaker();
    const operation = vi.fn().mockResolvedValue('success');

    const result = await breaker.execute(operation);

    expect(result).toBe('success');
    expect(breaker.getState()).toBe(CircuitState.Closed);
  });

  it('should open after failure threshold', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 3,
      timeout: 1000,
    });
    const error = new Error('fail');
    const operation = vi.fn().mockRejectedValue(error);

    // Trigger failures up to threshold
    for (let i = 0; i < 3; i++) {
      await expect(breaker.execute(operation)).rejects.toThrow('fail');
    }

    expect(breaker.getState()).toBe(CircuitState.Open);
  });

  it('should reject requests when open', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 1,
      timeout: 10000,
    });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    // Open the circuit
    await expect(breaker.execute(operation)).rejects.toThrow('fail');
    expect(breaker.getState()).toBe(CircuitState.Open);

    // Should reject immediately
    await expect(breaker.execute(vi.fn())).rejects.toThrow('Circuit breaker is OPEN');
  });

  it('should transition to half-open after timeout', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 1,
      timeout: 1000,
    });

    // Open the circuit
    await expect(
      breaker.execute(vi.fn().mockRejectedValue(new Error('fail')))
    ).rejects.toThrow();
    expect(breaker.getState()).toBe(CircuitState.Open);

    // Advance past timeout
    vi.advanceTimersByTime(1001);

    // Next execution should try (half-open)
    const successOp = vi.fn().mockResolvedValue('success');
    await breaker.execute(successOp);

    expect(successOp).toHaveBeenCalled();
  });

  it('should close after success threshold in half-open', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 1,
      successThreshold: 2,
      timeout: 1000,
    });

    // Open the circuit
    await expect(
      breaker.execute(vi.fn().mockRejectedValue(new Error('fail')))
    ).rejects.toThrow();

    // Advance past timeout to half-open
    vi.advanceTimersByTime(1001);

    // Execute success operations
    const successOp = vi.fn().mockResolvedValue('success');
    await breaker.execute(successOp);
    await breaker.execute(successOp);

    expect(breaker.getState()).toBe(CircuitState.Closed);
  });

  it('should return stats correctly', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 5,
      successThreshold: 3,
      timeout: 60000,
    });

    // Execute some operations
    await breaker.execute(vi.fn().mockResolvedValue('ok'));
    await expect(
      breaker.execute(vi.fn().mockRejectedValue(new Error('fail')))
    ).rejects.toThrow();

    const stats = breaker.getStats();
    expect(stats.state).toBe(CircuitState.Closed);
    expect(stats.successes).toBe(1);
    expect(stats.failures).toBe(1);
  });

  it('should re-open if failure in half-open state', async () => {
    const breaker = new CircuitBreaker({
      failureThreshold: 1,
      timeout: 1000,
    });

    // Open the circuit
    await expect(
      breaker.execute(vi.fn().mockRejectedValue(new Error('fail')))
    ).rejects.toThrow();

    // Advance past timeout to half-open
    vi.advanceTimersByTime(1001);

    // Fail again in half-open
    await expect(
      breaker.execute(vi.fn().mockRejectedValue(new Error('fail again')))
    ).rejects.toThrow();

    expect(breaker.getState()).toBe(CircuitState.Open);
  });
});

describe('DEFAULT_RETRY_CONFIG', () => {
  it('should have sensible defaults', () => {
    expect(DEFAULT_RETRY_CONFIG.maxAttempts).toBe(3);
    expect(DEFAULT_RETRY_CONFIG.baseDelay).toBe(1000);
    expect(DEFAULT_RETRY_CONFIG.maxDelay).toBe(10000);
    expect(DEFAULT_RETRY_CONFIG.backoffMultiplier).toBe(2);
    expect(DEFAULT_RETRY_CONFIG.jitter).toBe(0.1);
  });
});
