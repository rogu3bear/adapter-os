//! Chat-Specific Retry Configuration
//!
//! Provides specialized retry configurations for chat-related operations.
//! Each configuration is tuned for the expected latency and failure patterns
//! of different chat operations (adapter loading, model loading, SSE reconnection).
//!
//! Citations:
//! - ui/src/utils/retry.ts L1-L50 - Base retry mechanism and RetryConfig interface
//! - docs/CHAT_ARCHITECTURE.md - Chat infrastructure and failure modes
//! - docs/Smashing Design Techniques.md L300-L350 - Resilient UX patterns

import { RetryConfig } from '@/utils/retry';

/**
 * Retry configuration for adapter loading operations
 *
 * Adapter loading can fail due to:
 * - Insufficient memory (requires unloading other adapters)
 * - Concurrent loading conflicts
 * - Temporary resource unavailability
 *
 * Conservative retry strategy with longer delays to allow
 * for memory cleanup and resource availability.
 */
export const ADAPTER_LOAD_RETRY_CONFIG: Partial<RetryConfig> = {
  maxAttempts: 3,
  baseDelay: 2000, // 2 seconds
  maxDelay: 30000, // 30 seconds
  backoffMultiplier: 2,
  jitter: 0.1
};

/**
 * Retry configuration for base model loading operations
 *
 * Model loading is a heavier operation that can fail due to:
 * - Large file downloads from cache
 * - Backend initialization (CoreML/MLX/Metal)
 * - Memory allocation for model weights
 *
 * More aggressive retry strategy with longer delays and more attempts
 * to handle the heavier operation and longer recovery time.
 */
export const MODEL_LOAD_RETRY_CONFIG: Partial<RetryConfig> = {
  maxAttempts: 5,
  baseDelay: 3000, // 3 seconds
  maxDelay: 60000, // 60 seconds (1 minute)
  backoffMultiplier: 2.5,
  jitter: 0.15
};

/**
 * Retry configuration for SSE (Server-Sent Events) reconnection
 *
 * SSE connections for streaming chat responses can be interrupted by:
 * - Network hiccups
 * - Server restarts
 * - Load balancer issues
 *
 * Fast, aggressive retry strategy with many attempts since
 * reconnection is lightweight and users expect near-instant recovery.
 */
export const SSE_RECONNECT_RETRY_CONFIG: Partial<RetryConfig> = {
  maxAttempts: 10,
  baseDelay: 1000, // 1 second
  maxDelay: 30000, // 30 seconds
  backoffMultiplier: 2,
  jitter: 0.2 // Higher jitter to prevent thundering herd on reconnect
};

/**
 * All chat retry configurations grouped for convenience
 */
export const CHAT_RETRY_CONFIG = {
  adapterLoad: ADAPTER_LOAD_RETRY_CONFIG,
  modelLoad: MODEL_LOAD_RETRY_CONFIG,
  sseReconnect: SSE_RECONNECT_RETRY_CONFIG,
} as const;
