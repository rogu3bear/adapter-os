// API Response Helpers
//
// Centralized response handling utilities to reduce duplication in API client.
// These helpers extract common patterns for JSON parsing, error handling, and status checks.

import { logger, toError } from '@/utils/logger';
import type { ErrorResponse, PaginatedResponse } from '@/api/api-types';

/**
 * API error with extended properties for better error handling
 */
export interface ApiError extends Error {
  code?: string;
  status?: number;
  details?: Record<string, unknown>;
}

/**
 * Handle JSON response with automatic error checking and parsing
 *
 * Handles:
 * - 204 No Content (returns empty object)
 * - Error responses (throws ApiError)
 * - JSON parsing errors (throws with logging)
 *
 * @param response - Fetch response object
 * @param context - Context for error logging (method, path, requestId)
 * @returns Parsed JSON of type T
 * @throws ApiError if response is not ok or JSON parsing fails
 *
 * @example
 * const response = await fetch(url);
 * const data = await handleJsonResponse<MyType>(response, { method: 'GET', path: '/api/foo', requestId: '123' });
 */
export async function handleJsonResponse<T>(
  response: Response,
  context: { method: string; path: string; requestId?: string }
): Promise<T> {
  // Check response status
  if (!response.ok) {
    await throwOnError(response, context);
  }

  // Handle 204 No Content
  if (response.status === 204) {
    return {} as T;
  }

  // Parse JSON
  try {
    return await response.json();
  } catch (parseError) {
    const error = toError(parseError);
    logger.error('API response JSON parse error', {
      component: 'ApiClient',
      operation: 'handleJsonResponse',
      method: context.method,
      path: context.path,
      requestId: context.requestId,
      status: response.status,
    }, error);
    throw error;
  }
}

/**
 * Handle void response (204 No Content expected)
 *
 * Use for DELETE, POST, PUT operations that return no content.
 *
 * @param response - Fetch response object
 * @param context - Context for error logging
 * @throws ApiError if response is not ok
 *
 * @example
 * const response = await fetch(url, { method: 'DELETE' });
 * await handleVoidResponse(response, { method: 'DELETE', path: '/api/foo', requestId: '123' });
 */
export async function handleVoidResponse(
  response: Response,
  context: { method: string; path: string; requestId?: string }
): Promise<void> {
  if (!response.ok) {
    await throwOnError(response, context);
  }
  // No return value needed for void responses
}

/**
 * Handle blob response (for file downloads)
 *
 * Use for endpoints that return binary data (PDFs, images, etc.)
 *
 * @param response - Fetch response object
 * @param context - Context for error logging
 * @returns Blob data
 * @throws Error if response is not ok
 *
 * @example
 * const response = await fetch(url);
 * const blob = await handleBlobResponse(response, { method: 'GET', path: '/api/download', requestId: '123' });
 */
export async function handleBlobResponse(
  response: Response,
  context: { method: string; path: string; requestId?: string }
): Promise<Blob> {
  if (!response.ok) {
    // Try to extract error message from JSON response
    let errorMessage = 'Failed to download file';
    try {
      const error = await response.json();
      if (error && typeof error === 'object' && 'error' in error) {
        errorMessage = String(error.error);
      }
    } catch {
      // If JSON parsing fails, use default message
      errorMessage = response.statusText || errorMessage;
    }

    logger.error('API blob response error', {
      component: 'ApiClient',
      operation: 'handleBlobResponse',
      method: context.method,
      path: context.path,
      requestId: context.requestId,
      status: response.status,
      errorMessage,
    });

    throw new Error(errorMessage);
  }

  return response.blob();
}

/**
 * Throw an enhanced error from a failed response
 *
 * Internal helper that extracts error details from response and throws ApiError.
 * Attempts to parse error response as JSON to get detailed error information.
 *
 * @param response - Failed fetch response
 * @param context - Context for error logging
 * @throws ApiError with detailed error information
 */
