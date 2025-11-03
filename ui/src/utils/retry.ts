//! Automatic Retry Logic with Exponential Backoff
//!
//! Provides intelligent retry mechanisms for transient failures with user notifications.
//! Implements exponential backoff with jitter to prevent thundering herd problems.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Trust-building UX patterns
//! - ui/src/utils/errorMessages.ts L1-L50 - Error classification for retry decisions

import { isTransientError } from './errorMessages';
import { logger } from '../utils/logger';
import { toast } from 'sonner';

// Lazy import to avoid circular dependencies
let retryNotificationManager: any = null;
const getRetryNotificationManager = async () => {
  if (!retryNotificationManager) {
    const { retryNotificationManager: manager } = await import('../components/ui/retry-notification');
    retryNotificationManager = manager;
  }
  return retryNotificationManager;
};

export interface RetryConfig {
  maxAttempts: number;
  baseDelay: number; // Base delay in milliseconds
  maxDelay: number; // Maximum delay in milliseconds
  backoffMultiplier: number; // Exponential backoff multiplier
  jitter: number; // Jitter factor (0-1, e.g., 0.1 = ±10% jitter)
  retryableErrors?: (error: any) => boolean; // Custom function to determine if error is retryable
}

export interface RetryResult<T> {
  success: true;
  value: T;
  attempts: number;
} | {
  success: false;
  error: any;
  attempts: number;
}

// Default retry configuration
export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxAttempts: 3,
  baseDelay: 1000, // 1 second
  maxDelay: 10000, // 10 seconds
  backoffMultiplier: 2,
  jitter: 0.1, // ±10% jitter
  retryableErrors: isTransientError
};

/**
 * Calculate delay with exponential backoff and jitter
 */
function calculateDelay(attempt: number, config: RetryConfig): number {
  const exponentialDelay = config.baseDelay * Math.pow(config.backoffMultiplier, attempt - 1);
  const jitterOffset = exponentialDelay * config.jitter * (Math.random() * 2 - 1); // ±jitter
  const delay = Math.min(exponentialDelay + jitterOffset, config.maxDelay);
  return Math.max(0, delay); // Ensure non-negative
}

/**
 * Sleep for the specified number of milliseconds
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Execute an operation with automatic retry logic
 */
export async function retryWithBackoff<T>(
  operation: () => Promise<T>,
  config: Partial<RetryConfig> = {},
  onRetry?: (attempt: number, error: any, delay: number) => void,
  operationName: string = 'operation'
): Promise<RetryResult<T>> {
  const finalConfig = { ...DEFAULT_RETRY_CONFIG, ...config };
  let lastError: any;
  let attempt = 0;

  while (attempt < finalConfig.maxAttempts) {
    attempt++;

    try {
      const result = await operation();

      if (attempt > 1) {
        logger.info('Operation succeeded after retry', {
          component: 'retry',
          operation: 'retryWithBackoff',
          attempts: attempt,
          success: true
        });
      }

      return {
        success: true,
        value: result,
        attempts: attempt
      };
    } catch (error) {
      lastError = error;

      // Check if we should retry this error
      const shouldRetry = finalConfig.retryableErrors
        ? finalConfig.retryableErrors(error)
        : isTransientError(error);

      if (!shouldRetry || attempt >= finalConfig.maxAttempts) {
        logger.warn('Operation failed, not retrying', {
          component: 'retry',
          operation: 'retryWithBackoff',
          attempts: attempt,
          maxAttempts: finalConfig.maxAttempts,
          shouldRetry,
          error: error.message
        });
        break;
      }

      const delay = calculateDelay(attempt, finalConfig);

      logger.info('Operation failed, retrying', {
        component: 'retry',
        operation: 'retryWithBackoff',
        attempt,
        maxAttempts: finalConfig.maxAttempts,
        delay,
        error: error.message
      });

      // Notify about retry
      if (onRetry) {
        onRetry(attempt, error, delay);
      } else {
        // Default notification
        showRetryNotification(operationName, attempt, finalConfig.maxAttempts, delay);
      }

      // Wait before retrying
      await sleep(delay);
    }
  }

  return {
    success: false,
    error: lastError,
    attempts: attempt
  };
}

