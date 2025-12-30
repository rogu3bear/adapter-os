// API Client for AdapterOS Control Plane
//!
//! Provides centralized API communication with structured logging and error handling.
//!
//! # Citations
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"
//!
//! NOTE: Domain-specific API methods are now in ui/src/api/services/*.ts
//! This file contains only the core infrastructure (request, retry, token management).

import { logger, toError } from '@/utils/logger';
import { enhanceError, isTransientError, isTimeoutError } from '@/utils/errorMessages';
import { extractArrayFromResponse } from '@/api/helpers';
import { retryWithBackoff, RetryConfig } from '@/utils/retry';
import { captureException } from '@/stores/errorStore';
import { markBackendReachable, markBackendUnreachable } from '@/stores/backendReachability';
import { markSessionExpired } from '@/auth/session';
import { AUTH_STORAGE_KEYS } from '@/auth/constants';
import { createTokenCoordinator, TokenCoordinator } from '@/auth/tokenCoordination';
import { toCamelCase, toSnakeCase } from './transformers';

// Type-safe API error with extended properties
export interface ApiError extends Error {
  code?: string;
  status?: number;
  details?: Record<string, unknown>;
  detail?: string;
  requestId?: string;
}

const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

function readSelectedTenantId(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = sessionStorage.getItem(AUTH_STORAGE_KEYS.SELECTED_TENANT);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { tenantId?: unknown };
    const tenantId = typeof parsed.tenantId === 'string' ? parsed.tenantId.trim() : '';
    return tenantId ? tenantId : null;
  } catch {
    return null;
  }
}

class ApiClient {
  private baseUrl: string;
  private requestLog: Array<{ id: string; method: string; path: string; timestamp: string }> = [];
  private retryConfig: RetryConfig;
  private token?: string;
  private refreshPromise: Promise<void> | null = null;
  private pendingRequests: Set<{ reject: (error: Error) => void }> = new Set();
  private tokenCoordinator: TokenCoordinator;

  constructor(baseUrl: string = API_BASE_URL, retryConfig?: Partial<RetryConfig>) {
    this.baseUrl = baseUrl;
    this.retryConfig = {
      maxAttempts: 3,
      baseDelay: 1000,
      maxDelay: 10000,
      backoffMultiplier: 2,
      jitter: 0.1,
      retryableErrors: isTransientError,
      ...retryConfig
    };
    this.tokenCoordinator = createTokenCoordinator();
    logger.info('API Client initialized', {
      component: 'ApiClient',
      operation: 'constructor',
      baseUrl: this.baseUrl,
      retryEnabled: true
    });
  }

  setToken(token: string) {
    this.token = token;
  }

  getToken(): string | undefined {
    return this.token;
  }

  clearToken(): void {
    this.token = undefined;
  }

  /**
   * Get the base URL for the API.
   * Used by services that need direct fetch access (e.g., file uploads).
   */
  getBaseUrl(): string {
    return this.baseUrl;
  }

  /**
   * Transform request body to snake_case for backend compatibility.
   * Handles JSON objects, arrays, and FormData.
   */
  private transformRequestBody(body: unknown): unknown {
    // Skip transformation for FormData, Blob, File, or primitives
    if (
      body === null ||
      body === undefined ||
      typeof body !== 'object' ||
      (typeof FormData !== 'undefined' && body instanceof FormData) ||
      (typeof Blob !== 'undefined' && body instanceof Blob) ||
      (typeof File !== 'undefined' && body instanceof File)
    ) {
      return body;
    }

    return toSnakeCase(body);
  }

  /**
   * Transform response data from snake_case to camelCase.
   * Note: T represents the desired camelCase output type.
   * The transformation is applied but the result is cast to T.
   */
  private transformResponseData<T>(data: unknown): T {
    return toCamelCase(data) as T;
  }

  /**
   * Compute a request ID for audit logging.
   * Public wrapper for use by domain services.
   */
  async createRequestId(method: string, path: string, body: string): Promise<string> {
    return this.computeRequestId(method, path, body);
  }

  /**
   * Record a request in the audit log.
   * Public wrapper for use by domain services.
   */
  recordRequest(id: string, method: string, path: string): void {
    this.logRequest(id, method, path);
  }

