//! Automatic Retry Logic with Exponential Backoff
//!
//! Provides intelligent retry mechanisms for transient failures with user notifications.
//! Implements exponential backoff with jitter to prevent thundering herd problems.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Trust-building UX patterns
//! - ui/src/utils/errorMessages.ts L1-L50 - Error classification for retry decisions

import { isTransientError } from '@/utils/errorMessages';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';

// Lazy import to avoid circular dependencies
let retryNotificationManager: { show: (operation: string, attempt: number, maxAttempts: number, delayMs: number) => void } | null = null;
const getRetryNotificationManager = async () => {
  if (!retryNotificationManager) {
    const { retryNotificationManager: manager } = await import('@/components/ui/retry-notification');
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
  retryableErrors?: (error: unknown) => boolean; // Custom function to determine if error is retryable
}

/**
 * Validation errors for retry configuration
 */
export interface RetryConfigValidationError {
  field: string;
  message: string;
  value: unknown;
}

/**
 * Result of retry configuration validation
 */
export interface RetryConfigValidationResult {
  valid: boolean;
  errors: RetryConfigValidationError[];
}

/**
 * Validate retry configuration to ensure safe defaults
 *
 * Key validations:
 * - jitter > 0: Required for thundering herd protection
 * - maxDelay >= baseDelay: Prevents configuration errors
 * - backoffMultiplier >= 1: Ensures delays don't decrease
 * - maxAttempts >= 1: At least one attempt required
 *
 * @param config The retry configuration to validate
 * @returns Validation result with any errors
 */
export function validateRetryConfig(config: Partial<RetryConfig>): RetryConfigValidationResult {
  const errors: RetryConfigValidationError[] = [];

  // Validate jitter > 0 for thundering herd protection
  if (config.jitter !== undefined && config.jitter <= 0) {
    errors.push({
      field: 'jitter',
      message: 'Jitter must be greater than 0 for thundering herd prevention',
      value: config.jitter,
    });
  }

  // Validate jitter <= 1 (100%)
  if (config.jitter !== undefined && config.jitter > 1) {
    errors.push({
      field: 'jitter',
      message: 'Jitter must be at most 1 (100%)',
      value: config.jitter,
    });
  }

  // Validate maxDelay >= baseDelay
  if (
    config.maxDelay !== undefined &&
    config.baseDelay !== undefined &&
    config.maxDelay < config.baseDelay
  ) {
    errors.push({
      field: 'maxDelay',
      message: 'maxDelay must be greater than or equal to baseDelay',
      value: config.maxDelay,
    });
  }

  // Validate backoffMultiplier >= 1
  if (config.backoffMultiplier !== undefined && config.backoffMultiplier < 1) {
    errors.push({
      field: 'backoffMultiplier',
      message: 'backoffMultiplier must be at least 1',
      value: config.backoffMultiplier,
    });
  }

  // Validate maxAttempts >= 1
  if (config.maxAttempts !== undefined && config.maxAttempts < 1) {
    errors.push({
      field: 'maxAttempts',
      message: 'maxAttempts must be at least 1',
      value: config.maxAttempts,
    });
  }

  // Validate baseDelay > 0
  if (config.baseDelay !== undefined && config.baseDelay <= 0) {
    errors.push({
      field: 'baseDelay',
      message: 'baseDelay must be greater than 0',
      value: config.baseDelay,
    });
  }

  // Validate maxDelay > 0
  if (config.maxDelay !== undefined && config.maxDelay <= 0) {
    errors.push({
      field: 'maxDelay',
      message: 'maxDelay must be greater than 0',
      value: config.maxDelay,
    });
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Create a validated retry configuration
 * Throws if the configuration is invalid
 *
 * @param config The retry configuration to validate
 * @returns The validated configuration merged with defaults
 * @throws Error if configuration is invalid
 */
export function createValidatedRetryConfig(config: Partial<RetryConfig> = {}): RetryConfig {
  const validation = validateRetryConfig(config);
  if (!validation.valid) {
    const errorMessages = validation.errors.map(e => `${e.field}: ${e.message}`).join('; ');
    throw new Error(`Invalid retry configuration: ${errorMessages}`);
  }
  return { ...DEFAULT_RETRY_CONFIG, ...config };
}

export type RetryResult<T> =
  | {
      success: true;
      value: T;
      attempts: number;
    }
  | {
      success: false;
      error: unknown;
      attempts: number;
    };

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
  onRetry?: (attempt: number, error: unknown, delay: number) => void,
  operationName: string = 'operation'
): Promise<RetryResult<T>> {
  const finalConfig = { ...DEFAULT_RETRY_CONFIG, ...config };
  let lastError: unknown;
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
          error: error instanceof Error ? error.message : String(error)
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
        error: error instanceof Error ? error.message : String(error)
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
      error: error instanceof Error ? error.message : String(error)
    });
  }

  logger.info('Showing retry notification to user', {
    component: 'retry',
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
    }
    throw (result as { success: false; error: unknown; attempts: number }).error;
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