async function throwOnError(
  response: Response,
  context: { method: string; path: string; requestId?: string }
): Promise<never> {
  let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
  let errorCode: string | undefined;
  let errorDetails: Record<string, unknown> = {};

  // Try to parse error response as JSON
  try {
    const error: ErrorResponse = await response.json();
    errorMessage = error.error || errorMessage;
    errorCode = error.code;
    // ErrorResponse.details is a string, convert to object for ApiError
    if (error.details) {
      errorDetails = { message: error.details };
    }
  } catch {
    // If JSON parsing fails, use status text
  }

  const apiError = new Error(errorMessage) as ApiError;
  apiError.code = errorCode;
  apiError.status = response.status;
  apiError.details = errorDetails;

  logger.error('API request HTTP error', {
    component: 'ApiClient',
    operation: 'throwOnError',
    method: context.method,
    path: context.path,
    requestId: context.requestId,
    status: response.status,
    statusText: response.statusText,
    errorCode,
  }, apiError);

  throw apiError;
}

/**
 * Extract filename from Content-Disposition header
 *
 * Utility for file download endpoints to get the suggested filename.
 *
 * @param response - Fetch response with Content-Disposition header
 * @param fallback - Fallback filename if header is not present
 * @returns Extracted filename or fallback
 *
 * @example
 * const filename = getFilenameFromResponse(response, 'download.bin');
 */
export function getFilenameFromResponse(response: Response, fallback: string): string {
  const contentDisposition = response.headers.get('Content-Disposition');
  if (contentDisposition) {
    const filenameMatch = contentDisposition.match(/filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/);
    if (filenameMatch && filenameMatch[1]) {
      return filenameMatch[1].replace(/['"]/g, '');
    }
  }
  return fallback;
}

/**
 * Validate request ID match between sent and received
 *
 * Logs warning if request IDs don't match (potential security/consistency issue).
 *
 * @param response - Fetch response with X-Request-ID header
 * @param sentRequestId - Request ID that was sent
 */
export function validateRequestId(response: Response, sentRequestId: string): void {
  const returnedId = response.headers.get('X-Request-ID');
  if (returnedId && returnedId !== sentRequestId) {
    logger.warn('Request ID mismatch', {
      component: 'ApiClient',
      operation: 'request_validation',
      sent: sentRequestId,
      received: returnedId
    });
  }
}

/**
 * Extract array from paginated or direct array response.
 *
 * DEFENSIVE: Handles multiple response formats to prevent future bugs when
 * backend endpoints migrate to PaginatedResponse.
 *
 * Supported formats (checked in order):
 * - PaginatedResponse: { data: T[], total, page, limit, pages }
 * - Legacy wrapper: { items: T[] }
 * - Domain wrappers: { models: T[] }, { logs: T[] }
 * - Direct array: T[]
 *
 * @param response - Unknown response that may be array or wrapper
 * @returns Extracted array, or empty array if extraction fails
 *
 * @example
 * const items = extractArrayFromResponse<User>(response);
 */
export function extractArrayFromResponse<T>(response: unknown): T[] {
  // PaginatedResponse format: { data: T[], total, page, limit, pages }
  if (response && typeof response === 'object' && 'data' in response) {
    const paginated = response as PaginatedResponse<T>;
    if (Array.isArray(paginated.data)) {
      return paginated.data;
    }
  }

  // Legacy { items: T[] } format (for backwards compatibility)
  if (response && typeof response === 'object' && 'items' in response) {
    const legacy = response as { items: T[] };
    if (Array.isArray(legacy.items)) {
      return legacy.items;
    }
  }

  // Domain-specific wrappers: { models: T[] }
  if (response && typeof response === 'object' && 'models' in response) {
    const wrapper = response as { models: T[] };
    if (Array.isArray(wrapper.models)) {
      return wrapper.models;
    }
  }

  // Domain-specific wrappers: { logs: T[] }
  if (response && typeof response === 'object' && 'logs' in response) {
    const wrapper = response as { logs: T[] };
    if (Array.isArray(wrapper.logs)) {
      return wrapper.logs;
    }
  }

  // Direct array response
  if (Array.isArray(response)) {
    return response as T[];
  }

  // Fallback: return empty array (prevents .map errors on undefined/null)
  return [];
}