  private broadcastSessionExpiry(): void {
    const error = new Error('Session expired') as ApiError;
    error.code = 'SESSION_EXPIRED';
    error.status = 401;

    // Reject all pending requests waiting on refresh
    for (const pending of this.pendingRequests) {
      pending.reject(error);
    }
    this.pendingRequests.clear();
  }

  private async performRefresh(): Promise<void> {
    // Wait if another tab is already refreshing
    if (this.tokenCoordinator.isRefreshInProgress()) {
      await this.tokenCoordinator.waitForActiveRefresh();
      return; // Other tab handled it
    }

    if (this.refreshPromise) {
      return this.refreshPromise;
    }

    const hadBearerToken = Boolean(this.token);

    this.tokenCoordinator.broadcastRefreshStart();

    this.refreshPromise = (async () => {
      const refreshUrl = `${this.baseUrl}/v1/auth/refresh`;

      // Read CSRF token from cookie for auth endpoint
      const readCookie = (name: string): string | undefined => {
        if (typeof document === 'undefined') return undefined;
        const cookies = document.cookie?.split(';') ?? [];
        const prefix = `${name}=`;
        for (const raw of cookies) {
          const trimmed = raw.trim();
          if (trimmed.startsWith(prefix)) {
            return decodeURIComponent(trimmed.slice(prefix.length));
          }
        }
        return undefined;
      };
      const csrfToken = readCookie('csrf_token');

      const resp = await fetch(refreshUrl, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
          ...(csrfToken ? { 'X-CSRF-Token': csrfToken } : {}),
        },
      });

      if (!resp.ok) {
        this.token = undefined;
        // Broadcast failure to all pending requests before marking session expired
        this.broadcastSessionExpiry();
        // Only signal expiry when we previously held a bearer token; initial unauthenticated
        // loads (no token yet) should fail quietly without forcing a logout loop.
        if (hadBearerToken) {
          markSessionExpired();
        }
        const err = new Error('Session expired') as ApiError;
        err.code = 'SESSION_EXPIRED';
        err.status = resp.status;
        throw err;
      }

      try {
        const rawBody = await resp.json();
        const body = this.transformResponseData<{ token?: string }>(rawBody);
        if (body && typeof body === 'object' && 'token' in body && typeof body.token === 'string') {
          // Keep bearer token in memory synchronized with refreshed session
          this.token = body.token;
        }
      } catch {
        // If parsing fails, still proceed; cookies were refreshed by the server
      }
    })();

    try {
      await this.refreshPromise;
      this.tokenCoordinator.broadcastRefreshComplete();
    } catch (e) {
      this.tokenCoordinator.broadcastRefreshFailed();
      throw e;
    } finally {
      this.refreshPromise = null;
    }
  }

  private async computeRequestId(method: string, path: string, body: string): Promise<string> {
    const canonical = `${method}:${path}:${body}`;
    const encoder = new TextEncoder();
    const data = encoder.encode(canonical);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('').substring(0, 32);
  }

  private logRequest(id: string, method: string, path: string) {
    this.requestLog.push({
      id,
      method,
      path,
      timestamp: new Date().toISOString(),
    });
    // Keep last 1000 requests
    if (this.requestLog.length > 1000) {
      this.requestLog.shift();
    }
  }

  public getRequestLog() {
    return this.requestLog;
  }

  public buildUrl(path: string): string {
    if (/^https?:\/\//i.test(path)) {
      return path;
    }

    const base = this.baseUrl.replace(/\/$/, '');
    const relative = path.startsWith('/') ? path : `/${path}`;
    if (!base || base === '') {
      return relative;
    }
    return `${base}${relative}`;
  }

  /**
   * Make an API request with automatic retry and token refresh.
   *
   * @param path - API endpoint path
   * @param options - Fetch request options
   * @param skipRetry - Skip retry logic
   * @param cancelToken - AbortSignal for cancellation
   * @param allowMutationRetry - Allow retrying mutations (POST/PUT/PATCH/DELETE)
   * @param includeCredentials - If true, sends cookies (for auth endpoints). Default false (Bearer-only).
   */
  async request<T>(
    path: string,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal,
    allowMutationRetry: boolean = false,
    includeCredentials: boolean = false
  ): Promise<T> {
    const method = options.method || 'GET';

    // Configure retry based on HTTP method and explicit permission
    // GET requests are safe to retry, mutations need explicit permission
    const shouldRetry = !skipRetry && (method === 'GET' || method === 'HEAD' || allowMutationRetry);
    const operationConfig = shouldRetry ? this.retryConfig : {
      ...this.retryConfig,
      maxAttempts: 1 // No retry for mutations unless explicitly enabled
    };

    const operation = async (): Promise<T> => {
      return this.executeRequest(path, options, cancelToken, false, includeCredentials);
    };

    const result = await retryWithBackoff(operation, operationConfig, (attempt, error, delay) => {
      logger.info('Retrying API request', {
        component: 'ApiClient',
        operation: 'request',
        method,
        path,
        attempt,
        delay
      });
    }, `${method} ${path}`);

    if (result.success) {
      return result.value;
    } else {
      throw (result as { success: false; error: Error; attempts: number }).error;
    }
  }

  private async executeRequest<T>(
    path: string,
    options: RequestInit = {},
    cancelToken?: AbortSignal,
    attemptedRefresh: boolean = false,
    includeCredentials: boolean = false
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;

    // Compute deterministic request ID
    const method = options.method || 'GET';

    // Transform request body to snake_case if it's a JSON object
    let transformedBody = options.body;
    if (typeof options.body === 'string' && options.body.length > 0) {
      try {
        const parsed = JSON.parse(options.body);
        const snakeCaseBody = this.transformRequestBody(parsed);
        transformedBody = JSON.stringify(snakeCaseBody);
      } catch {
        // Not JSON, use as-is
        transformedBody = options.body;
      }
    } else if (
      options.body &&
      typeof options.body === 'object' &&
      !(options.body instanceof FormData) &&
      !(typeof Blob !== 'undefined' && options.body instanceof Blob)
    ) {
      // Object but not FormData/Blob - transform and stringify
      const snakeCaseBody = this.transformRequestBody(options.body);
      transformedBody = JSON.stringify(snakeCaseBody);
    }

    const body = transformedBody || '';
    const requestId = await this.computeRequestId(method, path, body.toString());
    const readCookie = (name: string): string | undefined => {
      if (typeof document === 'undefined') return undefined;
      const cookies = document.cookie?.split(';') ?? [];
      const prefix = `${name}=`;
      for (const raw of cookies) {
        const trimmed = raw.trim();
        if (trimmed.startsWith(prefix)) {
          return decodeURIComponent(trimmed.slice(prefix.length));
        }
      }
      return undefined;
    };

    const hasAuthHeader = (() => {
      if (!options.headers) return false;
      if (options.headers instanceof Headers) {
        return options.headers.has('Authorization');
      }
      if (Array.isArray(options.headers)) {
        return options.headers.some(([key]) => key?.toString().toLowerCase() === 'authorization');
      }
      return Object.keys(options.headers).some(key => key.toLowerCase() === 'authorization');
    })();

    const isFormDataBody =
      typeof FormData !== 'undefined' &&
      options.body instanceof FormData;

    const headers: HeadersInit = {
      ...(isFormDataBody ? {} : { 'Content-Type': 'application/json' }),
      'X-Request-ID': requestId,
      ...(this.token && !hasAuthHeader ? { Authorization: `Bearer ${this.token}` } : {}),
      ...options.headers,
    };
    const unsafeMethod = ['POST', 'PUT', 'PATCH', 'DELETE'].includes(method.toUpperCase());
    if (unsafeMethod) {
      const csrfToken = readCookie('csrf_token');
      if (csrfToken && !(headers as Record<string, string>)['X-CSRF-Token']) {
        (headers as Record<string, string>)['X-CSRF-Token'] = csrfToken;
      }
    }

    const selectedTenantId = readSelectedTenantId();
    if (selectedTenantId) {
      const headersRecord = headers as Record<string, string>;
      const hasTenantHeader = Object.keys(headersRecord).some((key) => key.toLowerCase() === 'x-tenant-id');
      if (!hasTenantHeader) {
        headersRecord['X-Tenant-Id'] = selectedTenantId;
      }
    }

    // Store in local audit buffer
    this.logRequest(requestId, method, path);

    let response: Response;
    try {
      response = await fetch(url, {
        ...options,
        body: transformedBody,
        headers,
        // Auth endpoints use 'include' for cookies; all others use 'omit' (Bearer-only)
        credentials: includeCredentials ? 'include' : 'omit',
        signal: cancelToken, // Add cancellation support
      });
    } catch (networkError) {
      // Network error (connection failure, timeout, etc.)
      const error = toError(networkError);
      if (error.name !== 'AbortError' && error.name !== 'CanceledError') {
        markBackendUnreachable(error, { method, path });
      }
      logger.error('API request network error', {
        component: 'ApiClient',
        operation: 'executeRequest',
        method,
        path,
        requestId,
      }, error);

      // Capture to dev error store (only active in dev mode)
      if (import.meta.env.DEV) {
        captureException(error, {
          component: 'ApiClient',
          operation: `${method} ${path}`,
          extra: { requestId, networkError: true },
        });
      }

      throw error;
    }

    // Treat gateway/unavailable responses as "unreachable" (common when dev proxy can't reach backend).
    if (response.status === 502 || response.status === 503 || response.status === 504) {
      markBackendUnreachable(
        new Error(`HTTP ${response.status}: ${response.statusText || 'Backend unavailable'}`),
        { method, path, status: response.status },
      );
    } else {
      markBackendReachable();
    }

    // Validate returned request ID matches
    const returnedId = response.headers.get('X-Request-ID');
    if (returnedId && returnedId !== requestId) {
      logger.warn('Request ID mismatch', {
        component: 'ApiClient',
        operation: 'request_validation',
        sent: requestId,
        received: returnedId
      });
    }

    // Track request ID for error correlation
    if (returnedId) {
      logger.trackRequestId(returnedId);
    }

    if (!response.ok) {
      // Attempt single silent refresh on 401 before surfacing error
      if (
        response.status === 401 &&
        !attemptedRefresh &&
        !path.startsWith('/v1/auth/login') &&
        !path.startsWith('/v1/auth/refresh')
      ) {
        // Register this request as pending during refresh
        let pendingRequest: { reject: (error: Error) => void } | null = null;
        const refreshPromise = new Promise<void>((_, reject) => {
          pendingRequest = { reject };
          this.pendingRequests.add(pendingRequest);
        });

        try {
          // Race between actual refresh completing and session expiry broadcast
          await Promise.race([
            this.performRefresh(),
            refreshPromise
          ]);
          // Refresh succeeded, retry the request
          return await this.executeRequest<T>(path, options, cancelToken, true, includeCredentials);
        } catch (refreshError) {
          const err = toError(refreshError) as ApiError;
          err.code = err.code || 'SESSION_EXPIRED';
          err.status = err.status || 401;
          if (err.code === 'SESSION_EXPIRED') {
            markSessionExpired();
          }
          throw err;
        } finally {
          // Clean up pending request registration
          if (pendingRequest) {
            this.pendingRequests.delete(pendingRequest);
          }
        }
      }

      let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
      let errorCode: string | undefined;
      let errorDetails: Record<string, unknown> = {};
      let errorDetail: string | undefined;
      let correlatedRequestId: string | undefined = returnedId || requestId;

      try {
        const rawParsed = await response.json();
        // Transform error response from snake_case to camelCase
        const parsed = this.transformResponseData<{
          code?: unknown;
          message?: unknown;
          detail?: unknown;
          requestId?: unknown;
          error?: unknown;
          details?: unknown;
        }>(rawParsed);

        if (parsed && typeof parsed === 'object') {
          if (typeof parsed.message === 'string') {
            errorMessage = parsed.message;
          } else if (typeof parsed.error === 'string') {
            errorMessage = parsed.error;
          }

          if (typeof parsed.code === 'string') {
            errorCode = parsed.code;
          } else if (typeof parsed.error === 'string') {
            errorCode = parsed.error;
          }

          // Handle detail (singular string field)
          if (typeof parsed.detail === 'string') {
            errorDetail = parsed.detail;
          } else if (typeof parsed.details === 'string') {
            // Fallback: some APIs use 'details' as a string message
            errorDetail = parsed.details;
          }

          // Handle details (plural object field) - separate from detail
          if (parsed.details && typeof parsed.details === 'object') {
            errorDetails = parsed.details as Record<string, unknown>;
          }

          if (typeof parsed.requestId === 'string') {
            correlatedRequestId = parsed.requestId;
          }
        }
      } catch (parseErr) {
        if (import.meta.env.DEV) {
          // eslint-disable-next-line no-console -- intentional dev-mode logging
          console.debug('[ApiClient] Failed to parse error response JSON:', parseErr);
        }
        // Continue with status text fallback
      }

      const originalError = new Error(errorMessage) as ApiError;
      originalError.code = errorCode;
      originalError.status = response.status;
      originalError.details = errorDetails;
      originalError.detail = errorDetail;
      originalError.requestId = correlatedRequestId;

      // Extract context from request for better error messages
      const context: Record<string, unknown> = {
        operation: path.split('/').pop(),
        method,
        path,
      };

      // Extract adapter ID from path if present
      const adapterMatch = path.match(/\/adapters\/([^\/]+)/);
      if (adapterMatch) {
        context.adapterId = adapterMatch[1];
      }

      // Extract model ID from path if present
      const modelMatch = path.match(/\/models\/([^\/]+)/);
      if (modelMatch) {
        context.modelId = modelMatch[1];
      }

      // Extract training job ID from path if present
      const trainingMatch = path.match(/\/training\/[^\/]+\/([^\/]+)/);
      if (trainingMatch) {
        context.jobId = trainingMatch[1];
      }

      // Extract file size from FormData if present
      if (options.body instanceof FormData) {
        const file = options.body.get('file') as File;
        if (file) {
          context.fileSize = file.size;
          context.fileName = file.name;
        }
      }

      // Extract memory requirements from request body if present
      if (typeof options.body === 'string') {
        try {
          const bodyData = JSON.parse(options.body);
          if (bodyData.memory_bytes) {
            context.memoryRequired = bodyData.memory_bytes;
          }
          if (bodyData.tenant_id) {
            context.tenantId = bodyData.tenant_id;
          }
        } catch {
          // Ignore JSON parse errors for context extraction
        }
      }

      // Enhance error with user-friendly messaging
      const requestIdForLog = correlatedRequestId || returnedId || requestId;
      const enhancedError = enhanceError(originalError, context);
      enhancedError.requestId = requestIdForLog;
      // Preserve detail/details fields from original error
      if (originalError.detail) {
        (enhancedError as ApiError).detail = originalError.detail;
      }
      if (originalError.details) {
        (enhancedError as ApiError).details = originalError.details;
      }

      if (import.meta.env.DEV) {
        console.error('API error', {
          path,
          method,
          status: response.status,
          code: errorCode,
          requestId: requestIdForLog,
          detail: errorDetail,
          details: Object.keys(errorDetails).length ? errorDetails : undefined,
        });
      }

      // Log both original and enhanced error details with network context
      logger.networkError('API request failed', {
        component: 'ApiClient',
        operation: 'request',
        method,
        path,
        requestId: requestIdForLog,
        status: response.status,
        statusText: response.statusText,
        errorCode,
        userFriendlyTitle: enhancedError.userFriendly.title,
        isTransient: isTransientError(enhancedError),
        userJourney: (context.operation as string | undefined) || 'api_request',
        detail: errorDetail,
        ...context // Include extracted context like adapterId, fileSize, etc.
      }, {
        status: response.status,
        statusText: response.statusText,
        url: path,
        method,
        connectionError: response.status === 0,
        timeout: isTimeoutError(originalError)
      }, originalError);

      // Capture to dev error store (only active in dev mode)
      if (import.meta.env.DEV) {
        captureException(enhancedError, {
          component: 'ApiClient',
          operation: `${method} ${path}`,
          extra: {
            requestId: requestIdForLog,
            status: response.status,
            detail: errorDetail,
            ...context,
          },
        });
      }

      throw enhancedError;
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return {} as T;
    }

    const rawBody = await response.text();
    if (!rawBody || rawBody.trim() === '') {
      return {} as T;
    }

    try {
      const parsed = JSON.parse(rawBody);
      // Transform response from snake_case to camelCase
      return this.transformResponseData<T>(parsed);
    } catch (parseError) {
      // JSON parsing error - enhance with user-friendly messaging
      const originalError = toError(parseError);
      const contentType = response.headers.get('content-type') || '';

      // Some endpoints may return empty or non-JSON bodies on success (e.g., legacy handlers).
      if (!contentType.toLowerCase().includes('json')) {
        logger.warn('Received non-JSON response, returning empty object', {
          component: 'ApiClient',
          operation: 'request',
          method,
          path,
          requestId,
          contentType,
        });
        return {} as T;
      }

      // Create enhanced error with PARSE_ERROR code
      const enhancedError = new Error('Invalid response from server') as ApiError;
      enhancedError.code = 'PARSE_ERROR';
      enhancedError.status = response.status;
      enhancedError.details = {
        originalMessage: originalError.message,
        responseStatus: response.status,
        contentType,
        bodyPreview: rawBody.slice(0, 200),
      };

      // Enhance with user-friendly messaging
      const userFriendlyError = enhanceError(enhancedError, {
        operation: path.split('/').pop(),
        method,
        path,
      });

      logger.error('API response JSON parse error', {
        component: 'ApiClient',
        operation: 'request',
        method,
        path,
        requestId,
        status: response.status,
        contentType: response.headers.get('content-type'),
        userFriendlyTitle: userFriendlyError.userFriendly.title,
      }, originalError);

      // Capture in dev error store
      if (import.meta.env.DEV) {
        captureException(userFriendlyError, {
          component: 'ApiClient',
          operation: `JSON parse: ${method} ${path}`,
          extra: {
            requestId,
            status: response.status,
            contentType: response.headers.get('content-type'),
          },
        });
      }

      // For successful responses with unparsable bodies, fall back to empty object
      // to avoid blocking the UI while still emitting diagnostics above.
      return {} as T;
    }
  }

  /**
   * Request that expects an array response.
   *
   * DEFENSIVE: Handles both direct arrays and PaginatedResponse wrappers.
   * Use this for ALL list endpoints returning T[] to prevent future bugs when
   * backend endpoints migrate to PaginatedResponse format.
   *
   * @param path - API endpoint path
   * @param options - Fetch options
   * @param skipRetry - Skip retry logic
   * @param cancelToken - Abort signal for cancellation
   * @returns Array of T extracted from response
   */
  async requestList<T>(
    path: string,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal
  ): Promise<T[]> {
    const response = await this.request<unknown>(path, options, skipRetry, cancelToken);
    return extractArrayFromResponse<T>(response);
  }

  /**
   * Stream inference using the /v1/infer/stream endpoint with SSE.
   *
   * POST /v1/infer/stream
   *
   * Uses fetch with ReadableStream since EventSource doesn't support POST.
   *
   * @param request - The streaming inference request payload (snake_case for backend)
   * @param callbacks - Event callbacks for streaming tokens
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Promise that resolves when stream completes
   */
  async streamInfer<TChunk = unknown, TMetadata = unknown>(
    request: unknown,
    callbacks: {
      onToken: (token: string, chunk: TChunk) => void;
      onComplete: (
        fullText: string,
        finishReason: string | null,
        metadata?: TMetadata
      ) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void> {
    const url = `${this.baseUrl}/v1/infer/stream`;

    // Read CSRF token from cookie
    const readCookie = (name: string): string | undefined => {
      if (typeof document === 'undefined') return undefined;
      const cookies = document.cookie?.split(';') ?? [];
      const prefix = `${name}=`;
      for (const raw of cookies) {
        const trimmed = raw.trim();
        if (trimmed.startsWith(prefix)) {
          return decodeURIComponent(trimmed.slice(prefix.length));
        }
      }
      return undefined;
    };
    const csrfToken = readCookie('csrf_token');

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      Accept: 'text/event-stream',
      ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
      ...(csrfToken ? { 'X-CSRF-Token': csrfToken } : {}),
    };

    let response: Response;
    try {
      response = await fetch(url, {
        method: 'POST',
        headers,
        body: JSON.stringify(request),
        credentials: 'omit',
        signal: cancelToken,
      });
    } catch (networkError) {
      const error = toError(networkError);
      if (error.name !== 'AbortError') {
        markBackendUnreachable(error, { method: 'POST', path: '/v1/infer/stream' });
      }
      callbacks.onError(error);
      throw error;
    }

    if (!response.ok) {
      const errorMessage = await response.text().catch(() => response.statusText);
      const error = new Error(`Stream request failed: ${response.status} ${errorMessage}`) as ApiError;
      error.status = response.status;
      callbacks.onError(error);
      throw error;
    }

    markBackendReachable();

    if (!response.body) {
      const error = new Error('Response body is null - streaming not supported');
      callbacks.onError(error);
      throw error;
    }

    // Read the SSE stream
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let fullText = '';
    let finishReason: string | null = null;
    let metadata: TMetadata | undefined;

    try {
      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          break;
        }

        buffer += decoder.decode(value, { stream: true });

        // Process complete SSE lines
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        for (const line of lines) {
          const trimmedLine = line.trim();

          // Skip empty lines and comments
          if (!trimmedLine || trimmedLine.startsWith(':')) {
            continue;
          }

          // Parse SSE data lines
          if (trimmedLine.startsWith('data:')) {
            const jsonStr = trimmedLine.slice(5).trim();

            // Handle [DONE] marker
            if (jsonStr === '[DONE]') {
              callbacks.onComplete(fullText, finishReason, metadata);
              return;
            }

            try {
              const event = JSON.parse(jsonStr);

              // Handle different event types based on 'event' discriminator
              if (typeof event === 'object' && event !== null && 'event' in event) {
                switch (event.event) {
                  case 'Token':
                    if (typeof event.text === 'string') {
                      fullText += event.text;
                      callbacks.onToken(event.text, event as TChunk);
                    }
                    break;

                  case 'Done':
                    finishReason = 'stop';
                    metadata = {
                      unavailable_pinned_adapters: event.unavailable_pinned_adapters,
                      pinned_routing_fallback: event.pinned_routing_fallback,
                      citations: event.citations,
                    } as TMetadata;
                    callbacks.onComplete(fullText, finishReason, metadata);
                    return;

                  case 'Error':
                    const errorMsg = event.message || 'Stream error';
                    const streamError = new Error(errorMsg);
                    callbacks.onError(streamError);
                    if (!event.recoverable) {
                      throw streamError;
                    }
                    break;

                  case 'Loading':
                  case 'Ready':
                    // These are progress events - can be extended to support onProgress callback
                    break;

                  default:
                    logger.debug('Unknown inference event type', {
                      component: 'ApiClient',
                      operation: 'streamInfer',
                      eventType: event.event,
                    });
                }
              } else if (typeof event === 'object' && event !== null && 'choices' in event) {
                // OpenAI-compatible format (StreamingChunk)
                const choices = event.choices as Array<{
                  delta?: { content?: string };
                  finish_reason?: string | null;
                }>;
                if (choices && choices[0]) {
                  const choice = choices[0];
                  if (choice.delta?.content) {
                    fullText += choice.delta.content;
                    callbacks.onToken(choice.delta.content, event as TChunk);
                  }
                  if (choice.finish_reason) {
                    finishReason = choice.finish_reason;
                  }
                }
              }
            } catch (parseError) {
              logger.warn('Failed to parse SSE event', {
                component: 'ApiClient',
                operation: 'streamInfer',
                data: jsonStr.slice(0, 100),
              });
            }
          }
        }
      }

      // Stream ended without explicit Done event
      callbacks.onComplete(fullText, finishReason || 'stop', metadata);
    } catch (error) {
      const err = toError(error);
      if (err.name !== 'AbortError') {
        callbacks.onError(err);
      }
      throw err;
    } finally {
      reader.releaseLock();
    }
  }
}

// Export class for service classes and direct usage
export { ApiClient };

// NOTE: For backward compatibility, apiClient singleton is available from:
//   import { apiClient } from '@/api/services'
// Do NOT re-export from here to avoid circular dependency.