/**
 * Show user notification about retry attempt
 */
async function showRetryNotification(operation: string, attempt: number, maxAttempts: number, delayMs: number): Promise<void> {
  try {
    const manager = await getRetryNotificationManager();
    manager.show(operation, attempt, maxAttempts, delayMs);
  } catch (error) {
    // Fallback to toast if notification manager fails
    const delaySeconds = Math.round(delayMs / 1000);
    const message = `Retrying ${operation}... (attempt ${attempt}/${maxAttempts})`;

    toast.info(message, {
      description: `Will retry in ${delaySeconds} second${delaySeconds !== 1 ? 's' : ''}`,
      duration: delayMs + 1000,
    });

    logger.warn('Failed to show retry notification, using fallback', {
      component: 'retry',
      operation: 'showRetryNotification',
      error: error.message
    });
  }

  logger.info('Showing retry notification to user', {
    component: 'retry',
    operation: 'showRetryNotification',
    operation: operation,
    attempt,
    maxAttempts,
    delayMs
  });
}

/**
 * Retry operation with timeout
 */
export async function retryWithTimeout<T>(
  operation: () => Promise<T>,
  config: Partial<RetryConfig> = {},
  timeoutMs: number = 30000 // 30 seconds default timeout
): Promise<RetryResult<T>> {
  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => reject(new Error('Operation timed out')), timeoutMs);
  });

  const retryOperation = () => Promise.race([operation(), timeoutPromise]);

  return retryWithBackoff(retryOperation, config);
}

/**
 * Create a retry wrapper for API operations
 */
export function createRetryWrapper(config: Partial<RetryConfig> = {}) {
  return async function<T>(operation: () => Promise<T>): Promise<T> {
    const result = await retryWithBackoff(operation, config);

    if (result.success) {
      return result.value;
    } else {
      throw result.error;
    }
  };
}

/**
 * Circuit breaker state for protecting against cascading failures
 */
export enum CircuitState {
  Closed = 'closed',     // Normal operation
  Open = 'open',         // Failing, reject requests
  HalfOpen = 'half_open' // Testing if service recovered
}

export interface CircuitBreakerConfig {
  failureThreshold: number;  // Number of failures before opening
  successThreshold: number;  // Number of successes before closing
  timeout: number;          // Time in ms before attempting half-open
}

/**
 * Circuit breaker for protecting against cascading failures
 */
export class CircuitBreaker {
  private state: CircuitState = CircuitState.Closed;
  private failures = 0;
  private successes = 0;
  private nextAttempt = 0;
  private config: CircuitBreakerConfig;

  constructor(config: Partial<CircuitBreakerConfig> = {}) {
    this.config = {
      failureThreshold: 5,
      successThreshold: 3,
      timeout: 60000, // 1 minute
      ...config
    };
  }

  async execute<T>(operation: () => Promise<T>): Promise<T> {
    if (this.state === CircuitState.Open) {
      if (Date.now() < this.nextAttempt) {
        throw new Error('Circuit breaker is OPEN');
      }
      this.state = CircuitState.HalfOpen;
    }

    try {
      const result = await operation();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      throw error;
    }
  }

  private onSuccess(): void {
    this.successes++;

    if (this.state === CircuitState.HalfOpen && this.successes >= this.config.successThreshold) {
      this.reset();
    }
  }

  private onFailure(): void {
    this.failures++;

    if (this.failures >= this.config.failureThreshold) {
      this.state = CircuitState.Open;
      this.nextAttempt = Date.now() + this.config.timeout;

      logger.warn('Circuit breaker opened', {
        component: 'retry',
        operation: 'circuitBreaker',
        failures: this.failures,
        timeout: this.config.timeout
      });
    }
  }

  private reset(): void {
    this.state = CircuitState.Closed;
    this.failures = 0;
    this.successes = 0;

    logger.info('Circuit breaker reset', {
      component: 'retry',
      operation: 'circuitBreaker'
    });
  }

  getState(): CircuitState {
    return this.state;
  }

  getStats(): { state: CircuitState; failures: number; successes: number; nextAttempt: number } {
    return {
      state: this.state,
      failures: this.failures,
      successes: this.successes,
      nextAttempt: this.nextAttempt
    };
  }
}
