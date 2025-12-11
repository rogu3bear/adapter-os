// API Client for AdapterOS Control Plane
//! 
//! Provides centralized API communication with structured logging and error handling.
//! 
//! # Citations
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"

import * as types from '@/api/types';
import * as authTypes from '@/api/auth-types';
import * as trainingTypes from '@/api/training-types';
import * as apiTypes from '@/api/api-types';
import * as federationTypes from '@/api/federation-types';
import * as pluginTypes from '@/api/plugin-types';
import * as chatTypes from '@/api/chat-types';
import * as documentTypes from '@/api/document-types';
import * as policyTypes from '@/api/policyTypes';
import * as ownerTypes from '@/api/owner-types';
import * as systemStateTypes from '@/api/system-state-types';
import * as adapterTypes from '@/api/adapter-types';
import * as replayTypes from '@/api/replay-types';
import * as repoTypes from '@/api/repo-types';
import { logger, toError } from '@/utils/logger';
import { SystemMetrics } from '@/api/types';
import { enhanceError, isTransientError, isTimeoutError } from '@/utils/errorMessages';
import { handleBlobResponse, getFilenameFromResponse, extractArrayFromResponse } from '@/api/helpers';
import { retryWithBackoff, RetryConfig, RetryResult, createRetryWrapper } from '@/utils/retry';
import { LoginResponseSchema } from '@/schemas/common.schema';
import { captureException } from '@/stores/errorStore';
import { markSessionExpired } from '@/auth/session';
import { isCoremlPackageUiEnabled } from '@/config/featureFlags';

// Type-safe API error with extended properties
export interface ApiError extends Error {
  code?: string;
  status?: number;
  details?: Record<string, unknown>;
  detail?: string;
  requestId?: string;
}

const API_BASE_URL = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

function parseAuditMetadata(metadata?: string | null): Record<string, unknown> | undefined {
  if (!metadata) {
    return undefined;
  }

  try {
    return JSON.parse(metadata);
  } catch (error) {
    logger.debug('Failed to parse audit log metadata', {
      component: 'ApiClient',
      operation: 'queryAuditLogs',
      metadata,
      error: error instanceof Error ? error.message : String(error),
    });
    return undefined;
  }
}

class ApiClient {
  private baseUrl: string;
  private requestLog: Array<{ id: string; method: string; path: string; timestamp: string }> = [];
  private retryConfig: RetryConfig;
  private token?: string;
  private refreshPromise: Promise<void> | null = null;

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

  private async performRefresh(): Promise<void> {
    if (this.refreshPromise) {
      return this.refreshPromise;
    }

    const hadBearerToken = Boolean(this.token);

    this.refreshPromise = (async () => {
      const refreshUrl = `${this.baseUrl}/v1/auth/refresh`;
      const resp = await fetch(refreshUrl, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
        },
      });

      if (!resp.ok) {
        this.token = undefined;
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
        const body = await resp.json();
        if (body && typeof body === 'object' && 'token' in body && typeof (body as { token?: unknown }).token === 'string') {
          // Keep bearer token in memory synchronized with refreshed session
          this.token = (body as { token: string }).token;
        }
      } catch {
        // If parsing fails, still proceed; cookies were refreshed by the server
      }
    })();

    try {
      await this.refreshPromise;
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

  async request<T>(
    path: string,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal,
    allowMutationRetry: boolean = false
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
      return this.executeRequest(path, options, cancelToken);
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
    attemptedRefresh: boolean = false
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;

    // Compute deterministic request ID
    const method = options.method || 'GET';
    const body = options.body || '';
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

    const headers: HeadersInit = {
      'Content-Type': 'application/json',
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

    // Store in local audit buffer
    this.logRequest(requestId, method, path);

    let response: Response;
    try {
      response = await fetch(url, {
        ...options,
        headers,
        credentials: 'include', // Send httpOnly cookies
        signal: cancelToken, // Add cancellation support
      });
    } catch (networkError) {
      // Network error (connection failure, timeout, etc.)
      const error = toError(networkError);
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
        try {
          await this.performRefresh();
          return await this.executeRequest<T>(path, options, cancelToken, true);
        } catch (refreshError) {
          const err = toError(refreshError) as ApiError;
          err.code = err.code || 'SESSION_EXPIRED';
          err.status = err.status || 401;
          if (err.code === 'SESSION_EXPIRED') {
            markSessionExpired();
          }
          throw err;
        }
      }

      let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
      let errorCode: string | undefined;
      let errorDetails: Record<string, unknown> = {};
      let errorDetail: string | undefined;
      let correlatedRequestId: string | undefined = returnedId || requestId;

      try {
        const parsed = await response.json();
        if (parsed && typeof parsed === 'object') {
          const envelope = parsed as {
            code?: unknown;
            message?: unknown;
            detail?: unknown;
            request_id?: unknown;
            requestId?: unknown;
            error?: unknown;
            details?: unknown;
          };

          if (typeof envelope.message === 'string') {
            errorMessage = envelope.message;
          } else if (typeof envelope.error === 'string') {
            errorMessage = envelope.error;
          }

          if (typeof envelope.code === 'string') {
            errorCode = envelope.code;
          } else if (typeof envelope.error === 'string') {
            errorCode = envelope.error;
          }

          if (typeof envelope.detail === 'string') {
            errorDetail = envelope.detail;
          } else if (typeof envelope.details === 'string') {
            errorDetail = envelope.details;
          } else if (envelope.details && typeof envelope.details === 'object') {
            errorDetails = envelope.details as Record<string, unknown>;
          }

          if (typeof envelope.request_id === 'string') {
            correlatedRequestId = envelope.request_id;
          } else if (typeof envelope.requestId === 'string') {
            correlatedRequestId = envelope.requestId;
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

      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console -- intentional dev-mode logging
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
      return JSON.parse(rawBody) as T;
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

  // Authentication
  async login(credentials: authTypes.LoginRequest): Promise<authTypes.LoginResponse> {
    const response = await this.request<unknown>('/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify(credentials),
    });

    // Runtime validation of login response structure
    try {
      const validated = LoginResponseSchema.parse(response);
      validated.session_mode = validated.session_mode ?? 'normal';
      logger.info('User authentication successful', {
        component: 'ApiClient',
        operation: 'login',
        user_id: validated.user_id,
        tenant_id: validated.tenant_id,
        email: credentials.email,
      });
      this.token = validated.token;
      return validated as authTypes.LoginResponse;
    } catch (validationError) {
      const error = toError(validationError);
      logger.validationError('Login response validation failed', {
        component: 'ApiClient',
        operation: 'login',
        userJourney: 'login_flow',
        details: 'Server returned invalid login response structure',
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
        receivedResponse: typeof response === 'object' ? Object.keys(response as Record<string, unknown>) : String(response),
      }, ['Invalid response structure from authentication server'], error);

      // Create a more helpful error message
      const validationError_ = new Error('Login response has invalid structure') as ApiError;
      validationError_.code = 'RESPONSE_VALIDATION_ERROR';
      validationError_.details = {
        message: error.message,
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
      };
      throw validationError_;
    }
  }

  async logout(): Promise<void> {
    await this.request('/v1/auth/logout', { method: 'POST' });
    // Clear stored token (cookie is also cleared by server)
    this.token = undefined;
  }

  async devBypass(): Promise<authTypes.LoginResponse> {
    const response = await this.request<unknown>('/v1/auth/dev-bypass', { method: 'POST' });

    // Runtime validation of devBypass response structure
    try {
      const validated = LoginResponseSchema.parse(response);
      validated.session_mode = validated.session_mode ?? 'dev_bypass';
      logger.info('Dev bypass authentication successful', {
        component: 'ApiClient',
        operation: 'devBypass',
        user_id: validated.user_id,
        tenant_id: validated.tenant_id,
      });
      this.token = validated.token;
      return validated as authTypes.LoginResponse;
    } catch (validationError) {
      const error = toError(validationError);
      logger.error('Dev bypass response validation failed', {
        component: 'ApiClient',
        operation: 'devBypass',
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
        receivedResponse: typeof response === 'object' ? Object.keys(response as Record<string, unknown>) : String(response),
      }, error);

      // Create a more helpful error message
      const validationError_ = new Error('Dev bypass returned invalid response structure') as ApiError;
      validationError_.code = 'RESPONSE_VALIDATION_ERROR';
      validationError_.details = {
        message: error.message,
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
      };
      throw validationError_;
    }
  }

  async getCurrentUser(): Promise<authTypes.UserInfoResponse> {
    return this.request<authTypes.UserInfoResponse>('/v1/auth/me');
  }

  async refreshSession(): Promise<authTypes.UserInfoResponse> {
    logger.info('Refreshing auth session', {
      component: 'ApiClient',
      operation: 'refreshSession',
    });
    const resp = await this.request<authTypes.RefreshResponse>('/v1/auth/refresh', { method: 'POST' });
    this.token = resp.token;
    return this.getCurrentUser();
  }

  async logoutAllSessions(): Promise<void> {
    // Backend has no logout-all; fall back to logging out current session.
    logger.info('Logging out current session (logout-all fallback)', {
      component: 'ApiClient',
      operation: 'logoutAllSessions',
    });
    await this.request('/v1/auth/logout', { method: 'POST' });
    // Clear stored token
    this.token = undefined;
  }

  async listSessions(): Promise<types.SessionInfo[]> {
    return this.requestList<types.SessionInfo>('/v1/auth/sessions');
  }

  async listUserTenants(): Promise<authTypes.TenantSummary[]> {
    const resp = await this.request<authTypes.TenantListResponse>('/v1/auth/tenants');
    return resp.tenants ?? [];
  }

  async switchTenant(tenantId: string): Promise<authTypes.SwitchTenantResponse> {
    const resp = await this.request<authTypes.SwitchTenantResponse>('/v1/auth/tenants/switch', {
      method: 'POST',
      body: JSON.stringify({ tenant_id: tenantId }),
    });
    if (resp?.token) {
      this.token = resp.token;
    }
    return resp;
  }

  async revokeSession(sessionId: string): Promise<void> {
    await this.request<void>(`/v1/auth/sessions/${sessionId}`, {
      method: 'DELETE',
    });
  }

  async rotateApiToken(): Promise<authTypes.RotateTokenResponse> {
    logger.info('Rotating API token', {
      component: 'ApiClient',
      operation: 'rotateApiToken',
    });
    return this.request<authTypes.RotateTokenResponse>('/v1/auth/token/rotate', {
      method: 'POST',
    });
  }

  async getTokenMetadata(): Promise<types.TokenMetadata> {
    return this.request<types.TokenMetadata>('/v1/auth/token');
  }

  async updateUserProfile(data: types.UpdateProfileRequest): Promise<types.ProfileResponse> {
    return this.request<types.ProfileResponse>('/v1/auth/profile', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async getAuthConfig(cancelToken?: AbortSignal): Promise<types.AuthConfigResponse> {
    return this.request<types.AuthConfigResponse>('/v1/auth/config', {}, false, cancelToken);
  }

  async updateAuthConfig(data: types.UpdateAuthConfigRequest): Promise<types.AuthConfigResponse> {
    return this.request<types.AuthConfigResponse>('/v1/auth/config', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  // Health
  async health(): Promise<types.HealthResponse> {
    return this.request<types.HealthResponse>('/healthz');
  }

  async getHealthz(): Promise<types.HealthResponse> {
    return this.health();
  }

  async getHealthzAll(): Promise<apiTypes.SystemHealthResponse> {
    return this.request<apiTypes.SystemHealthResponse>('/healthz/all');
  }

  async getComponentHealth(component: string): Promise<types.ComponentHealth> {
    return this.request<types.ComponentHealth>(`/healthz/${component}`);
  }

  async ready(): Promise<types.HealthResponse> {
    return this.request<types.HealthResponse>('/readyz');
  }

  async getReadyz(): Promise<types.HealthResponse> {
    return this.ready();
  }

  async meta(): Promise<types.MetaResponse> {
    return this.request<types.MetaResponse>('/v1/meta');
  }

  async getMeta(): Promise<types.MetaResponse> {
    return this.meta();
  }

  // Tenants
  async listTenants(): Promise<types.Tenant[]> {
    return this.requestList<types.Tenant>('/v1/tenants');
  }

  async createTenant(data: types.CreateTenantRequest): Promise<types.Tenant> {
    return this.request<types.Tenant>('/v1/tenants', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  // Nodes
  async listNodes(): Promise<types.Node[]> {
    return this.requestList<types.Node>('/v1/nodes');
  }

  // Adapters
  async listAdapters(params?: { tier?: string; framework?: string }): Promise<types.Adapter[]> {
    const qs = new URLSearchParams();
    if (params?.tier !== undefined) qs.append('tier', params.tier);
    if (params?.framework) qs.append('framework', params.framework);
    const query = qs.toString() ? `?${qs.toString()}` : '';
    return this.requestList<types.Adapter>(`/v1/adapters${query}`);
  }

  async getCoremlPackageStatus(
    adapterId: string,
    modelId?: string
  ): Promise<adapterTypes.CoremlPackageStatus> {
    if (!isCoremlPackageUiEnabled()) {
      return { supported: false, export_available: false, verification_status: 'unsupported' };
    }
    if (!adapterId) {
      return { supported: false, export_available: false, verification_status: 'unknown' };
    }
    const qs = new URLSearchParams();
    if (modelId) {
      qs.append('model_id', modelId);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    try {
      const resp = await this.request<adapterTypes.CoremlPackageStatusResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/status${query}`
      );
      return resp.status ?? (resp as unknown as adapterTypes.CoremlPackageStatus);
    } catch (error) {
      const apiErr = error as ApiError;
      if (apiErr?.status === 404 || apiErr?.status === 501) {
        return {
          supported: false,
          export_available: false,
          verification_status: 'unsupported',
          notes: apiErr?.detail ? [apiErr.detail] : undefined,
        };
      }
      throw enhanceError(apiErr ?? (error as Error), {
        operation: 'coreml_status',
        adapterId,
        modelId,
      });
    }
  }

  async triggerCoremlExport(
    adapterId: string,
    modelId?: string
  ): Promise<adapterTypes.CoremlPackageActionResponse> {
    if (!isCoremlPackageUiEnabled()) {
      return {
        message: 'CoreML export is disabled in this build',
        status: { supported: false, export_available: false, verification_status: 'unsupported' },
      };
    }
    const qs = new URLSearchParams();
    if (modelId) {
      qs.append('model_id', modelId);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    try {
      return await this.request<adapterTypes.CoremlPackageActionResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/export${query}`,
        { method: 'POST' },
        false,
        undefined,
        true
      );
    } catch (error) {
      const apiErr = error as ApiError;
      const detail =
        apiErr?.detail ||
        apiErr?.message ||
        (apiErr?.status === 404 || apiErr?.status === 501
          ? 'CoreML export not supported by server'
          : 'CoreML export request failed');
      const err = enhanceError(apiErr ?? (error as Error), {
        operation: 'coreml_export',
        adapterId,
        modelId,
        detail,
      });
      throw err;
    }
  }

  async triggerCoremlVerification(
    adapterId: string
  ): Promise<adapterTypes.CoremlPackageActionResponse> {
    if (!isCoremlPackageUiEnabled()) {
      return {
        message: 'CoreML verification is disabled in this build',
        status: { supported: false, export_available: false, verification_status: 'unsupported' },
      };
    }
    try {
      return await this.request<adapterTypes.CoremlPackageActionResponse>(
        `/v1/adapters/${encodeURIComponent(adapterId)}/coreml/verify`,
        { method: 'POST' },
        false,
        undefined,
        true
      );
    } catch (error) {
      const apiErr = error as ApiError;
      const detail =
        apiErr?.detail ||
        apiErr?.message ||
        (apiErr?.status === 404 || apiErr?.status === 501
          ? 'CoreML verification not supported by server'
          : 'CoreML verification request failed');
      const err = enhanceError(apiErr ?? (error as Error), {
        operation: 'coreml_verification',
        adapterId,
        detail,
      });
      throw err;
    }
  }

  async preflightAdapterLoad(
    adapterId: string,
    operation: 'load' | 'unload' = 'load'
  ): Promise<policyTypes.PolicyPreflightResponse> {
    return this.request<policyTypes.PolicyPreflightResponse>(
      `/v1/adapters/${encodeURIComponent(adapterId)}/load/preflight`,
      {
        method: 'POST',
        body: JSON.stringify({ operation, includeDetails: true }),
      }
    );
  }

  async loadAdapter(adapterId: string): Promise<types.Adapter> {
    return this.request<types.Adapter>(`/v1/adapters/${adapterId}/load`, {
      method: 'POST',
    });
  }

  async unloadAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}/unload`, {
      method: 'POST',
    });
  }

  async getJourney(journeyType: string, journeyId: string): Promise<types.JourneyResponse> {
    return this.request<types.JourneyResponse>(`/v1/journeys/${journeyType}/${journeyId}`);
  }

  async registerNode(data: types.RegisterNodeRequest): Promise<types.Node> {
    return this.request<types.Node>('/v1/nodes/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async testNodeConnection(nodeId: string): Promise<types.NodePingResponse> {
    return this.request<types.NodePingResponse>(`/v1/nodes/${nodeId}/ping`, {
      method: 'POST',
    });
  }

  async markNodeOffline(nodeId: string): Promise<void> {
    return this.request<void>(`/v1/nodes/${nodeId}/offline`, {
      method: 'POST',
    });
  }

  async evictNode(nodeId: string): Promise<void> {
    return this.request<void>(`/v1/nodes/${nodeId}`, {
      method: 'DELETE',
    });
  }

  async getNodeDetails(nodeId: string): Promise<types.NodeDetailsResponse> {
    return this.request<types.NodeDetailsResponse>(`/v1/nodes/${nodeId}/details`);
  }

  // Workers
  async listWorkers(tenantId?: string, nodeId?: string): Promise<types.WorkerResponse[]> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    if (nodeId) params.append('node_id', nodeId);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.requestList<types.WorkerResponse>(`/v1/workers${query}`);
  }

  async spawnWorker(request: types.SpawnWorkerRequest): Promise<types.WorkerResponse> {
    return this.request<types.WorkerResponse>('/v1/workers/spawn', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async stopWorker(workerId: string, force: boolean = false): Promise<void> {
    return this.request<void>(`/v1/workers/${workerId}/stop`, {
      method: 'POST',
      body: JSON.stringify({ force }),
    });
  }

  async getWorkerDetails(workerId: string): Promise<types.WorkerDetailsResponse> {
    return this.request<types.WorkerDetailsResponse>(`/v1/workers/${workerId}/details`);
  }

  // Plans
  async listPlans(): Promise<types.Plan[]> {
    return this.requestList<types.Plan>('/v1/plans');
  }

  async buildPlan(data: types.BuildPlanRequest): Promise<types.Plan> {
    return this.request<types.Plan>('/v1/plans/build', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async rebuildPlan(planId: string): Promise<types.Plan> {
    return this.request<types.Plan>(`/v1/plans/${planId}/rebuild`, {
      method: 'POST',
    });
  }

  async comparePlans(planId1: string, planId2: string): Promise<types.PlanComparisonResponse> {
    return this.request<types.PlanComparisonResponse>('/v1/plans/compare', {
      method: 'POST',
      body: JSON.stringify({ plan_id_1: planId1, plan_id_2: planId2 }),
    });
  }

  async deletePlan(planId: string): Promise<void> {
    return this.request<void>(`/v1/plans/${planId}`, {
      method: 'DELETE',
    });
  }

  async exportPlanManifest(planId: string): Promise<Blob> {
    const path = `/v1/plans/${planId}/manifest`;
    const url = `${this.baseUrl}${path}`;
    const response = await fetch(url, { credentials: 'include' });
    return handleBlobResponse(response, { method: 'GET', path });
  }

  // Control Plane
  async promote(data: types.PromotionRequest): Promise<types.PromotionRecord> {
    return this.request<types.PromotionRecord>('/v1/cp/promote', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getPromotionGates(cpid: string): Promise<types.PromotionGate[]> {
    return this.requestList<types.PromotionGate>(`/v1/cp/promotion-gates/${cpid}`);
  }

  async rollback(): Promise<void> {
    return this.request('/v1/cp/rollback', { method: 'POST' });
  }

  async getPromotion(id: string): Promise<types.PromotionRecord> {
    return this.request<types.PromotionRecord>(`/v1/promotions/${id}`);
  }

  // Policies
  async listPolicies(): Promise<types.Policy[]> {
    return this.requestList<types.Policy>('/v1/policies');
  }

  async getPolicy(cpid: string): Promise<types.Policy> {
    return this.request<types.Policy>(`/v1/policies/${cpid}`);
  }

  async validatePolicy(data: types.ValidatePolicyRequest): Promise<{ valid: boolean; errors?: string[] }> {
    return this.request('/v1/policies/validate', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async applyPolicy(data: types.ApplyPolicyRequest): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async createPolicy(cpid: string, content: string): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  async updatePolicy(cpid: string, content: string): Promise<types.PolicyPackResponse> {
    return this.request<types.PolicyPackResponse>('/v1/policies/apply', {
      method: 'POST',
      body: JSON.stringify({ cpid, content }),
    });
  }

  // Telemetry
  async listTelemetryBundles(): Promise<types.TelemetryBundle[]> {
    return this.requestList<types.TelemetryBundle>('/v1/telemetry/bundles');
  }

  async getTelemetryLogs(filters?: { category?: string; limit?: number; offset?: number }): Promise<types.TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.category) params.append('category', filters.category);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.requestList<types.TelemetryEvent>(`/v1/telemetry/logs${query}`);
  }

  async listContacts(tenantId: string): Promise<types.Contact[]> {
    const params = new URLSearchParams({ tenant_id: tenantId });
    return this.requestList<types.Contact>(`/v1/contacts?${params.toString()}`);
  }

  // Golden baselines
  async listGoldenRuns(): Promise<string[]> {
    return this.requestList<string>('/v1/golden/runs');
  }

  async getGoldenRun(name: string): Promise<types.GoldenRunSummary> {
    return this.request<types.GoldenRunSummary>(`/v1/golden/runs/${encodeURIComponent(name)}`);
    }

  async compareGoldenRuns(runA: string, runB: string): Promise<types.GoldenCompareResult> {
    return this.request<types.GoldenCompareResult>('/v1/golden/compare', {
      method: 'POST',
      body: JSON.stringify({ run_a: runA, run_b: runB }),
    });
  }

  async goldenCompare(req: types.GoldenCompareRequest): Promise<types.VerificationReport> {
    return this.request<types.VerificationReport>('/v1/golden/compare', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  }

  // Golden run promotion workflow
  async requestGoldenPromotion(runId: string, targetStage: string): Promise<types.PromotionResponse> {
    return this.request<types.PromotionResponse>(`/v1/golden/${encodeURIComponent(runId)}/promote`, {
      method: 'POST',
      body: JSON.stringify({ target_stage: targetStage }),
    });
  }

  async getGoldenPromotionStatus(runId: string): Promise<types.PromotionStatusResponse> {
    return this.request<types.PromotionStatusResponse>(`/v1/golden/${encodeURIComponent(runId)}/promotion`);
  }

  async approveGoldenPromotion(runId: string, stageId: string, notes: string): Promise<types.ApproveResponse> {
    return this.request<types.ApproveResponse>(`/v1/golden/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
      body: JSON.stringify({ stage_id: stageId, approved: true, notes }),
    });
  }

  async rejectGoldenPromotion(runId: string, stageId: string, notes: string): Promise<types.ApproveResponse> {
    return this.request<types.ApproveResponse>(`/v1/golden/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
      body: JSON.stringify({ stage_id: stageId, approved: false, notes }),
    });
  }

  async getGoldenGateStatus(runId: string): Promise<types.GateStatus[]> {
    return this.requestList<types.GateStatus>(`/v1/golden/${encodeURIComponent(runId)}/gates`);
  }

  async rollbackGoldenPromotion(stage: string): Promise<types.RollbackResponse> {
    return this.request<types.RollbackResponse>(`/v1/golden/${encodeURIComponent(stage)}/rollback`, {
      method: 'POST',
    });
  }

  // (removed duplicate listAdapters without parameters)

  async getAdapter(adapterId: string): Promise<types.Adapter> {
    return this.request<types.Adapter>(`/v1/adapters/${adapterId}`);
  }

  async getAdapterDetail(adapterId: string): Promise<types.AdapterDetailResponse> {
    return this.request<types.AdapterDetailResponse>(`/v1/adapters/${adapterId}/detail`);
  }

  async updateAdapterStrength(adapterId: string, loraStrength: number): Promise<types.AdapterDetailResponse> {
    return this.request<types.AdapterDetailResponse>(`/v1/adapters/${adapterId}/strength`, {
      method: 'PATCH',
      body: JSON.stringify({ lora_strength: loraStrength }),
    });
  }

  async getAdapterLineage(adapterId: string): Promise<types.AdapterLineageResponse> {
    return this.request<types.AdapterLineageResponse>(`/v1/adapters/${adapterId}/lineage`);
  }

  async getAdapterVersionLineage(
    adapterVersionId: string,
    params?: types.LineageQueryParams
  ): Promise<types.LineageGraphResponse> {
    const queryParams = new URLSearchParams();
    if (params?.direction) queryParams.set('direction', params.direction);
    if (params?.include_evidence !== undefined) {
      queryParams.set('include_evidence', String(params.include_evidence));
    }
    if (params?.limit_per_level !== undefined) {
      queryParams.set('limit_per_level', String(params.limit_per_level));
    }
    if (params?.cursors) {
      Object.entries(params.cursors).forEach(([level, cursor]) => {
        queryParams.append(`cursor[${level}]`, cursor);
      });
    }
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<types.LineageGraphResponse>(`/v1/lineage/adapter_versions/${adapterVersionId}${query}`);
  }

  async promoteAdapterLifecycle(adapterId: string, reason: string): Promise<types.LifecycleTransitionResponse> {
    return this.request<types.LifecycleTransitionResponse>(`/v1/adapters/${adapterId}/lifecycle/promote`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  async demoteAdapterLifecycle(adapterId: string, reason: string): Promise<types.LifecycleTransitionResponse> {
    return this.request<types.LifecycleTransitionResponse>(`/v1/adapters/${adapterId}/lifecycle/demote`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  async registerAdapter(data: types.RegisterAdapterRequest): Promise<types.Adapter> {
    return this.request<types.Adapter>('/v1/adapters/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async importAdapter(file: File, load?: boolean, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.Adapter> {
    const formData = new FormData();
    formData.append('file', file);

    const params = new URLSearchParams();
    if (load) params.append('load', 'true');

    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<types.Adapter>(`/v1/adapters/import${query}`, {
      method: 'POST',
      body: formData,
      headers: {}, // Let browser set Content-Type for FormData
      ...options,
    }, skipRetry, cancelToken);
  }

  async deleteAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}`, {
      method: 'DELETE',
    });
  }

  async upsertAdapterDirectory(data: {
    tenant_id: string;
    root: string;
    path: string;
    activate: boolean;
  }): Promise<{ adapter_id: string }> {
    return this.request<{ adapter_id: string }>('/v1/adapters/directory/upsert', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  // (duplicate methods removed; see definitions above returning types.Adapter)

  // Training endpoints
  async listTrainingJobs(params?: { dataset_id?: string; status?: string; adapter_name?: string; template_id?: string; page?: number; page_size?: number }): Promise<trainingTypes.ListTrainingJobsResponse> {
    const queryParams = new URLSearchParams();
    if (params?.dataset_id) queryParams.append('dataset_id', params.dataset_id);
    if (params?.status) queryParams.append('status', params.status);
    if (params?.adapter_name) queryParams.append('adapter_name', params.adapter_name);
    if (params?.template_id) queryParams.append('template_id', params.template_id);
    if (params?.page) queryParams.append('page', params.page.toString());
    if (params?.page_size) queryParams.append('page_size', params.page_size.toString());

    const queryString = queryParams.toString();
    const url = queryString ? `/v1/training/jobs?${queryString}` : '/v1/training/jobs';
    return this.request<trainingTypes.ListTrainingJobsResponse>(url);
  }

  async getTrainingJob(jobId: string): Promise<trainingTypes.TrainingJob> {
    return this.request<trainingTypes.TrainingJob>(`/v1/training/jobs/${jobId}`);
  }

  async getTrainingArtifacts(jobId: string): Promise<types.TrainingArtifactsResponse> {
    return this.request<types.TrainingArtifactsResponse>(`/v1/training/jobs/${jobId}/artifacts`);
  }

  async startTraining(request: types.StartTrainingRequest): Promise<types.TrainingJob> {
    return this.request<types.TrainingJob>('/v1/training/start', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async cancelTraining(jobId: string): Promise<void> {
    return this.request<void>(`/v1/training/jobs/${jobId}/cancel`, {
      method: 'POST',
    });
  }

  async getTrainingLogs(jobId: string): Promise<string[]> {
    return this.requestList<string>(`/v1/training/jobs/${jobId}/logs`);
  }

  async getTrainingMetrics(jobId: string): Promise<types.TrainingMetrics> {
    return this.request<types.TrainingMetrics>(`/v1/training/jobs/${jobId}/metrics`);
  }

  /**
   * Download a training artifact file.
   * Returns the download URL or triggers a blob download for the artifact.
   */
  async downloadArtifact(jobId: string, artifactId: string, filename?: string): Promise<void> {
    const path = `/v1/training/jobs/${jobId}/artifacts/${artifactId}/download`;
    const url = this.buildUrl(path);

    try {
      const response = await fetch(url, {
        method: 'GET',
        credentials: 'include',
      });

      // Use helper for blob response with error handling
      const blob = await handleBlobResponse(response, { method: 'GET', path });

      // Get filename from Content-Disposition header or use provided filename
      const downloadFilename = getFilenameFromResponse(response, filename || artifactId);
      const blobUrl = window.URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = blobUrl;
      link.download = downloadFilename;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      window.URL.revokeObjectURL(blobUrl);

      logger.info('Artifact downloaded', {
        component: 'ApiClient',
        operation: 'downloadArtifact',
        jobId,
        artifactId,
        filename: downloadFilename,
      });
    } catch (error) {
      logger.error('Failed to download artifact', {
        component: 'ApiClient',
        operation: 'downloadArtifact',
        jobId,
        artifactId,
      }, toError(error));
      throw error;
    }
  }

  async listTrainingTemplates(): Promise<types.TrainingTemplate[]> {
    return this.requestList<types.TrainingTemplate>('/v1/training/templates');
  }

  async getTrainingTemplate(templateId: string): Promise<types.TrainingTemplate> {
    return this.request<types.TrainingTemplate>(`/v1/training/templates/${templateId}`);
  }

  /**
   * Get chat bootstrap data for a training job
   * Returns the "recipe" for starting a chat from a completed training job
   */
  async getChatBootstrap(jobId: string): Promise<trainingTypes.ChatBootstrapResponse> {
    return this.request<trainingTypes.ChatBootstrapResponse>(`/v1/training/jobs/${jobId}/chat_bootstrap`);
  }

  /**
   * Create a chat session from a training job
   * Creates a chat session bound to the training job's stack in one call
   */
  async createChatFromTrainingJob(request: trainingTypes.CreateChatFromJobRequest): Promise<trainingTypes.CreateChatFromJobResponse> {
    return this.request<trainingTypes.CreateChatFromJobResponse>('/v1/chats/from_training_job', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // Dataset endpoints
  async createDataset(request: trainingTypes.CreateDatasetRequest): Promise<trainingTypes.DatasetResponse> {
    // Use FormData for file uploads
    const formData = new FormData();
    formData.append('name', request.name);
    formData.append('source_type', request.source_type);
    formData.append('format', request.format ?? 'jsonl');
    if (request.description) formData.append('description', request.description);
    if (request.language) formData.append('language', request.language);
    if (request.framework) formData.append('framework', request.framework);
    if (request.repository_url) formData.append('repository_url', request.repository_url);
    if (request.branch) formData.append('branch', request.branch);
    if (request.commit_hash) formData.append('commit_hash', request.commit_hash);
    if (request.files) {
      request.files.forEach((file) => {
        formData.append('files', file);
      });
    }

    const url = `${this.baseUrl}/v1/datasets/upload`;
    const requestId = await this.computeRequestId('POST', '/v1/datasets/upload', request.name);
    this.logRequest(requestId, 'POST', '/v1/datasets/upload');

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'X-Request-ID': requestId,
        ...(this.token ? { 'Authorization': `Bearer ${this.token}` } : {}),
      },
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
      try {
        const error = await response.json();
        errorMessage = error.error || errorMessage;
      } catch {
        // Use status text
      }
      throw new Error(errorMessage);
    }

    type UploadDatasetResponse = {
      schema_version?: string;
      dataset_id: string;
      name: string;
      description?: string;
      file_count?: number;
      total_size_bytes?: number;
      format?: string;
      hash?: string;
      storage_path?: string;
      created_at?: string;
    };

    const raw = (await response.json()) as UploadDatasetResponse;
    const createdAt = raw.created_at ?? new Date().toISOString();

    const dataset: trainingTypes.Dataset = {
      id: raw.dataset_id,
      name: raw.name,
      hash_b3: raw.hash ?? '',
      source_type: request.source_type,
      language: request.language,
      framework: request.framework,
      file_count: raw.file_count ?? request.files?.length ?? 0,
      total_size_bytes: raw.total_size_bytes ?? 0,
      total_tokens: 0,
      validation_status: 'draft',
      created_at: createdAt,
      updated_at: createdAt,
      format: raw.format ?? request.format ?? 'jsonl',
      storage_path: raw.storage_path,
      description: raw.description,
    };

    return {
      schema_version: raw.schema_version ?? '1.0',
      dataset,
    };
  }

  async listDatasets(params?: { page?: number; page_size?: number }): Promise<trainingTypes.ListDatasetsResponse> {
    const queryParams = new URLSearchParams();
    if (params?.page) queryParams.append('page', String(params.page));
    if (params?.page_size) queryParams.append('page_size', String(params.page_size));
    const query = queryParams.toString();
    
    // Backend returns array directly, but frontend expects wrapped response
    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    type BackendDataset = {
      dataset_id: string;
      dataset_version_id?: string;
      name: string;
      hash: string;
      total_size_bytes: number;
      file_count: number;
      format: string;
      storage_path: string;
      validation_status: string;
      validation_errors?: string;
      created_by: string;
      created_at: string;
      updated_at: string;
      description?: string;
      trust_state?: string;
      trust_reason?: string;
      overall_safety_status?: string;
      pii_status?: string;
      toxicity_status?: string;
      leak_status?: string;
      anomaly_status?: string;
    };
    const rawResponse = await this.request<unknown>(`/v1/datasets${query ? `?${query}` : ''}`);
    const response = extractArrayFromResponse<BackendDataset>(rawResponse);

    // Map backend responses to frontend Dataset type
    const datasets: trainingTypes.Dataset[] = response.map((d) => ({
      id: d.dataset_id,
      dataset_version_id: d.dataset_version_id,
      name: d.name,
      hash_b3: d.hash,
      source_type: 'uploaded_files' as trainingTypes.DatasetSourceType, // Default, parse from metadata_json if needed
      file_count: d.file_count,
      total_size_bytes: d.total_size_bytes,
      total_tokens: 0, // Will be fetched separately if needed
      validation_status: d.validation_status as trainingTypes.DatasetValidationStatus,
      created_at: d.created_at,
      updated_at: d.updated_at,
      format: d.format,
      storage_path: d.storage_path,
      validation_errors: d.validation_errors,
      created_by: d.created_by,
      description: d.description,
      trust_state: (d.trust_state as trainingTypes.TrustState) ?? 'unknown',
      trust_reason: d.trust_reason,
      overall_safety_status: d.overall_safety_status,
      pii_status: d.pii_status,
      toxicity_status: d.toxicity_status,
      leak_status: d.leak_status,
      anomaly_status: d.anomaly_status,
    }));
    
    return {
      schema_version: '1.0',
      datasets,
      total: datasets.length,
      page: params?.page || 1,
      page_size: params?.page_size || datasets.length,
    };
  }

  async getDataset(datasetId: string): Promise<trainingTypes.Dataset> {
    const response = await this.request<{
      dataset_id: string;
      dataset_version_id?: string;
      name: string;
      hash: string;
      total_size_bytes: number;
      file_count: number;
      format: string;
      storage_path: string;
      validation_status: string;
      validation_errors?: string;
      created_by: string;
      created_at: string;
      updated_at: string;
      description?: string;
      trust_state?: string;
      trust_reason?: string;
      overall_safety_status?: string;
      pii_status?: string;
      toxicity_status?: string;
      leak_status?: string;
      anomaly_status?: string;
    }>(`/v1/datasets/${datasetId}`);
    
    // Try to get statistics for total_tokens
    let totalTokens = 0;
    try {
      const stats = await this.request<{ total_tokens: number }>(`/v1/datasets/${datasetId}/statistics`).catch(() => null);
      if (stats) {
        totalTokens = stats.total_tokens;
      }
    } catch {
      // Statistics not available, use 0
    }
    
    // Parse metadata_json for source_type if available
    let sourceType: trainingTypes.DatasetSourceType = 'uploaded_files';
    try {
      // Try to infer from format or other fields
      // For now, default to uploaded_files
    } catch {
      // Use default
    }
    
    // Map backend response to frontend Dataset type
    return {
      id: response.dataset_id,
      dataset_version_id: response.dataset_version_id,
      name: response.name,
      hash_b3: response.hash,
      source_type: sourceType,
      file_count: response.file_count,
      total_size_bytes: response.total_size_bytes,
      total_tokens: totalTokens,
      validation_status: response.validation_status as trainingTypes.DatasetValidationStatus,
      created_at: response.created_at,
      updated_at: response.updated_at,
      format: response.format,
      storage_path: response.storage_path,
      validation_errors: response.validation_errors,
      created_by: response.created_by,
      description: response.description,
      trust_state: (response.trust_state as trainingTypes.TrustState) ?? 'unknown',
      trust_reason: response.trust_reason,
      overall_safety_status: response.overall_safety_status,
      pii_status: response.pii_status,
      toxicity_status: response.toxicity_status,
      leak_status: response.leak_status,
      anomaly_status: response.anomaly_status,
    };
  }

  async listDatasetVersions(datasetId: string): Promise<trainingTypes.DatasetVersionListResponse> {
    const response = await this.request<trainingTypes.DatasetVersionListResponse>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/versions`,
    );

    return {
      ...response,
      versions: (response.versions || []).map((v) => ({
        ...v,
        trust_state: (v.trust_state as trainingTypes.TrustState) ?? 'unknown',
      })),
    };
  }

  /**
   * Apply or update a dataset trust override for the latest version.
   *
   * POST /v1/datasets/:datasetId/trust_override
   */
  async applyDatasetTrustOverride(
    datasetId: string,
    payload: trainingTypes.DatasetTrustOverrideRequest
  ): Promise<trainingTypes.DatasetTrustOverrideResponse> {
    return this.request<trainingTypes.DatasetTrustOverrideResponse>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/trust_override`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  async getDatasetVersionLineage(
    datasetVersionId: string,
    params?: types.LineageQueryParams
  ): Promise<types.LineageGraphResponse> {
    const queryParams = new URLSearchParams();
    if (params?.direction) queryParams.set('direction', params.direction);
    if (params?.include_evidence !== undefined) {
      queryParams.set('include_evidence', String(params.include_evidence));
    }
    if (params?.limit_per_level !== undefined) {
      queryParams.set('limit_per_level', String(params.limit_per_level));
    }
    if (params?.cursors) {
      Object.entries(params.cursors).forEach(([level, cursor]) => {
        queryParams.append(`cursor[${level}]`, cursor);
      });
    }
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<types.LineageGraphResponse>(`/v1/lineage/dataset_versions/${datasetVersionId}${query}`);
  }

  async validateDataset(datasetId: string): Promise<trainingTypes.DatasetValidationResult> {
    return this.request<trainingTypes.DatasetValidationResult>(`/v1/datasets/${datasetId}/validate`, {
      method: 'POST',
    });
  }

  async deleteDataset(datasetId: string): Promise<void> {
    return this.request<void>(`/v1/datasets/${datasetId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Create a training dataset from existing documents or a document collection.
   * Converts RAG documents into JSONL training format.
   * Either documentId or collectionId must be provided (mutually exclusive).
   */
  async createDatasetFromDocuments(params: {
    document_ids?: string[];
    documentId?: string;
    collection_id?: string;
    collectionId?: string;
    name?: string;
    description?: string;
  }): Promise<trainingTypes.CreateDatasetFromDocumentsResponse> {
    return this.request<trainingTypes.CreateDatasetFromDocumentsResponse>('/v1/datasets/from-documents', {
      method: 'POST',
      body: JSON.stringify({
        document_ids: params.document_ids,
        document_id: params.documentId,
        collection_id: params.collection_id ?? params.collectionId,
        name: params.name,
        description: params.description,
      }),
    });
  }

  // Adapter lifecycle management
  // Supports both boolean and advanced pinning modes
  async pinAdapter(adapterId: string, pinnedOrTtlHours: boolean | number, reason?: string): Promise<void> {
    // If boolean, use simple pin/unpin API
    if (typeof pinnedOrTtlHours === 'boolean') {
      if (pinnedOrTtlHours) {
        return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
          method: 'POST',
          body: JSON.stringify({}),
        });
      } else {
        return this.unpinAdapter(adapterId);
      }
    }
    // Otherwise use advanced API with TTL
    return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'POST',
      body: JSON.stringify({ ttl_hours: pinnedOrTtlHours, reason }),
    });
  }

  async unpinAdapter(adapterId: string): Promise<void> {
    return this.request<void>(`/v1/adapters/${adapterId}/pin`, {
      method: 'DELETE',
    });
  }

  async swapAdapters(add: string[], remove: string[], commit: boolean = false): Promise<void> {
    return this.request<void>('/v1/adapters/swap', {
      method: 'POST',
      body: JSON.stringify({ add, remove, commit }),
    });
  }

  async getAdapterStats(adapterId: string): Promise<types.AdapterStats> {
    return this.request<types.AdapterStats>(`/v1/adapters/${adapterId}/stats`);
  }

  async getAdapterUsage(adapterId: string): Promise<types.AdapterUsageResponse> {
    return this.request<types.AdapterUsageResponse>(`/v1/adapters/${adapterId}/usage`);
  }

  async getAdapterActivations(adapterId: string): Promise<types.AdapterActivation[]> {
    return this.requestList<types.AdapterActivation>(`/v1/adapters/${adapterId}/activations`);
  }

  async promoteAdapterState(adapterId: string, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.AdapterStateResponse> {
    return this.request<types.AdapterStateResponse>(`/v1/adapters/${adapterId}/state/promote`, {
      method: 'POST',
      ...options,
    }, skipRetry, cancelToken);
  }

  async updateAdapterPolicy(adapterId: string, req: types.UpdateAdapterPolicyRequest): Promise<types.UpdateAdapterPolicyResponse> {
    return this.request<types.UpdateAdapterPolicyResponse>(`/v1/adapters/${adapterId}/policy`, {
      method: 'PUT',
      body: JSON.stringify(req),
    });
  }

  async downloadAdapterManifest(adapterId: string): Promise<types.AdapterManifest> {
    return this.request<types.AdapterManifest>(`/v1/adapters/${adapterId}/manifest`);
  }

  async getAdapterHealth(adapterId: string): Promise<types.AdapterHealthResponse> {
    return this.request<types.AdapterHealthResponse>(`/v1/adapters/${adapterId}/health`);
  }

  async validateAdapterName(request: { name: string }): Promise<types.ValidateAdapterNameResponse> {
    return this.request<types.ValidateAdapterNameResponse>('/v1/adapters/validate-name', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // Category Policies
  async getCategoryPolicies(): Promise<Record<types.AdapterCategory, types.CategoryPolicy>> {
    return this.request<Record<types.AdapterCategory, types.CategoryPolicy>>('/v1/adapters/category-policies');
  }

  async getCategoryPolicy(category: types.AdapterCategory): Promise<types.CategoryPolicy> {
    return this.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`);
  }

  async updateCategoryPolicy(category: types.AdapterCategory, policy: types.CategoryPolicy): Promise<types.CategoryPolicy> {
    return this.request<types.CategoryPolicy>(`/v1/adapters/category-policies/${category}`, {
      method: 'PUT',
      body: JSON.stringify(policy),
    });
  }

  // Repositories
  async listRepositories(): Promise<types.Repository[]> {
    return this.requestList<types.Repository>('/v1/repositories');
  }

  async registerRepository(data: types.RegisterRepositoryRequest): Promise<types.Repository> {
    return this.request<types.Repository>('/v1/code/register-repo', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async triggerRepositoryScan(repositoryId: string): Promise<types.TriggerScanResponse> {
    return this.request<types.TriggerScanResponse>('/v1/code/scan', {
      method: 'POST',
      body: JSON.stringify({ repository_id: repositoryId }),
    });
  }

  async getRepositoryStatus(repoId: string): Promise<types.ScanStatusResponse> {
    return this.request<types.ScanStatusResponse>(
      `/v1/repositories/${repoId}/status`
    );
  }

  // Commits
  async listCommits(repoId?: string): Promise<types.Commit[]> {
    const query = repoId ? `?repo_id=${repoId}` : '';
    return this.requestList<types.Commit>(`/v1/commits${query}`);
  }

  async getCommit(sha: string): Promise<types.Commit> {
    return this.request<types.Commit>(`/v1/commits/${sha}`);
  }

  async getCommitDiff(sha: string): Promise<types.CommitDiff> {
    return this.request<types.CommitDiff>(`/v1/commits/${sha}/diff`);
  }

  // Metrics
  async getSystemMetrics(): Promise<types.SystemMetrics> {
    return this.request<types.SystemMetrics>('/v1/metrics/system');
  }

  async getTenantStorageUsage(): Promise<apiTypes.TenantStorageUsageResponse> {
    return this.request<apiTypes.TenantStorageUsageResponse>('/v1/storage/tenant-usage');
  }

  async getQualityMetrics(): Promise<types.QualityMetrics> {
    return this.request<types.QualityMetrics>('/v1/metrics/quality');
  }

  async getAdapterMetrics(): Promise<types.AdapterMetrics[]> {
    return this.requestList<types.AdapterMetrics>('/v1/metrics/adapters');
  }

  async getSystemOverview(): Promise<ownerTypes.SystemOverview> {
    return this.request('/v1/system/overview');
  }

  /**
   * Get ground truth system state
   *
   * Returns hierarchical view: Node -> Tenant -> Stack -> Adapter
   * Includes memory pressure and top adapters by usage
   */
  async getSystemState(
    params?: systemStateTypes.SystemStateQuery
  ): Promise<systemStateTypes.SystemStateResponse> {
    const queryParams = new URLSearchParams();
    if (params?.include_adapters !== undefined) {
      queryParams.set('include_adapters', String(params.include_adapters));
    }
    if (params?.top_adapters !== undefined) {
      queryParams.set('top_adapters', String(params.top_adapters));
    }
    if (params?.tenant_id) {
      queryParams.set('tenant_id', params.tenant_id);
    }
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<systemStateTypes.SystemStateResponse>(`/v1/system/state${query}`);
  }

  // Base Model Status
  async getBaseModelStatus(tenantId?: string): Promise<types.BaseModelStatus> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.BaseModelStatus>(`/v1/models/status${query}`);
  }

  // Get all loaded models status
  async getAllModelsStatus(tenantId?: string): Promise<types.AllModelsStatusResponse> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.request<types.AllModelsStatusResponse>(`/v1/models/status/all${query}`);
  }

  // List models with stats for ModelSelector
  // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
  async listModels(): Promise<apiTypes.ModelWithStatsResponse[]> {
    const resp = await this.request<unknown>(`/v1/models`);
    return extractArrayFromResponse<apiTypes.ModelWithStatsResponse>(resp);
  }

  /**
   * Helper: list models with optional runtime status data.
   * Falls back gracefully if status endpoint is not accessible to the user.
   */
  async listModelsWithStatus(
    tenantId?: string
  ): Promise<
    Array<apiTypes.ModelWithStatsResponse & { status?: types.BaseModelStatus }>
  > {
    const [models, statusResp] = await Promise.all([
      this.listModels(),
      this.getAllModelsStatus(tenantId).catch(() => null),
    ]);

    const statusModels: types.BaseModelStatus[] = statusResp?.models ?? [];

    const statusById = statusModels.reduce<Record<string, types.BaseModelStatus>>(
      (acc, s) => {
        acc[s.model_id] = s;
        return acc;
      },
      {},
    );

    return models.map((model) => ({
      ...model,
      status: statusById[model.id],
    }));
  }

  // Base Model Management API Methods - Citation: IMPLEMENTATION_PLAN.md Phase 2
  async importModel(data: types.ImportModelRequest, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.ImportModelResponse> {
    return this.request<types.ImportModelResponse>('/v1/models/import', {
      method: 'POST',
      body: JSON.stringify(data),
      ...options,
    }, skipRetry, cancelToken);
  }

  async loadBaseModel(modelId: string): Promise<types.ModelStatusResponse> {
    return this.request<types.ModelStatusResponse>(`/v1/models/${modelId}/load`, {
      method: 'POST',
    });
  }

  async unloadBaseModel(modelId: string): Promise<void> {
    return this.request<void>(`/v1/models/${modelId}/unload`, {
      method: 'POST',
    });
  }

  async getModelStatus(modelId: string): Promise<types.ModelStatusResponse> {
    return this.request<types.ModelStatusResponse>(`/v1/models/${encodeURIComponent(modelId)}/status`);
  }

  async getModelImportStatus(importId: string): Promise<types.ImportModelResponse> {
    return this.request<types.ImportModelResponse>(`/v1/models/imports/${importId}`);
  }

  async getCursorConfig(): Promise<types.CursorConfigResponse> {
    return this.request<types.CursorConfigResponse>('/v1/models/cursor-config');
  }

  async validateModel(modelId: string): Promise<types.ModelValidationResponse> {
    return this.request<types.ModelValidationResponse>(`/v1/models/${modelId}/validate`);
  }

  /**
   * Start downloading a model from HuggingFace
   * Returns immediately with a job ID that can be polled for progress
   */
  async downloadModel(modelId: string): Promise<types.DownloadJobResponse> {
    return this.request<types.DownloadJobResponse>(`/v1/models/${encodeURIComponent(modelId)}/download`, {
      method: 'POST',
    });
  }

  /**
   * Get the status of a model download job
   */
  async getDownloadStatus(modelId: string, jobId: string): Promise<types.DownloadJobResponse> {
    return this.request<types.DownloadJobResponse>(
      `/v1/models/${encodeURIComponent(modelId)}/download/${jobId}`
    );
  }

  // Routing
  async debugRouting(data: types.RoutingDebugRequest): Promise<types.RoutingDebugResponse> {
    return this.request<types.RoutingDebugResponse>('/v1/routing/debug', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getRoutingHistory(limit?: number): Promise<types.RoutingDecision[]> {
    const query = limit ? `?limit=${limit}` : '';
    return this.requestList<types.RoutingDecision>(`/v1/routing/history${query}`);
  }

  // Backend metadata
  async listBackends(): Promise<types.BackendListResponse> {
    return this.request<types.BackendListResponse>('/v1/backends');
  }

  async getBackendCapabilities(): Promise<types.BackendCapabilitiesResponse> {
    return this.request<types.BackendCapabilitiesResponse>('/v1/backends/capabilities');
  }

  async getBackendStatus(name: types.BackendName): Promise<types.BackendStatusResponse> {
    return this.request<types.BackendStatusResponse>(`/v1/backends/${name}/status`);
  }

  // Inference
  async infer(data: types.InferRequest, options: RequestInit = {}, skipRetry: boolean = false, cancelToken?: AbortSignal): Promise<types.InferResponse> {
    return this.request<types.InferResponse>('/v1/infer', {
      method: 'POST',
      body: JSON.stringify(data),
      ...options,
    }, skipRetry, cancelToken);
  }

  async batchInfer(data: types.BatchInferRequest, cancelToken?: AbortSignal): Promise<types.BatchInferResponse> {
    logger.info('Batch inference requested', {
      component: 'ApiClient',
      operation: 'batchInfer',
      batchSize: data.requests.length,
    });
    return this.request<types.BatchInferResponse>('/v1/infer/batch', {
      method: 'POST',
      body: JSON.stringify(data),
    }, false, cancelToken);
  }

  /**
   * Stream inference using the /v1/infer/stream endpoint with SSE
   *
   * @param data - The streaming inference request payload
   * @param callbacks - Event callbacks for streaming tokens
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Promise that resolves when stream completes
   */
  async streamInfer(
    data: types.StreamingInferRequest,
    callbacks: {
      onToken: (token: string, chunk: types.StreamingChunk) => void;
      onComplete: (fullText: string, finishReason: string | null, metadata?: { unavailable_pinned_adapters?: string[], pinned_routing_fallback?: string, citations?: types.Citation[] }) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void> {
    const url = `${this.baseUrl}/v1/infer/stream`;

    logger.info('Streaming inference requested', {
      component: 'ApiClient',
      operation: 'streamInfer',
      prompt_length: data.prompt.length,
    });

    const MAX_RESPONSE_SIZE = 10 * 1024 * 1024; // 10MB limit
    let fullText = '';

    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(data),
        credentials: 'include',
        signal: cancelToken,
      });

      if (!response.ok) {
        let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
        try {
          const error = await response.json();
          errorMessage = error.error || errorMessage;
        } catch {
          // If JSON parsing fails, use status text
        }
        throw new Error(errorMessage);
      }

      const contentType = response.headers.get('content-type');
      if (!contentType?.includes('text/event-stream') && !contentType?.includes('application/stream')) {
        throw new Error(`Unexpected content type: ${contentType}. Expected streaming response.`);
      }

      if (!response.body) {
        throw new Error('Response body is null - streaming not supported');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let finishReason: string | null = null;
      let unavailablePinnedAdapters: string[] | undefined = undefined;
      let pinnedRoutingFallback: string | undefined = undefined;
      let citations: types.Citation[] | undefined = undefined;
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          break;
        }

        // Decode the chunk and add to buffer
        buffer += decoder.decode(value, { stream: true });

        // Process complete SSE messages from buffer
        const lines = buffer.split('\n');
        // Keep the last incomplete line in buffer
        buffer = lines.pop() || '';

        for (const line of lines) {
          const trimmedLine = line.trim();

          // Skip empty lines and comments
          if (!trimmedLine || trimmedLine.startsWith(':')) {
            continue;
          }

          // Handle SSE data lines
          if (trimmedLine.startsWith('data: ')) {
            const data = trimmedLine.slice(6);

            // Check for stream termination
            if (data === '[DONE]') {
              const metadata = (unavailablePinnedAdapters || pinnedRoutingFallback || citations) ? {
                unavailable_pinned_adapters: unavailablePinnedAdapters,
                pinned_routing_fallback: pinnedRoutingFallback,
                citations,
              } : undefined;
              callbacks.onComplete(fullText, finishReason, metadata);
              return;
            }

            try {
              const chunk = JSON.parse(data) as types.StreamingChunk;

              // Extract token from delta
              const choice = chunk.choices?.[0];
              if (choice) {
                const token = choice.delta?.content || '';
                if (token) {
                  fullText += token;

                  // Check if response size exceeds limit
                  if (fullText.length > MAX_RESPONSE_SIZE) {
                    reader.cancel();
                    throw new Error('Response exceeded maximum size limit');
                  }

                  callbacks.onToken(token, chunk);
                }

                // Check for finish reason
                if (choice.finish_reason) {
                  finishReason = choice.finish_reason;
                }
              }
            } catch (parseError) {
              // Try parsing as a Done event with pinned adapter metadata
              try {
                const event = JSON.parse(data) as { event?: string; unavailable_pinned_adapters?: string[]; pinned_routing_fallback?: string; citations?: types.Citation[] };
                if (event.event === 'Done') {
                  unavailablePinnedAdapters = event.unavailable_pinned_adapters;
                  pinnedRoutingFallback = event.pinned_routing_fallback;
                  citations = event.citations;
                }
              } catch {
                // Not a Done event, log the original parse error
                logger.warn('Failed to parse streaming chunk', {
                  component: 'ApiClient',
                  operation: 'streamInfer',
                  data,
                });
              }
            }
          }
        }
      }

      // Stream ended normally (without [DONE])
      const metadata = (unavailablePinnedAdapters || pinnedRoutingFallback || citations) ? {
        unavailable_pinned_adapters: unavailablePinnedAdapters,
        pinned_routing_fallback: pinnedRoutingFallback,
        citations,
      } : undefined;
      callbacks.onComplete(fullText, finishReason, metadata);
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        logger.info('Streaming inference cancelled', {
          component: 'ApiClient',
          operation: 'streamInfer',
        });
        callbacks.onComplete(fullText || '', 'cancelled', undefined);
        return;
      }

      logger.error('Streaming inference failed', {
        component: 'ApiClient',
        operation: 'streamInfer',
      }, error instanceof Error ? error : new Error(String(error)));
      callbacks.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  // ===== Phase 6: Policy Operations =====
  async signPolicy(cpid: string): Promise<types.SignPolicyResponse> {
    return this.request<types.SignPolicyResponse>(`/v1/policies/${cpid}/sign`, {
      method: 'POST',
    });
  }

  async comparePolicies(cpid1: string, cpid2: string): Promise<types.PolicyComparisonResponse> {
    return this.request<types.PolicyComparisonResponse>('/v1/policies/compare', {
      method: 'POST',
      body: JSON.stringify({ cpid_1: cpid1, cpid_2: cpid2 }),
    });
  }

  async exportPolicy(cpid: string): Promise<types.ExportPolicyResponse> {
    return this.request<types.ExportPolicyResponse>(`/v1/policies/${cpid}/export`);
  }

  // ===== Phase 7: Promotion Execution =====
  async dryRunPromotion(cpid: string): Promise<types.DryRunPromotionResponse> {
    return this.request<types.DryRunPromotionResponse>('/v1/cp/promote/dry-run', {
      method: 'POST',
      body: JSON.stringify({ cpid }),
    });
  }

  async getPromotionHistory(): Promise<types.PromotionHistoryEntry[]> {
    return this.requestList<types.PromotionHistoryEntry>('/v1/cp/promotions');
  }

  // ===== Phase 8: Telemetry Operations =====
  // (duplicate method removed; see canonical definition above returning TelemetryBundle[])
  async exportTelemetryBundle(bundleId: string): Promise<types.ExportTelemetryBundleResponse> {
    return this.request<types.ExportTelemetryBundleResponse>(`/v1/telemetry/bundles/${bundleId}/export`);
  }

  async generateTelemetryBundle(): Promise<{ id: string; cpid: string; event_count: number; size_bytes: number; created_at: string }> {
    return this.request('/v1/telemetry/bundles/generate', { method: 'POST' });
  }

  async verifyBundleSignature(bundleId: string): Promise<types.VerifyBundleSignatureResponse> {
    return this.request<types.VerifyBundleSignatureResponse>(`/v1/telemetry/bundles/${bundleId}/verify`, {
      method: 'POST',
    });
  }

  async purgeOldBundles(keepCount: number): Promise<types.PurgeOldBundlesResponse> {
    return this.request<types.PurgeOldBundlesResponse>('/v1/telemetry/bundles/purge', {
      method: 'POST',
      body: JSON.stringify({ keep_bundles_per_cpid: keepCount }),
    });
  }

  // ===== Phase 9: Code Intelligence =====
  async getRepositoryReport(repoId: string): Promise<types.RepositoryReportResponse> {
    return this.request<types.RepositoryReportResponse>(`/v1/repositories/${repoId}/report`);
  }

  async unregisterRepository(repoId: string): Promise<void> {
    return this.request<void>(`/v1/repositories/${repoId}`, {
      method: 'DELETE',
    });
  }

  async getScanStatus(jobId: string): Promise<types.TriggerScanResponse> {
    return this.request<types.TriggerScanResponse>(`/v1/code/scan/${jobId}`);
  }

  async createCommitDelta(delta: types.CommitDeltaRequest): Promise<types.CommitDeltaResponse> {
    return this.request<types.CommitDeltaResponse>('/v1/code/commit-delta', {
      method: 'POST',
      body: JSON.stringify(delta),
    });
  }

  // ===== Phase 10: Tenant Management =====
  async updateTenant(tenantId: string, name: string): Promise<types.TenantResponse> {
    return this.request<types.TenantResponse>(`/v1/tenants/${tenantId}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    });
  }

  async pauseTenant(tenantId: string): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/pause`, {
      method: 'POST',
    });
  }

  async archiveTenant(tenantId: string): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/archive`, {
      method: 'POST',
    });
  }

  async assignTenantPolicies(tenantId: string, cpids: string[]): Promise<types.AssignPoliciesResponse> {
    return this.request<types.AssignPoliciesResponse>(`/v1/tenants/${tenantId}/policies`, {
      method: 'POST',
      body: JSON.stringify({ cpids }),
    });
  }

  async assignTenantAdapters(tenantId: string, adapterIds: string[]): Promise<types.AssignAdaptersResponse> {
    return this.request<types.AssignAdaptersResponse>(`/v1/tenants/${tenantId}/adapters`, {
      method: 'POST',
      body: JSON.stringify({ adapter_ids: adapterIds }),
    });
  }

  async getTenantUsage(tenantId: string): Promise<types.TenantUsageResponse> {
    return this.request<types.TenantUsageResponse>(`/v1/tenants/${tenantId}/usage`);
  }

  // Git Repository API
  async registerGitRepository(request: types.RegisterRepositoryRequest): Promise<types.RegisterGitRepositoryResponse> {
    return this.request<types.RegisterGitRepositoryResponse>(`/v1/git/repositories`, {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async getRepositoryAnalysis(repoId: string): Promise<unknown> {
    return this.request(`/v1/git/repositories/${repoId}/analysis`);
  }

  async trainRepositoryAdapter(repoId: string, config: Record<string, unknown>): Promise<{
    training_id: string;
    status: string;
    estimated_duration: string;
    evidence_count: number;
  }> {
    return this.request(`/v1/git/repositories/${repoId}/train`, {
      method: 'POST',
      body: JSON.stringify({ config }),
    });
  }

  // Domain Adapter API
  async listDomainAdapters(): Promise<types.DomainAdapter[]> {
    return this.requestList<types.DomainAdapter>('/v1/domain-adapters');
  }

  async testDomainAdapter(adapterId: string, inputData: string, expectedOutput?: string, iterations?: number): Promise<types.TestDomainAdapterResponse> {
    return this.request<types.TestDomainAdapterResponse>(`/v1/domain-adapters/${adapterId}/test`, {
      method: 'POST',
      body: JSON.stringify({
        adapter_id: adapterId,
        input_data: inputData,
        expected_output: expectedOutput,
        iterations: iterations || 100,
      }),
    });
  }

  // Adapter Stack API
  async listAdapterStacks(): Promise<types.AdapterStack[]> {
    // Backend returns StackResponse[] with adapter_ids, map to AdapterStack
    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    type BackendStack = {
      id: string;
      name: string;
      adapter_ids: string[];
      description?: string;
      created_at: string;
      updated_at: string;
      version?: number;
      workflow_type?: string;
    };
    const rawResponse = await this.request<unknown>('/v1/adapter-stacks');
    const backendStacks = extractArrayFromResponse<BackendStack>(rawResponse);

    return backendStacks.map(stack => ({
      id: stack.id,
      name: stack.name,
      adapter_ids: stack.adapter_ids,
      description: stack.description,
      created_at: stack.created_at,
      updated_at: stack.updated_at,
      version: stack.version,
      workflow_type: stack.workflow_type as 'Parallel' | 'UpstreamDownstream' | 'Sequential' | undefined,
    }));
  }

  async createAdapterStack(stack: types.CreateAdapterStackRequest): Promise<types.AdapterStackResponse> {
    const response = await this.request<types.AdapterStackResponse>('/v1/adapter-stacks', {
      method: 'POST',
      body: JSON.stringify(stack),
    });
    return response;
  }

  async getAdapterStack(id: string): Promise<types.AdapterStack> {
    const response = await this.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}`);
    return response.stack;
  }

  async deleteAdapterStack(id: string): Promise<void> {
    return this.request<void>(`/v1/adapter-stacks/${id}`, {
      method: 'DELETE',
    });
  }

  async getAdapterStackHistory(id: string): Promise<types.LifecycleHistoryEvent[]> {
    return this.requestList<types.LifecycleHistoryEvent>(`/v1/adapter-stacks/${id}/history`);
  }

  async updateAdapterStack(id: string, data: types.UpdateAdapterStackRequest): Promise<types.AdapterStack> {
    const response = await this.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return response.stack;
  }

  /**
   * Run preflight policy checks before activating an adapter stack
   * 【2025-11-25†ui†stack-preflight-checks】
   */
  async preflightStackActivation(stackId: string): Promise<types.PolicyPreflightResponse> {
    return this.request<types.PolicyPreflightResponse>(
      `/v1/adapter-stacks/${encodeURIComponent(stackId)}/activate/preflight`,
      { method: 'POST' }
    );
  }

  async activateAdapterStack(id: string): Promise<types.AdapterStack> {
    const response = await this.request<types.AdapterStackResponse>(`/v1/adapter-stacks/${id}/activate`, {
      method: 'POST',
    });
    return response.stack;
  }

  async deactivateAdapterStack(): Promise<void> {
    return this.request<void>('/v1/adapter-stacks/deactivate', {
      method: 'POST',
    });
  }

  /**
   * Get policies assigned to a stack with compliance summary
   * Stack-Policy API
   */
  async getStackPolicies(stackId: string): Promise<policyTypes.StackPoliciesResponse> {
    return this.request<policyTypes.StackPoliciesResponse>(
      `/v1/adapter-stacks/${encodeURIComponent(stackId)}/policies`
    );
  }

  // Packages API
  async listPackages(params?: { tenantId?: string; domain?: string }): Promise<types.AdapterPackage[]> {
    const qs = new URLSearchParams();
    if (params?.domain) {
      qs.append('domain', params.domain);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    const basePath = params?.tenantId
      ? `/v1/tenants/${encodeURIComponent(params.tenantId)}/packages`
      : '/v1/packages';

    const response = await this.request<types.PackageListResponse>(`${basePath}${query}`);
    if ('packages' in response && Array.isArray((response as types.PackageListResponse).packages)) {
      return (response as types.PackageListResponse).packages;
    }
    return [];
  }

  async createPackage(payload: types.CreatePackageRequest): Promise<types.AdapterPackage> {
    const response = await this.request<types.PackageResponse>('/v1/packages', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
    return response.package;
  }

  async getPackage(id: string): Promise<types.AdapterPackage> {
    const response = await this.request<types.PackageResponse>(`/v1/packages/${id}`);
    return response.package;
  }

  async updatePackage(id: string, payload: types.UpdatePackageRequest): Promise<types.AdapterPackage> {
    const response = await this.request<types.PackageResponse>(`/v1/packages/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
    return response.package;
  }

  async deletePackage(id: string): Promise<void> {
    return this.request<void>(`/v1/packages/${id}`, {
      method: 'DELETE',
    });
  }

  async installTenantPackage(tenantId: string, packageId: string): Promise<types.AdapterPackage> {
    const response = await this.request<types.PackageResponse>(
      `/v1/tenants/${encodeURIComponent(tenantId)}/packages/${encodeURIComponent(packageId)}/install`,
      { method: 'POST' }
    );
    return response.package;
  }

  async uninstallTenantPackage(tenantId: string, packageId: string): Promise<types.AdapterPackage> {
    const response = await this.request<types.PackageResponse>(
      `/v1/tenants/${encodeURIComponent(tenantId)}/packages/${encodeURIComponent(packageId)}/install`,
      { method: 'DELETE' }
    );
    return response.package;
  }

  async getDefaultAdapterStack(tenantId: string = 'default'): Promise<types.AdapterStack | null> {
    try {
      const response = await this.request<types.DefaultStackResponse>(`/v1/tenants/${tenantId}/default-stack`);
      if (response.stack_id) {
        return await this.getAdapterStack(response.stack_id);
      }
      return null;
    } catch (error: unknown) {
      if (error instanceof Error && 'status' in error && (error as ApiError).status === 404) {
        return null;
      }
      throw error;
    }
  }

  async setDefaultAdapterStack(stackId: string, tenantId: string = 'default'): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/default-stack`, {
      method: 'PUT',
      body: JSON.stringify({ stack_id: stackId }),
    });
  }

  async clearDefaultAdapterStack(tenantId: string = 'default'): Promise<void> {
    return this.request<void>(`/v1/tenants/${tenantId}/default-stack`, {
      method: 'DELETE',
    });
  }

  async validateStackName(name: string): Promise<types.ValidateStackNameResponse> {
    return this.request<types.ValidateStackNameResponse>('/v1/stacks/validate-name', {
      method: 'POST',
      body: JSON.stringify({ name }),
    });
  }

  // Monitoring API
  async listMonitoringRules(tenantId?: string): Promise<types.MonitoringRule[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.requestList<types.MonitoringRule>(`/v1/monitoring/rules${query}`);
  }

  async createMonitoringRule(data: types.CreateMonitoringRuleRequest): Promise<types.MonitoringRule> {
    return this.request<types.MonitoringRule>('/v1/monitoring/rules', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async deleteMonitoringRule(ruleId: string): Promise<void> {
    return this.request<void>(`/v1/monitoring/rules/${ruleId}`, {
      method: 'DELETE',
    });
  }

  async listAlerts(filters?: types.AlertFilters): Promise<types.Alert[]> {
    const params = new URLSearchParams();
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    if (filters?.worker_id) params.append('worker_id', filters.worker_id);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.severity) params.append('severity', filters.severity);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.requestList<types.Alert>(`/v1/monitoring/alerts${query}`);
  }

  async acknowledgeAlert(alertId: string, data: types.AcknowledgeAlertRequest): Promise<types.Alert> {
    return this.request<types.Alert>(`/v1/monitoring/alerts/${alertId}/acknowledge`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateMonitoringRule(ruleId: string, data: apiTypes.UpdateMonitoringRuleRequest): Promise<types.MonitoringRule> {
    return this.request<types.MonitoringRule>(`/v1/monitoring/rules/${ruleId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async resolveAlert(alertId: string, data?: types.ResolveAlertRequest): Promise<types.Alert> {
    return this.request<types.Alert>(`/v1/monitoring/alerts/${alertId}/resolve`, {
      method: 'POST',
      body: JSON.stringify(data || {}),
    });
  }

  async listHealthMetrics(tenantId?: string): Promise<types.HealthMetric[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.requestList<types.HealthMetric>(`/v1/monitoring/health-metrics${query}`);
  }

  async listAnomalies(): Promise<apiTypes.Anomaly[]> {
    return this.requestList<apiTypes.Anomaly>('/v1/monitoring/anomalies');
  }

  async updateAnomalyStatus(anomalyId: string, data: apiTypes.UpdateAnomalyStatusRequest): Promise<apiTypes.Anomaly> {
    return this.request<apiTypes.Anomaly>(`/v1/monitoring/anomalies/${anomalyId}/status`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  // Replay API
  async listReplaySessions(tenantId?: string): Promise<types.ReplaySession[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.requestList<types.ReplaySession>(`/v1/replay/sessions${query}`);
  }

  async getReplaySession(sessionId: string): Promise<types.ReplaySession> {
    return this.request<types.ReplaySession>(`/v1/replay/sessions/${sessionId}`);
  }

  async createReplaySession(data: types.CreateReplaySessionRequest): Promise<types.ReplaySession> {
    return this.request<types.ReplaySession>('/v1/replay/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async verifyReplaySession(sessionId: string): Promise<types.ReplayVerificationResponse> {
    return this.request<types.ReplayVerificationResponse>(`/v1/replay/sessions/${sessionId}/verify`, {
      method: 'POST',
    });
  }

  async deleteReplaySession(sessionId: string): Promise<void> {
    return this.request<void>(`/v1/replay/sessions/${sessionId}`, {
      method: 'DELETE',
    });
  }

  // Memory management methods
  async getMemoryUsage(): Promise<{
    adapters: Array<{
      id: string;
      name: string;
      memory_usage_mb: number;
      state: string;
      pinned: boolean;
      category: string;
    }>;
    total_memory_mb: number;
    available_memory_mb: number;
    memory_pressure_level: 'low' | 'medium' | 'high' | 'critical';
  }> {
    return this.request('/v1/memory/usage');
  }

  async evictAdapter(adapterId: string): Promise<{ success: boolean; message: string }> {
    return this.request(`/v1/memory/adapters/${adapterId}/evict`, {
      method: 'POST',
    });
  }

  // Note: pinAdapter method is consolidated above in Adapter lifecycle management section

  // Training methods
  async startAdapterTraining(data: {
    repository_path: string;
    adapter_name: string;
    description: string;
    training_config: Record<string, unknown>;
    tenant_id: string;
  }): Promise<{ session_id: string; status: string; created_at: string }> {
    return this.request<{ session_id: string; status: string; created_at: string }>('/v1/training/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'pending' | 'running' | 'completed' | 'failed';
    progress: number;
    adapter_name: string;
    repository_path: string;
    created_at: string;
    updated_at: string;
    error_message?: string;
  }> {
    return this.request(`/v1/training/sessions/${sessionId}`);
  }

  async listTrainingSessions(tenantId?: string): Promise<Array<{
    session_id: string;
    status: string;
    adapter_name: string;
    repository_path: string;
    created_at: string;
    updated_at: string;
  }>> {
    const params = new URLSearchParams();
    if (tenantId) params.append('tenant_id', tenantId);
    
    const queryString = params.toString();
    return this.request(`/v1/training/sessions${queryString ? `?${queryString}` : ''}`);
  }

  async pauseTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'running' | 'cancelled';
    message: string;
  }> {
    return Promise.reject(
      new Error('Training pause/resume is not supported in this build')
    );
  }

  async resumeTrainingSession(sessionId: string): Promise<{
    session_id: string;
    status: 'running';
    message: string;
  }> {
    return Promise.reject(
      new Error('Training pause/resume is not supported in this build')
    );
  }

  // Telemetry methods
  async getTelemetryEvents(filters?: {
    limit?: number;
    tenantId?: string;
    userId?: string;
    startTime?: string;
    endTime?: string;
    eventType?: string;
    eventTypes?: string[];
    level?: string;
  }): Promise<types.TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());

    const normalizedEventTypes =
      filters?.eventTypes?.length ? filters.eventTypes : filters?.eventType ? [filters.eventType] : [];
    normalizedEventTypes.forEach((evt) => params.append('event_types[]', evt));

    const queryString = params.toString();
    return this.requestList<types.TelemetryEvent>(
      `/v1/telemetry/events/recent${queryString ? `?${queryString}` : ''}`,
    );
  }

  // Logs API methods
  async queryLogs(filters?: {
    limit?: number;
    tenant_id?: string;
    event_type?: string;
    level?: string;
    component?: string;
    trace_id?: string;
  }): Promise<types.UnifiedTelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    if (filters?.event_type) params.append('event_type', filters.event_type);
    if (filters?.level) params.append('level', filters.level);
    if (filters?.component) params.append('component', filters.component);
    if (filters?.trace_id) params.append('trace_id', filters.trace_id);

    const queryString = params.toString();
    return this.requestList<types.UnifiedTelemetryEvent>(`/v1/logs/query${queryString ? `?${queryString}` : ''}`);
  }

  // Metrics API methods
  async getMetricsSnapshot(): Promise<types.MetricsSnapshotResponse> {
    return this.request<types.MetricsSnapshotResponse>('/v1/metrics/snapshot');
  }

  async getMetricsSeries(params?: {
    series_name?: string;
    start_ms?: number;
    end_ms?: number;
  }): Promise<types.MetricsSeriesResponse[]> {
    const queryParams = new URLSearchParams();
    if (params?.series_name) queryParams.append('series_name', params.series_name);
    if (params?.start_ms) queryParams.append('start_ms', params.start_ms.toString());
    if (params?.end_ms) queryParams.append('end_ms', params.end_ms.toString());

    const queryString = queryParams.toString();
    return this.requestList<types.MetricsSeriesResponse>(`/v1/metrics/series${queryString ? `?${queryString}` : ''}`);
  }

  // Traces API methods
  async searchTraces(params?: {
    span_name?: string;
    status?: string;
    start_time_ns?: number;
    end_time_ns?: number;
  }): Promise<string[]> {
    const queryParams = new URLSearchParams();
    if (params?.span_name) queryParams.append('span_name', params.span_name);
    if (params?.status) queryParams.append('status', params.status);
    if (params?.start_time_ns) queryParams.append('start_time_ns', params.start_time_ns.toString());
    if (params?.end_time_ns) queryParams.append('end_time_ns', params.end_time_ns.toString());

    const queryString = queryParams.toString();
    return this.requestList<string>(`/v1/traces/search${queryString ? `?${queryString}` : ''}`);
  }

  async getTrace(traceId: string): Promise<types.Trace | null> {
    return this.request<types.Trace | null>(`/v1/traces/${traceId}`);
  }

  // Audit export API method
  async exportAuditLogs(params?: {
    format?: 'csv' | 'json';
    startTime?: string;
    endTime?: string;
    tenantId?: string;
    eventType?: string;
    level?: string;
  }): Promise<Blob> {
    const queryParams = new URLSearchParams();
    if (params?.format) queryParams.append('format', params.format);
    if (params?.startTime) queryParams.append('start_time', params.startTime);
    if (params?.endTime) queryParams.append('end_time', params.endTime);
    if (params?.tenantId) queryParams.append('tenant_id', params.tenantId);
    if (params?.eventType) queryParams.append('event_type', params.eventType);
    if (params?.level) queryParams.append('level', params.level);

    const queryString = queryParams.toString();
    const path = `/v1/audits/export${queryString ? `?${queryString}` : ''}`;
    const url = `${this.baseUrl}${path}`;

    const response = await fetch(url, {
      credentials: 'include',
    });

    return handleBlobResponse(response, { method: 'GET', path });
  }

  // Compliance audit API method
  // Returns compliance controls and policy violations from policy_quarantine table
  async getComplianceAudit(): Promise<types.ComplianceAuditResponse> {
    return this.request<types.ComplianceAuditResponse>('/v1/audit/compliance');
  }

  // Query audit logs with filters
  async queryAuditLogs(filters?: types.AuditLogFilters): Promise<types.AuditLog[]> {
    const params = new URLSearchParams();
    if (filters?.action) params.append('action', filters.action);
    if (filters?.user_id) params.append('user_id', filters.user_id);
    if (filters?.resource) params.append('resource', filters.resource);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    if (filters?.tenant_id) params.append('tenant_id', filters.tenant_id);
    const query = params.toString() ? `?${params.toString()}` : '';

    // DEFENSIVE: Use extractArrayFromResponse to handle potential PaginatedResponse migration
    const rawResponse = await this.request<unknown>(`/v1/audit/logs${query}`);
    const logs = extractArrayFromResponse<types.AuditLogEntry>(rawResponse);
    return logs.map((log) => ({
      id: log.id,
      user_id: log.user_id,
      action: log.action,
      resource: log.resource_type,
      resource_id: log.resource_id,
      status: log.status,
      timestamp: log.timestamp,
      ip_address: log.ip_address,
      user_agent: undefined,
      details: parseAuditMetadata(log.metadata_json),
      tenant_id: log.tenant_id,
      session_id: undefined,
      user_role: log.user_role,
      error_message: log.error_message,
      metadata_json: log.metadata_json,
    }));
  }

  // Run tenant isolation test
  async runIsolationTest(scenarioId: string, tenantId: string): Promise<types.IsolationTestResult> {
    return this.request<types.IsolationTestResult>('/v1/security/isolation/test', {
      method: 'POST',
      body: JSON.stringify({ scenario_id: scenarioId, tenant_id: tenantId }),
    });
  }

  // Get anomaly detection status
  async getAnomalyDetectionStatus(): Promise<types.AnomalyDetectionStatus> {
    return this.request<types.AnomalyDetectionStatus>('/v1/security/anomaly/status');
  }

  // Get access patterns for visualization
  async getAccessPatterns(tenantId?: string): Promise<types.AccessPattern[]> {
    const query = tenantId ? `?tenant_id=${tenantId}` : '';
    return this.requestList<types.AccessPattern>(`/v1/security/access-patterns${query}`);
  }

  // Federation management methods
  // Get overall federation status including node health and sync status
  async getFederationStatus(): Promise<federationTypes.FederationStatusResponse> {
    return this.request<federationTypes.FederationStatusResponse>('/v1/federation/status');
  }

  // Get current quarantine status for all nodes
  async getQuarantineStatus(): Promise<federationTypes.QuarantineStatusResponse> {
    return this.request<federationTypes.QuarantineStatusResponse>('/v1/federation/quarantine');
  }

  // Release a node from quarantine
  async releaseQuarantine(request: federationTypes.ReleaseQuarantineRequest): Promise<federationTypes.ReleaseQuarantineResponse> {
    return this.request<federationTypes.ReleaseQuarantineResponse>('/v1/federation/release-quarantine', {
      method: 'POST',
      body: JSON.stringify(request),
    }, false, undefined, true); // allowMutationRetry: true for safety
  }

  // Get federation audit logs with optional filters
  async getFederationAudit(filters?: federationTypes.FederationAuditFilters): Promise<federationTypes.FederationAuditResponse> {
    const params = new URLSearchParams();
    if (filters?.event_type) params.append('event_type', filters.event_type);
    if (filters?.node_id) params.append('node_id', filters.node_id);
    if (filters?.status) params.append('status', filters.status);
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.offset) params.append('offset', filters.offset.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.request<federationTypes.FederationAuditResponse>(`/v1/audit/federation${query}`);
  }

  // Get list of federated peers with sync status
  async getFederationPeers(): Promise<federationTypes.PeerListResponse> {
    return this.request<federationTypes.PeerListResponse>('/v1/federation/peers');
  }

  // Process debugging methods
  async getProcessLogs(workerId: string, filters?: types.ProcessLogFilters): Promise<types.ProcessLog[]> {
    const params = new URLSearchParams();
    if (filters?.level) params.append('level', filters.level);
    if (filters?.limit) params.append('limit', filters.limit.toString());
    if (filters?.start_time) params.append('start_time', filters.start_time);
    if (filters?.end_time) params.append('end_time', filters.end_time);
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.requestList<types.ProcessLog>(`/v1/workers/${workerId}/logs${query}`);
  }

  async getProcessCrashes(workerId: string): Promise<types.ProcessCrash[]> {
    return this.requestList<types.ProcessCrash>(`/v1/workers/${workerId}/crashes`);
  }

  async startDebugSession(workerId: string, config: types.DebugSessionConfig): Promise<types.DebugSession> {
    return this.request<types.DebugSession>(`/v1/workers/${workerId}/debug`, {
      method: 'POST',
      body: JSON.stringify(config),
    });
  }

  async runTroubleshootingStep(workerId: string, step: types.TroubleshootingStep): Promise<types.TroubleshootingResult> {
    return this.request<types.TroubleshootingResult>(`/v1/workers/${workerId}/troubleshoot`, {
      method: 'POST',
      body: JSON.stringify(step),
    });
  }

  async getWorkerIncidents(workerId: string, limit?: number): Promise<types.WorkerIncident[]> {
    const params = new URLSearchParams();
    if (limit !== undefined) params.append('limit', limit.toString());
    const query = params.toString() ? `?${params.toString()}` : '';
    return this.requestList<types.WorkerIncident>(`/v1/workers/${workerId}/incidents${query}`);
  }

  async getWorkersHealthSummary(): Promise<types.WorkerHealthSummary[]> {
    return this.requestList<types.WorkerHealthSummary>('/v1/workers/health/summary');
  }

  // Routing methods
  async getSessionRouterView(requestId: string): Promise<types.SessionRouterViewResponse> {
    return this.request<types.SessionRouterViewResponse>(`/v1/routing/sessions/${requestId}`);
  }

  async getRouterConfig(tenantId: string): Promise<types.RouterConfigView> {
    const effectiveTenant = tenantId || 'default';
    return this.request<types.RouterConfigView>(
      `/v1/tenants/${effectiveTenant}/router/config`
    );
  }

  async getDeterminismStatus(): Promise<types.DeterminismStatusResponse> {
    return this.request<types.DeterminismStatusResponse>('/v1/diagnostics/determinism-status');
  }

  async getDiagnosticsQuarantineStatus(): Promise<types.AdapterQuarantineStatusResponse> {
    return this.request<types.AdapterQuarantineStatusResponse>('/v1/diagnostics/quarantine-status');
  }

  async getCapacity(): Promise<types.CapacityResponse> {
    return this.request<types.CapacityResponse>('/v1/system/capacity');
  }

  /**
   * Get current system settings
   *
   * GET /v1/settings
   */
  async getSettings(): Promise<documentTypes.SystemSettings> {
    return this.request<documentTypes.SystemSettings>('/v1/settings');
  }

  /**
   * Update system settings
   *
   * PUT /v1/settings
   */
  async updateSettings(
    request: documentTypes.UpdateSettingsRequest
  ): Promise<documentTypes.SettingsUpdateResponse> {
    return this.request<documentTypes.SettingsUpdateResponse>('/v1/settings', {
      method: 'PUT',
      body: JSON.stringify(request),
    });
  }

  async getRoutingDecisions(filters?: types.RoutingDecisionFilters): Promise<types.TransformedRoutingDecision[]> {
    const params = new URLSearchParams();
    // Backend requires 'tenant' parameter (not tenant_id) - see RoutingDecisionsQuery
    const tenantId = filters?.tenant_id || 'default';
    params.append('tenant', tenantId);

    if (filters?.limit) {
      params.append('limit', filters.limit.toString());
    }
    if (filters?.offset) {
      params.append('offset', filters.offset.toString());
    }
    if (filters?.adapter_id) {
      params.append('adapter_id', filters.adapter_id);
    }
    if (filters?.stack_id) {
      params.append('stack_id', filters.stack_id);
    }
    if (filters?.since) {
      params.append('since', filters.since);
    }
    if (filters?.until) {
      params.append('until', filters.until);
    }
    if (filters?.min_entropy !== undefined) {
      params.append('min_entropy', filters.min_entropy.toString());
    }
    if (filters?.max_overhead_pct !== undefined) {
      params.append('max_overhead_pct', filters.max_overhead_pct.toString());
    }
    if (filters?.anomalies_only) {
      params.append('anomalies_only', 'true');
    }
    if (filters?.source_type) {
      params.append('source_type', filters.source_type);
    }

    const query = `?${params.toString()}`;

    logger.debug('Fetching routing decisions', {
      component: 'ApiClient',
      operation: 'getRoutingDecisions',
      query,
      tenant_id: tenantId,
    });
    
    // Backend returns RoutingDecisionsResponse with 'items' field
    interface BackendRoutingDecision {
      ts: string;
      tenant_id: string;
      adapters_used: string[];
      activations: number[];
      reason: string;
      trace_id: string;
    }
    
    interface BackendRoutingDecisionsResponse {
      items: BackendRoutingDecision[];
    }
    
    const response = await this.request<BackendRoutingDecisionsResponse>(`/v1/routing/decisions${query}`);
    
    // Transform backend format to frontend format
    // Must satisfy TransformedRoutingDecision which extends RoutingDecision
    return response.items.map((item, index) => {
      const scores: Record<string, number> = {};
      item.adapters_used.forEach((adapter, i) => {
        scores[adapter] = item.activations[i] || 0;
      });

      // Transform candidates into RouterCandidateInfo objects
      const candidates: types.RouterCandidateInfo[] = item.adapters_used.map((adapter, i) => ({
        adapter_id: adapter,
        adapter_idx: i,
        gate_q15: Math.round((item.activations[i] || 0) * 32767), // Convert float to Q15
        gate_float: item.activations[i] || 0,
        raw_score: item.activations[i] || 0,
        selected: true, // All adapters_used are selected
      }));

      return {
        // Required TransformedRoutingDecision fields
        id: item.trace_id || `decision-${index}`,
        request_id: item.trace_id || `decision-${index}`,
        selected_adapters: item.adapters_used,
        scores,
        timestamp: item.ts,
        latency_ms: 0, // Not provided by backend

        // Additional TransformedRoutingDecision fields
        transformed: true,
        display_adapters: item.adapters_used,

        // Routing inspector fields
        entropy: this.calculateEntropy(item.activations),
        k_value: item.adapters_used.length,
        router_latency_us: undefined, // Not provided by backend
        candidates,

        // Optional fields
        tau: 1.0,
        entropy_floor: 0.0,
        step: index,
      };
    });
  }
  
  private calculateEntropy(values: number[]): number {
    if (values.length === 0) return 0;
    // Normalize values to probabilities
    const sum = values.reduce((a, b) => a + b, 0);
    if (sum === 0) return 0;
    const probs = values.map(v => v / sum);
    // Calculate Shannon entropy
    return -probs.reduce((entropy, p) => {
      if (p === 0) return entropy;
      return entropy + p * Math.log2(p);
    }, 0);
  }

  // Workspace methods
  async listWorkspaces(): Promise<types.Workspace[]> {
    return this.requestList<types.Workspace>('/v1/workspaces');
  }

  async listUserWorkspaces(): Promise<types.Workspace[]> {
    return this.requestList<types.Workspace>('/v1/workspaces/my');
  }

  async createWorkspace(data: types.CreateWorkspaceRequest): Promise<types.Workspace> {
    return this.request<types.Workspace>('/v1/workspaces', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getWorkspace(workspaceId: string): Promise<types.Workspace> {
    return this.request<types.Workspace>(`/v1/workspaces/${workspaceId}`);
  }

  async updateWorkspace(workspaceId: string, data: { name?: string; description?: string }): Promise<types.Workspace> {
    return this.request<types.Workspace>(`/v1/workspaces/${workspaceId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteWorkspace(workspaceId: string): Promise<void> {
    return this.request<void>(`/v1/workspaces/${workspaceId}`, {
      method: 'DELETE',
    });
  }

  async listWorkspaceMembers(workspaceId: string): Promise<types.WorkspaceMember[]> {
    return this.requestList<types.WorkspaceMember>(`/v1/workspaces/${workspaceId}/members`);
  }

  async addWorkspaceMember(workspaceId: string, data: types.AddWorkspaceMemberRequest): Promise<{ id: string }> {
    return this.request<{ id: string }>(`/v1/workspaces/${workspaceId}/members`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateWorkspaceMember(workspaceId: string, memberId: string, data: { role: string }): Promise<void> {
    return this.request<void>(`/v1/workspaces/${workspaceId}/members/${memberId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async removeWorkspaceMember(workspaceId: string, memberId: string): Promise<void> {
    return this.request<void>(`/v1/workspaces/${workspaceId}/members/${memberId}`, {
      method: 'DELETE',
    });
  }

  async listWorkspaceResources(workspaceId: string): Promise<types.WorkspaceResource[]> {
    return this.requestList<types.WorkspaceResource>(`/v1/workspaces/${workspaceId}/resources`);
  }

  async shareWorkspaceResource(workspaceId: string, data: { resource_type: string; resource_id: string }): Promise<{ id: string }> {
    return this.request<{ id: string }>(`/v1/workspaces/${workspaceId}/resources`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async unshareWorkspaceResource(workspaceId: string, resourceId: string, resourceType: string): Promise<void> {
    const params = new URLSearchParams({ resource_type: resourceType });
    return this.request<void>(`/v1/workspaces/${workspaceId}/resources/${resourceId}?${params.toString()}`, {
      method: 'DELETE',
    });
  }

  // Messaging methods
  async listWorkspaceMessages(workspaceId: string, params?: { limit?: number; offset?: number }): Promise<types.Message[]> {
    const queryParams = new URLSearchParams();
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.requestList<types.Message>(`/v1/workspaces/${workspaceId}/messages${query}`);
  }

  async createMessage(workspaceId: string, data: types.CreateMessageRequest): Promise<types.Message> {
    return this.request<types.Message>(`/v1/workspaces/${workspaceId}/messages`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async editMessage(workspaceId: string, messageId: string, data: types.CreateMessageRequest): Promise<types.Message> {
    return this.request<types.Message>(`/v1/workspaces/${workspaceId}/messages/${messageId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async getMessageThread(workspaceId: string, threadId: string): Promise<types.Message[]> {
    return this.requestList<types.Message>(`/v1/workspaces/${workspaceId}/messages/${threadId}/thread`);
  }

  async markMessageRead(workspaceId: string, messageId: string): Promise<types.Message> {
    return this.request<types.Message>(`/v1/workspaces/${workspaceId}/messages/${messageId}/read`, {
      method: 'POST',
    });
  }

  async deleteMessage(workspaceId: string, messageId: string): Promise<void> {
    return this.request<void>(`/v1/workspaces/${workspaceId}/messages/${messageId}`, {
      method: 'DELETE',
    });
  }

  // Notification methods
  async listNotifications(params?: {
    workspace_id?: string;
    type?: string;
    unread_only?: boolean;
    limit?: number;
    offset?: number;
  }): Promise<types.Notification[]> {
    const queryParams = new URLSearchParams();
    if (params?.workspace_id) queryParams.append('workspace_id', params.workspace_id);
    if (params?.type) queryParams.append('type', params.type);
    if (params?.unread_only !== undefined) queryParams.append('unread_only', params.unread_only.toString());
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.requestList<types.Notification>(`/v1/notifications${query}`);
  }

  async getNotificationSummary(workspaceId?: string): Promise<types.NotificationSummary> {
    const queryParams = new URLSearchParams();
    if (workspaceId) queryParams.append('workspace_id', workspaceId);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<types.NotificationSummary>(`/v1/notifications/summary${query}`);
  }

  async markNotificationRead(notificationId: string): Promise<void> {
    return this.request<void>(`/v1/notifications/${notificationId}/read`, {
      method: 'POST',
    });
  }

  async markAllNotificationsRead(workspaceId?: string): Promise<{ count: number }> {
    const queryParams = new URLSearchParams();
    if (workspaceId) queryParams.append('workspace_id', workspaceId);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<{ count: number }>(`/v1/notifications/read-all${query}`, {
      method: 'POST',
    });
  }

  // Tutorial methods
  async listTutorials(): Promise<types.Tutorial[]> {
    return this.requestList<types.Tutorial>('/v1/tutorials');
  }

  async markTutorialCompleted(tutorialId: string): Promise<void> {
    return this.request<void>(`/v1/tutorials/${tutorialId}/complete`, {
      method: 'POST',
    });
  }

  async unmarkTutorialCompleted(tutorialId: string): Promise<void> {
    return this.request<void>(`/v1/tutorials/${tutorialId}/complete`, {
      method: 'DELETE',
    });
  }

  async markTutorialDismissed(tutorialId: string): Promise<void> {
    return this.request<void>(`/v1/tutorials/${tutorialId}/dismiss`, {
      method: 'POST',
    });
  }

  async unmarkTutorialDismissed(tutorialId: string): Promise<void> {
    return this.request<void>(`/v1/tutorials/${tutorialId}/dismiss`, {
      method: 'DELETE',
    });
  }

  // Activity event methods
  async listActivityEvents(params?: {
    workspace_id?: string;
    user_id?: string;
    tenant_id?: string;
    event_type?: string;
    limit?: number;
    offset?: number;
  }): Promise<types.ActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (params?.workspace_id) queryParams.append('workspace_id', params.workspace_id);
    if (params?.user_id) queryParams.append('user_id', params.user_id);
    if (params?.tenant_id) queryParams.append('tenant_id', params.tenant_id);
    if (params?.event_type) queryParams.append('event_type', params.event_type);
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.offset) queryParams.append('offset', params.offset.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.requestList<types.ActivityEvent>(`/v1/activity${query}`);
  }

  async getRecentActivityEvents(params?: { event_types?: string[]; limit?: number }): Promise<types.RecentActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (params?.limit) {
      queryParams.append('limit', params.limit.toString());
    }
    params?.event_types?.forEach((eventType) => {
      queryParams.append('event_types[]', eventType);
    });
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.requestList<types.RecentActivityEvent>(`/v1/telemetry/events/recent${query}`);
  }

  async createActivityEvent(data: types.CreateActivityEventRequest): Promise<types.ActivityEvent> {
    return this.request<types.ActivityEvent>('/v1/activity', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async listUserWorkspaceActivity(limit?: number): Promise<types.ActivityEvent[]> {
    const queryParams = new URLSearchParams();
    if (limit) queryParams.append('limit', limit.toString());
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.requestList<types.ActivityEvent>(`/v1/activity/my${query}`);
  }

  subscribeToMetrics(callback: (metrics: SystemMetrics | null) => void): () => void {
    // With cookie-based auth, cookies are sent automatically with credentials: 'include'
    const sseUrl = import.meta.env.VITE_SSE_URL
      ? `ws://${import.meta.env.VITE_SSE_URL}/v1/stream/metrics`
      : `${import.meta.env.VITE_API_URL}/v1/stream/metrics`;

    let eventSource: EventSource | null = null;
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 1000;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let fallbackInterval: ReturnType<typeof setInterval> | null = null;
    let disposed = false;
    let isConnecting = false;

    const stopFallback = () => {
      if (fallbackInterval) {
        clearInterval(fallbackInterval);
        fallbackInterval = null;
      }
    };

    const startFallback = () => {
      if (fallbackInterval || disposed) {
        return;
      }
      logger.warn('Falling back to polling for metrics', {
        component: 'ApiClient',
        operation: 'subscribeToMetrics',
      });
      fallbackInterval = setInterval(() => {
        if (disposed) {
          stopFallback();
          return;
        }
        // Poll as fallback
        this.getSystemMetrics().then(callback).catch(() => callback(null));
      }, 5000);
    };

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const cleanupEventSource = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      clearReconnectTimer();
      const delay = Math.min(baseDelay * Math.pow(2, Math.max(0, reconnectAttempts - 1)), 30000);
      reconnectTimer = setTimeout(() => {
        if (disposed) return; // Check disposed before reconnecting
        reconnectTimer = null;
        connect();
      }, delay);
    };

    const onMetrics = (event: MessageEvent) => {
      if (disposed) return;
      try {
        const data: SystemMetrics = JSON.parse(event.data);
        callback(data);
        reconnectAttempts = 0; // Reset on success
        stopFallback();
      } catch (error) {
        logger.error('Failed to parse metrics SSE payload', {
          component: 'ApiClient',
          operation: 'subscribeToMetrics',
        }, toError(error));
        callback(null);
      }
    };

    const connect = () => {
      if (disposed || isConnecting) return;
      isConnecting = true;

      try {
        cleanupEventSource();
        stopFallback();

        try {
          eventSource = new EventSource(sseUrl);
        } catch (error) {
          logger.error('Failed to initialise metrics SSE', {
            component: 'ApiClient',
            operation: 'subscribeToMetrics',
          }, toError(error));
          callback(null);
          reconnectAttempts++;
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
          return;
        }

        eventSource.addEventListener('metrics', onMetrics);

        eventSource.addEventListener('open', () => {
          if (disposed) return;
          logger.info('Metrics SSE connected', {
            component: 'ApiClient',
            operation: 'subscribeToMetrics',
          });
          reconnectAttempts = 0;
          stopFallback();
        });

        eventSource.addEventListener('error', () => {
          if (disposed) return;
          callback(null);
          reconnectAttempts++;
          logger.warn('Metrics SSE error detected', {
            component: 'ApiClient',
            operation: 'subscribeToMetrics',
            reconnectAttempts,
          });
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
        });
      } finally {
        isConnecting = false;
      }
    };

    connect();

    return () => {
      disposed = true;
      stopFallback();
      clearReconnectTimer();
      cleanupEventSource();
    };
  }

  // Notification SSE subscription
  subscribeToNotifications(callback: (notifications: { notifications: types.Notification[]; count: number; timestamp: string } | null) => void): () => void {
    const sseUrl = import.meta.env.VITE_SSE_URL
      ? `http://${import.meta.env.VITE_SSE_URL}/v1/stream/notifications`
      : `${this.baseUrl}/v1/stream/notifications`;

    let eventSource: EventSource | null = null;
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 1000;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let fallbackInterval: ReturnType<typeof setInterval> | null = null;
    let disposed = false;
    let isConnecting = false;

    const stopFallback = () => {
      if (fallbackInterval) {
        clearInterval(fallbackInterval);
        fallbackInterval = null;
      }
    };

    const startFallback = () => {
      if (fallbackInterval) {
        return;
      }
      logger.info('Notifications: using polling fallback (SSE unavailable after max retries)', {
        component: 'ApiClient',
        operation: 'subscribeToNotifications',
        url: sseUrl,
        maxReconnect,
        pollIntervalMs: 5000,
      });
      fallbackInterval = setInterval(async () => {
        try {
          const summary = await this.getNotificationSummary().catch(() => null);
          if (!summary) {
            callback(null);
            return;
          }
          const notifications = await this.listNotifications({ unread_only: true }).catch(() => null);
          if (!notifications) {
            callback(null);
            return;
          }
          callback({
            notifications,
            count: summary.unread_count,
            timestamp: new Date().toISOString(),
          });
        } catch (error) {
          logger.error('Fallback polling for notifications failed', {
            component: 'ApiClient',
            operation: 'subscribeToNotifications',
          }, toError(error));
          callback(null);
        }
      }, 5000);
    };

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const cleanupEventSource = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      clearReconnectTimer();
      const delay = Math.min(baseDelay * Math.pow(2, Math.max(0, reconnectAttempts - 1)), 30000);
      reconnectTimer = setTimeout(() => {
        if (disposed) return; // Check disposed before reconnecting
        reconnectTimer = null;
        // Properly handle async reconnection errors
        connect().catch((error) => {
          logger.error('Reconnection failed', {
            component: 'ApiClient',
            operation: 'SSE reconnection',
            error: error instanceof Error ? error.message : String(error),
          });
        });
      }, delay);
    };

    const onNotifications = (event: MessageEvent) => {
      try {
        const data: { notifications: types.Notification[]; count: number; timestamp: string } = JSON.parse(event.data);
        callback(data);
        reconnectAttempts = 0;
        stopFallback();
      } catch (error) {
        logger.error('Failed to parse notifications SSE payload', {
          component: 'ApiClient',
          operation: 'subscribeToNotifications',
        }, toError(error));
        callback(null);
      }
    };

    const connect = async () => {
      if (disposed || isConnecting) return;
      isConnecting = true;

      try {
        cleanupEventSource();
        stopFallback();

        // Pre-flight check to detect common errors with actionable messages
        try {
          const checkResponse = await fetch(sseUrl, {
            method: 'HEAD',
            credentials: 'include',
          });
          if (!checkResponse.ok) {
            const statusMessages: Record<number, string> = {
              401: 'Authentication required - please log in',
              403: 'Permission denied - NotificationView permission required',
              404: 'Notifications stream endpoint not implemented on server',
              500: 'Server error - check server logs',
              502: 'Backend unavailable - server may be starting up',
              503: 'Service unavailable - server overloaded or in maintenance',
            };
            const message = statusMessages[checkResponse.status] || `HTTP ${checkResponse.status}`;

            // For 404, immediately fall back to polling - endpoint doesn't exist
            if (checkResponse.status === 404) {
              logger.info('Notifications SSE endpoint not available, using polling fallback', {
                component: 'ApiClient',
                operation: 'subscribeToNotifications',
                url: sseUrl,
              });
              callback(null);
              startFallback();
              return;
            }

            logger.warn(`Notifications SSE unavailable: ${message}`, {
              component: 'ApiClient',
              operation: 'subscribeToNotifications',
              url: sseUrl,
              status: checkResponse.status,
              reconnectAttempts,
            });
            callback(null);
            reconnectAttempts++;
            if (reconnectAttempts >= maxReconnect) {
              startFallback();
            }
            scheduleReconnect();
            return;
          }
        } catch (prefetchError) {
          // Network error during pre-flight - server likely down
          logger.warn('Notifications SSE: server unreachable', {
            component: 'ApiClient',
            operation: 'subscribeToNotifications',
            url: sseUrl,
            error: prefetchError instanceof Error ? prefetchError.message : String(prefetchError),
            reconnectAttempts,
          });
          callback(null);
          reconnectAttempts++;
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
          return;
        }

        try {
          // EventSource doesn't support withCredentials option
          // Cookies are sent automatically if they're httpOnly and origin matches
          eventSource = new EventSource(sseUrl);
        } catch (error) {
          logger.error('Failed to initialise notifications SSE', {
            component: 'ApiClient',
            operation: 'subscribeToNotifications',
            url: sseUrl,
          }, toError(error));
          callback(null);
          reconnectAttempts++;
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
          return;
        }

        eventSource.addEventListener('notifications', onNotifications);

        eventSource.addEventListener('open', () => {
          logger.info('Notifications SSE connected', {
            component: 'ApiClient',
            operation: 'subscribeToNotifications',
            url: sseUrl,
          });
          reconnectAttempts = 0;
          stopFallback();
        });

        eventSource.addEventListener('error', () => {
          callback(null);
          reconnectAttempts++;
          // EventSource error events don't provide details, but we can check readyState
          const readyStateMap: Record<number, string> = {
            0: 'connecting',
            1: 'open',
            2: 'closed',
          };
          const state = eventSource?.readyState ?? -1;
          logger.warn('Notifications SSE connection lost', {
            component: 'ApiClient',
            operation: 'subscribeToNotifications',
            url: sseUrl,
            readyState: readyStateMap[state] || `unknown(${state})`,
            reconnectAttempts,
            maxReconnect,
            willRetry: reconnectAttempts < maxReconnect,
          });
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
        });
      } finally {
        isConnecting = false;
      }
    };

    // Start initial connection with proper error handling
    connect().catch((error) => {
      logger.error('Initial connection failed', {
        component: 'ApiClient',
        operation: 'subscribeToNotifications',
        error: error instanceof Error ? error.message : String(error),
      });
      // Start fallback if initial connection fails
      startFallback();
    });

    return () => {
      disposed = true;
      stopFallback();
      clearReconnectTimer();
      cleanupEventSource();
    };
  }

  // Messages SSE subscription for workspace
  subscribeToMessages(workspaceId: string, callback: (messages: { messages: types.Message[]; count: number; timestamp: string } | null) => void): () => void {
    // Similar SSE pattern to notifications
    const sseUrl = import.meta.env.VITE_SSE_URL
      ? `http://${import.meta.env.VITE_SSE_URL}/v1/stream/messages/${workspaceId}`
      : `${this.baseUrl}/v1/stream/messages/${workspaceId}`;

    let eventSource: EventSource | null = null;
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 1000;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let fallbackInterval: ReturnType<typeof setInterval> | null = null;
    let disposed = false;
    let isConnecting = false;

    const stopFallback = () => {
      if (fallbackInterval) {
        clearInterval(fallbackInterval);
        fallbackInterval = null;
      }
    };

    const startFallback = () => {
      if (fallbackInterval || disposed) {
        return;
      }
      logger.warn('Falling back to polling for messages', {
        component: 'ApiClient',
        operation: 'subscribeToMessages',
        workspaceId,
      });
      fallbackInterval = setInterval(() => {
        if (disposed) {
          stopFallback();
          return;
        }
        // Poll as fallback
        this.listWorkspaceMessages(workspaceId).then(messages => {
          if (!disposed) {
            callback({ messages, count: messages.length, timestamp: new Date().toISOString() });
          }
        }).catch(() => {
          if (!disposed) {
            callback(null);
          }
        });
      }, 5000);
    };

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const cleanupEventSource = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      clearReconnectTimer();
      const delay = Math.min(baseDelay * Math.pow(2, Math.max(0, reconnectAttempts - 1)), 30000);
      reconnectTimer = setTimeout(() => {
        if (disposed) return; // Check disposed before reconnecting
        reconnectTimer = null;
        // Properly handle async reconnection errors
        connect().catch((error) => {
          logger.error('Reconnection failed', {
            component: 'ApiClient',
            operation: 'SSE reconnection',
            error: error instanceof Error ? error.message : String(error),
          });
        });
      }, delay);
    };

    const onMessages = (event: MessageEvent) => {
      if (disposed) return;
      try {
        const data: { messages: types.Message[]; count: number; timestamp: string } = JSON.parse(event.data);
        callback(data);
        reconnectAttempts = 0; // Reset on success
        stopFallback();
      } catch (error) {
        logger.error('Failed to parse messages SSE payload', {
          component: 'ApiClient',
          operation: 'subscribeToMessages',
          workspaceId,
        }, toError(error));
        callback(null);
      }
    };

    const connect = async () => {
      if (disposed || isConnecting) return;
      isConnecting = true;

      try {
        cleanupEventSource();
        stopFallback();

        // Pre-flight check - immediately fall back for 404 (endpoint not implemented)
        try {
          const checkResponse = await fetch(sseUrl, { method: 'HEAD', credentials: 'include' });
          if (checkResponse.status === 404) {
            logger.info('Messages SSE endpoint not available, using polling fallback', {
              component: 'ApiClient',
              operation: 'subscribeToMessages',
              workspaceId,
            });
            callback(null);
            startFallback();
            return;
          }
        } catch {
          // Network error - continue to try EventSource
        }

        try {
          eventSource = new EventSource(sseUrl);
        } catch (error) {
          logger.error('Failed to initialise messages SSE', {
            component: 'ApiClient',
            operation: 'subscribeToMessages',
            workspaceId,
          }, toError(error));
          callback(null);
          reconnectAttempts++;
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
          return;
        }

        eventSource.addEventListener('messages', onMessages);

        eventSource.addEventListener('open', () => {
          if (disposed) return;
          logger.info('Messages SSE connected', {
            component: 'ApiClient',
            operation: 'subscribeToMessages',
            workspaceId,
          });
          reconnectAttempts = 0;
          stopFallback();
        });

        eventSource.addEventListener('error', () => {
          if (disposed) return;
          callback(null);
          reconnectAttempts++;
          logger.warn('Messages SSE error detected', {
            component: 'ApiClient',
            operation: 'subscribeToMessages',
            workspaceId,
            reconnectAttempts,
          });
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
        });
      } finally {
        isConnecting = false;
      }
    };

    // Start initial connection with proper error handling
    connect().catch((error) => {
      logger.error('Initial SSE connection failed', {
        component: 'ApiClient',
        error: error instanceof Error ? error.message : String(error),
      });
    });

    return () => {
      disposed = true;
      stopFallback();
      clearReconnectTimer();
      cleanupEventSource();
    };
  }

  // Activity SSE subscription for workspace
  subscribeToActivity(workspaceId: string, callback: (events: { events: types.ActivityEvent[]; count: number; timestamp: string } | null) => void): () => void {
    // Similar SSE pattern to notifications
    const sseUrl = import.meta.env.VITE_SSE_URL
      ? `http://${import.meta.env.VITE_SSE_URL}/v1/stream/activity/${workspaceId}`
      : `${this.baseUrl}/v1/stream/activity/${workspaceId}`;

    let eventSource: EventSource | null = null;
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 1000;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let fallbackInterval: ReturnType<typeof setInterval> | null = null;
    let disposed = false;
    let isConnecting = false;

    const stopFallback = () => {
      if (fallbackInterval) {
        clearInterval(fallbackInterval);
        fallbackInterval = null;
      }
    };

    const startFallback = () => {
      if (fallbackInterval || disposed) {
        return;
      }
      logger.warn('Falling back to polling for activity', {
        component: 'ApiClient',
        operation: 'subscribeToActivity',
        workspaceId,
      });
      fallbackInterval = setInterval(() => {
        if (disposed) {
          stopFallback();
          return;
        }
        // Poll as fallback
        this.listActivityEvents({ workspace_id: workspaceId }).then(events => {
          if (!disposed) {
            callback({ events, count: events.length, timestamp: new Date().toISOString() });
          }
        }).catch(() => {
          if (!disposed) {
            callback(null);
          }
        });
      }, 5000);
    };

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const cleanupEventSource = () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      clearReconnectTimer();
      const delay = Math.min(baseDelay * Math.pow(2, Math.max(0, reconnectAttempts - 1)), 30000);
      reconnectTimer = setTimeout(() => {
        if (disposed) return; // Check disposed before reconnecting
        reconnectTimer = null;
        // Properly handle async reconnection errors
        connect().catch((error) => {
          logger.error('Reconnection failed', {
            component: 'ApiClient',
            operation: 'SSE reconnection',
            error: error instanceof Error ? error.message : String(error),
          });
        });
      }, delay);
    };

    const onActivity = (event: MessageEvent) => {
      if (disposed) return;
      try {
        const data: { events: types.ActivityEvent[]; count: number; timestamp: string } = JSON.parse(event.data);
        callback(data);
        reconnectAttempts = 0; // Reset on success
        stopFallback();
      } catch (error) {
        logger.error('Failed to parse activity SSE payload', {
          component: 'ApiClient',
          operation: 'subscribeToActivity',
          workspaceId,
        }, toError(error));
        callback(null);
      }
    };

    const connect = async () => {
      if (disposed || isConnecting) return;
      isConnecting = true;

      try {
        cleanupEventSource();
        stopFallback();

        // Pre-flight check - immediately fall back for 404 (endpoint not implemented)
        try {
          const checkResponse = await fetch(sseUrl, { method: 'HEAD', credentials: 'include' });
          if (checkResponse.status === 404) {
            logger.info('Activity SSE endpoint not available, using polling fallback', {
              component: 'ApiClient',
              operation: 'subscribeToActivity',
              workspaceId,
            });
            callback(null);
            startFallback();
            return;
          }
        } catch {
          // Network error - continue to try EventSource
        }

        try {
          eventSource = new EventSource(sseUrl);
        } catch (error) {
          logger.error('Failed to initialise activity SSE', {
            component: 'ApiClient',
            operation: 'subscribeToActivity',
            workspaceId,
          }, toError(error));
          callback(null);
          reconnectAttempts++;
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
          return;
        }

        eventSource.addEventListener('activity', onActivity);

        eventSource.addEventListener('open', () => {
          if (disposed) return;
          logger.info('Activity SSE connected', {
            component: 'ApiClient',
            operation: 'subscribeToActivity',
            workspaceId,
          });
          reconnectAttempts = 0;
          stopFallback();
        });

        eventSource.addEventListener('error', () => {
          if (disposed) return;
          callback(null);
          reconnectAttempts++;
          logger.warn('Activity SSE error detected', {
            component: 'ApiClient',
            operation: 'subscribeToActivity',
            workspaceId,
            reconnectAttempts,
          });
          if (reconnectAttempts >= maxReconnect) {
            startFallback();
          }
          scheduleReconnect();
        });
      } finally {
        isConnecting = false;
      }
    };

    // Start initial connection with proper error handling
    connect().catch((error) => {
      logger.error('Initial SSE connection failed', {
        component: 'ApiClient',
        error: error instanceof Error ? error.message : String(error),
      });
    });

    return () => {
      disposed = true;
      stopFallback();
      clearReconnectTimer();
      cleanupEventSource();
    };
  }

  /**
   * Get current system status including service information
   * Citation: crates/adapteros-server/src/status_writer.rs L135-144
   */
  async getStatus(): Promise<types.AdapterOSStatus> {
    return this.request<types.AdapterOSStatus>('/v1/status', {
      method: 'GET',
    });
  }

  // Service Control Methods

  /**
   * Start a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async startService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Starting service', {
      component: 'ApiClient',
      operation: 'startService',
      serviceId,
    });

    return this.request(`/v1/services/${serviceId}/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for start operations
  }

  /**
   * Stop a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async stopService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Stopping service', {
      component: 'ApiClient',
      operation: 'stopService',
      serviceId,
    });

    return this.request(`/v1/services/${serviceId}/stop`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for stop operations
  }

  /**
   * Restart a service
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async restartService(serviceId: string): Promise<{ success: boolean; message: string }> {
    logger.info('Restarting service', {
      component: 'ApiClient',
      operation: 'restartService',
      serviceId,
    });

    return this.request(`/v1/services/${serviceId}/restart`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for restart operations
  }

  /**
   * Start all essential services
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async startEssentialServices(): Promise<{ success: boolean; message: string }> {
    logger.info('Starting all essential services', {
      component: 'ApiClient',
      operation: 'startEssentialServices',
    });

    return this.request('/v1/services/essential/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for start operations
  }

  /**
   * Stop all essential services
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async stopEssentialServices(): Promise<{ success: boolean; message: string }> {
    logger.info('Stopping all essential services', {
      component: 'ApiClient',
      operation: 'stopEssentialServices',
    });

    return this.request('/v1/services/essential/stop', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    }, false, undefined, false); // No retry for stop operations
  }

  /**
   * Get service logs
   * Citation: crates/adapteros-server-api/src/handlers/services.rs
   */
  async getServiceLogs(serviceId: string, lines: number = 100): Promise<string[]> {
    logger.info('Fetching service logs', {
      component: 'ApiClient',
      operation: 'getServiceLogs',
      serviceId,
      lines,
    });

    return this.requestList<string>(`/v1/services/${serviceId}/logs?lines=${lines}`, {
      method: 'GET',
    });
  }

  // Dashboard Configuration API Methods
  async getDashboardConfig(): Promise<types.DashboardConfig> {
    return this.request<types.DashboardConfig>('/v1/dashboard/config');
  }

  async updateDashboardConfig(config: types.UpdateDashboardConfigRequest): Promise<types.UpdateDashboardConfigResponse> {
    return this.request<types.UpdateDashboardConfigResponse>('/v1/dashboard/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
  }

  async resetDashboardConfig(): Promise<types.ResetDashboardConfigResponse> {
    return this.request<types.ResetDashboardConfigResponse>('/v1/dashboard/config', {
      method: 'DELETE',
    });
  }

  // ============================================================================
  // Prompt Orchestration API Methods
  // ============================================================================

  /**
   * Get orchestration configuration
   *
   * Retrieves the current prompt orchestration configuration including routing
   * strategy, adapter settings, and custom rules.
   *
   * @returns OrchestrationConfig or null if endpoint not available
   */
  async getOrchestrationConfig(): Promise<types.OrchestrationConfig | null> {
    logger.info('Fetching orchestration config', {
      component: 'ApiClient',
      operation: 'getOrchestrationConfig',
    });

    try {
      return await this.request<types.OrchestrationConfig>('/v1/orchestration/config');
    } catch (error) {
      // Gracefully handle 404 - endpoint may not be implemented yet
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.info('Orchestration config endpoint not available', {
          component: 'ApiClient',
          operation: 'getOrchestrationConfig',
        });
        return null;
      }
      throw error;
    }
  }

  /**
   * Save orchestration configuration
   *
   * Updates the prompt orchestration configuration with new settings.
   *
   * @param config - New orchestration configuration
   * @returns Updated configuration
   * @throws Error if endpoint not available or validation fails
   */
  async saveOrchestrationConfig(config: types.OrchestrationConfig): Promise<types.OrchestrationConfig> {
    logger.info('Saving orchestration config', {
      component: 'ApiClient',
      operation: 'saveOrchestrationConfig',
      routing_strategy: config.routing_strategy,
      enabled: config.enabled,
    });

    try {
      return await this.request<types.OrchestrationConfig>('/v1/orchestration/config', {
        method: 'PUT',
        body: JSON.stringify(config),
      });
    } catch (error) {
      // Provide friendly error for 404
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.warn('Orchestration config endpoint not available for saving', {
          component: 'ApiClient',
          operation: 'saveOrchestrationConfig',
        });
        throw new Error('Orchestration configuration endpoint is not available. The backend may not support this feature yet.');
      }
      throw error;
    }
  }

  /**
   * Analyze a prompt for routing recommendations
   *
   * Sends a prompt to the orchestration service for analysis, returning
   * intent detection, complexity scoring, and adapter recommendations.
   *
   * @param prompt - The prompt text to analyze
   * @returns PromptAnalysis with recommendations
   * @throws Error if endpoint not available or analysis fails
   */
  async analyzePrompt(prompt: string): Promise<types.PromptAnalysis> {
    logger.info('Analyzing prompt', {
      component: 'ApiClient',
      operation: 'analyzePrompt',
      promptLength: prompt.length,
    });

    try {
      return await this.request<types.PromptAnalysis>('/v1/orchestration/analyze', {
        method: 'POST',
        body: JSON.stringify({ prompt }),
      });
    } catch (error) {
      // Provide friendly error for 404
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.warn('Orchestration analyze endpoint not available', {
          component: 'ApiClient',
          operation: 'analyzePrompt',
        });
        throw new Error('Prompt analysis endpoint is not available. The backend may not support this feature yet.');
      }
      throw error;
    }
  }

  /**
   * Get orchestration metrics
   *
   * Retrieves metrics about orchestration performance including request counts,
   * latency percentiles, cache hit rates, and adapter usage statistics.
   *
   * @returns OrchestrationMetrics or null if endpoint not available
   */
  async getOrchestrationMetrics(): Promise<types.OrchestrationMetrics | null> {
    logger.info('Fetching orchestration metrics', {
      component: 'ApiClient',
      operation: 'getOrchestrationMetrics',
    });

    try {
      return await this.request<types.OrchestrationMetrics>('/v1/orchestration/metrics');
    } catch (error) {
      // Gracefully handle 404 - endpoint may not be implemented yet
      if (error instanceof Error && (error.message.includes('404') || error.message.includes('Not Found'))) {
        logger.info('Orchestration metrics endpoint not available', {
          component: 'ApiClient',
          operation: 'getOrchestrationMetrics',
        });
        return null;
      }
      throw error;
    }
  }

  /**
   * Generic GET request method
   *
   * Provides a simple interface for GET requests without wrapping in request method.
   * Useful for simple data fetching operations.
   *
   * @param path - API endpoint path
   * @returns Parsed JSON response
   * @throws Error if response is not ok
   */
  async get<T>(path: string): Promise<T> {
    logger.info('GET request', {
      component: 'ApiClient',
      operation: 'get',
      path,
    });

    const response = await fetch(`${this.baseUrl}${path}`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...(this.token ? { 'Authorization': `Bearer ${this.token}` } : {}),
      },
    });

    if (!response.ok) {
      const errorText = await response.text();
      logger.error('GET request failed', {
        component: 'ApiClient',
        operation: 'get',
        path,
        status: response.status,
        statusText: response.statusText,
      });
      throw new Error(errorText || `HTTP ${response.status}: ${response.statusText}`);
    }

    return response.json();
  }

  /**
   * Wait for service to become healthy
   *
   * Polls the /healthz endpoint until the service reports a healthy status.
   * Useful for initialization and startup verification.
   *
   * @param timeout - Maximum time to wait in milliseconds (default: 30000)
   * @returns true if service became healthy, false if timeout reached
   */
  async waitForHealthy(timeout: number = 30000): Promise<boolean> {
    logger.info('Waiting for service health', {
      component: 'ApiClient',
      operation: 'waitForHealthy',
      timeout,
    });

    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      try {
        const health = await this.get<types.HealthResponse>('/healthz');
        if (health.status === 'healthy') {
          logger.info('Service became healthy', {
            component: 'ApiClient',
            operation: 'waitForHealthy',
            elapsedMs: Date.now() - startTime,
          });
          return true;
        }
      } catch (e) {
        logger.debug('Service not yet healthy, retrying', {
          component: 'ApiClient',
          operation: 'waitForHealthy',
          error: toError(e).message,
        });
        // Continue waiting
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }

    logger.warn('Service health check timed out', {
      component: 'ApiClient',
      operation: 'waitForHealthy',
      timeout,
    });

    return false;
  }

  // ============================================================================
  // Plugin Management API
  // ============================================================================

  /**
   * List all installed plugins
   *
   * Retrieves all plugins registered in the system with their current status.
   *
   * @returns List of plugins with counts
   */
  async listPlugins(): Promise<pluginTypes.ListPluginsResponse> {
    logger.info('Listing plugins', {
      component: 'ApiClient',
      operation: 'listPlugins',
    });
    return this.request<pluginTypes.ListPluginsResponse>('/v1/plugins');
  }

  /**
   * Get plugin details and status
   *
   * Retrieves detailed information about a specific plugin including
   * its current status, enabled tenants, and any error state.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin details with status information
   */
  async getPlugin(pluginId: string): Promise<pluginTypes.PluginStatusResponse> {
    logger.info('Getting plugin details', {
      component: 'ApiClient',
      operation: 'getPlugin',
      pluginId,
    });
    return this.request<pluginTypes.PluginStatusResponse>(`/v1/plugins/${encodeURIComponent(pluginId)}`);
  }

  /**
   * Get plugin status (alias for getPlugin)
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin status information
   */
  async getPluginStatus(pluginId: string): Promise<pluginTypes.PluginStatusResponse> {
    return this.getPlugin(pluginId);
  }

  /**
   * Enable a plugin
   *
   * Activates a plugin for the specified tenants or globally.
   * Requires appropriate permissions (typically Admin or Operator role).
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param options - Optional enable configuration (tenant_ids, reason, config)
   * @returns Enable operation result
   */
  async enablePlugin(
    pluginId: string,
    options?: pluginTypes.EnablePluginRequest
  ): Promise<pluginTypes.EnablePluginResponse> {
    logger.info('Enabling plugin', {
      component: 'ApiClient',
      operation: 'enablePlugin',
      pluginId,
      tenantIds: options?.tenant_ids,
    });
    return this.request<pluginTypes.EnablePluginResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/enable`,
      {
        method: 'POST',
        body: JSON.stringify(options || {}),
      }
    );
  }

  /**
   * Disable a plugin
   *
   * Deactivates a plugin for the specified tenants or globally.
   * Requires appropriate permissions (typically Admin or Operator role).
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param options - Optional disable configuration (tenant_ids, reason, force)
   * @returns Disable operation result with any warnings
   */
  async disablePlugin(
    pluginId: string,
    options?: pluginTypes.DisablePluginRequest
  ): Promise<pluginTypes.DisablePluginResponse> {
    logger.info('Disabling plugin', {
      component: 'ApiClient',
      operation: 'disablePlugin',
      pluginId,
      tenantIds: options?.tenant_ids,
      force: options?.force,
    });
    return this.request<pluginTypes.DisablePluginResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/disable`,
      {
        method: 'POST',
        body: JSON.stringify(options || {}),
      }
    );
  }

  /**
   * Get plugin configuration
   *
   * Retrieves the configuration for a specific plugin from the database.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @returns Plugin configuration or null if not configured
   */
  async getPluginConfig(pluginId: string): Promise<pluginTypes.GetPluginConfigResponse> {
    logger.info('Getting plugin configuration', {
      component: 'ApiClient',
      operation: 'getPluginConfig',
      pluginId,
    });
    return this.request<pluginTypes.GetPluginConfigResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/config`
    );
  }

  /**
   * Update plugin configuration
   *
   * Updates the configuration JSON and/or enabled status for a plugin.
   *
   * @param pluginId - Unique plugin identifier (name)
   * @param config - Configuration update request
   * @returns Updated plugin configuration
   */
  async updatePluginConfig(
    pluginId: string,
    config: pluginTypes.UpdatePluginConfigRequest
  ): Promise<pluginTypes.UpdatePluginConfigResponse> {
    logger.info('Updating plugin configuration', {
      component: 'ApiClient',
      operation: 'updatePluginConfig',
      pluginId,
      hasConfig: !!config.config_json,
      enabled: config.enabled,
    });
    return this.request<pluginTypes.UpdatePluginConfigResponse>(
      `/v1/plugins/${encodeURIComponent(pluginId)}/config`,
      {
        method: 'PUT',
        body: JSON.stringify(config),
      }
    );
  }

  // ============================================================================
  // User Management API
  // ============================================================================

  /**
   * List all users
   *
   * Retrieves all users in the system with pagination support.
   * Requires Admin role.
   *
   * @param params - Optional pagination and filter parameters
   * @returns List of users with pagination metadata
   */
  async listUsers(params?: {
    page?: number;
    page_size?: number;
    role?: authTypes.UserRole;
    tenant_id?: string;
  }): Promise<authTypes.ListUsersResponse> {
    logger.info('Listing users', {
      component: 'ApiClient',
      operation: 'listUsers',
      params,
    });
    const queryParams = new URLSearchParams();
    if (params?.page !== undefined) queryParams.append('page', String(params.page));
    if (params?.page_size !== undefined) queryParams.append('page_size', String(params.page_size));
    if (params?.role) queryParams.append('role', params.role);
    if (params?.tenant_id) queryParams.append('tenant_id', params.tenant_id);
    const query = queryParams.toString() ? `?${queryParams.toString()}` : '';
    return this.request<authTypes.ListUsersResponse>(`/v1/admin/users${query}`);
  }

  /**
   * Get user by ID
   *
   * Retrieves detailed information about a specific user.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @returns User details
   */
  async getUser(userId: string): Promise<authTypes.User> {
    logger.info('Getting user', {
      component: 'ApiClient',
      operation: 'getUser',
      userId,
    });
    return this.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`);
  }

  /**
   * Create a new user
   *
   * Registers a new user in the system.
   * Requires Admin role.
   *
   * @param data - User registration data
   * @returns Created user
   */
  async createUser(data: authTypes.RegisterUserRequest): Promise<authTypes.User> {
    logger.info('Creating user', {
      component: 'ApiClient',
      operation: 'createUser',
      email: data.email,
      role: data.role,
    });
    return this.request<authTypes.User>('/v1/admin/users', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Update an existing user
   *
   * Updates user details including role assignment.
   * Requires Admin role.
   *
   * @param userId - User ID to update
   * @param data - User update data
   * @returns Updated user
   */
  async updateUser(userId: string, data: authTypes.UpdateUserRequest): Promise<authTypes.User> {
    logger.info('Updating user', {
      component: 'ApiClient',
      operation: 'updateUser',
      userId,
      updates: data,
    });
    return this.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  /**
   * Delete a user
   *
   * Removes a user from the system.
   * Requires Admin role.
   *
   * @param userId - User ID to delete
   */
  async deleteUser(userId: string): Promise<void> {
    logger.info('Deleting user', {
      component: 'ApiClient',
      operation: 'deleteUser',
      userId,
    });
    return this.request<void>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'DELETE',
    });
  }

  /**
   * Assign role to a user
   *
   * Updates the role of an existing user.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @param role - New role to assign
   * @returns Updated user
   */
  async assignUserRole(userId: string, role: authTypes.UserRole): Promise<authTypes.User> {
    logger.info('Assigning user role', {
      component: 'ApiClient',
      operation: 'assignUserRole',
      userId,
      role,
    });
    return this.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}/role`, {
      method: 'PUT',
      body: JSON.stringify({ role }),
    });
  }

  /**
   * Reset user password (Admin)
   *
   * Sends a password reset email to the user.
   * Requires Admin role.
   *
   * @param userId - User ID
   */
  async resetUserPassword(userId: string): Promise<void> {
    logger.info('Resetting user password', {
      component: 'ApiClient',
      operation: 'resetUserPassword',
      userId,
    });
    return this.request<void>(`/v1/admin/users/${encodeURIComponent(userId)}/reset-password`, {
      method: 'POST',
    });
  }

  /**
   * Activate or deactivate a user
   *
   * Enables or disables user access to the system.
   * Requires Admin role.
   *
   * @param userId - User ID
   * @param isActive - Whether the user should be active
   * @returns Updated user
   */
  async setUserActive(userId: string, isActive: boolean): Promise<authTypes.User> {
    logger.info('Setting user active status', {
      component: 'ApiClient',
      operation: 'setUserActive',
      userId,
      isActive,
    });
    return this.request<authTypes.User>(`/v1/admin/users/${encodeURIComponent(userId)}`, {
      method: 'PUT',
      body: JSON.stringify({ is_active: isActive }),
    });
  }

  // Chat Sessions API methods
  // 【2025-11-25†prd-ux-01†chat_api_client】

  /**
   * Create a new chat session
   *
   * POST /v1/chat/sessions
   *
   * @param req - Session creation request
   * @returns Created session response
   */
  async createChatSession(req: chatTypes.CreateChatSessionRequest): Promise<chatTypes.CreateChatSessionResponse> {
    logger.info('Creating chat session', {
      component: 'ApiClient',
      operation: 'createChatSession',
      name: req.name,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      source_type: req.source_type,
    });

    // Build metadata, including document context when provided (backend stores in metadata_json)
    const metadata = {
      ...(req.metadata || {}),
      ...(req.source_type ? { source_type: req.source_type } : {}),
      ...(req.document_id ? { documentId: req.document_id } : {}),
      ...(req.document_name ? { documentName: req.document_name } : {}),
    };

    // Convert metadata object to JSON string if present
    const payload = {
      name: req.name,
      title: req.title,
      tenant_id: req.tenant_id,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      document_name: req.document_name,
      source_type: req.source_type,
      source_ref_id: req.source_ref_id,
      metadata_json: Object.keys(metadata).length > 0 ? JSON.stringify(metadata) : undefined,
      tags_json: req.tags_json,
    };

    return this.request<chatTypes.CreateChatSessionResponse>('/v1/chat/sessions', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  /**
   * Update an existing chat session
   *
   * PUT /v1/chat/sessions/:session_id
   */
  async updateChatSession(
    sessionId: string,
    req: chatTypes.UpdateChatSessionRequest
  ): Promise<chatTypes.ChatSession> {
    logger.info('Updating chat session', {
      component: 'ApiClient',
      operation: 'updateChatSession',
      sessionId,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      source_type: req.source_type,
    });

    return this.request<chatTypes.ChatSession>(`/v1/chat/sessions/${encodeURIComponent(sessionId)}`, {
      method: 'PUT',
      body: JSON.stringify(req),
    });
  }

  /**
   * List chat sessions for current user/tenant
   *
   * GET /v1/chat/sessions
   *
   * @param query - Optional filters
   * @returns Array of chat sessions
   */
  async listChatSessions(query?: chatTypes.ListSessionsQuery): Promise<chatTypes.ChatSession[]> {
    const params = new URLSearchParams();
    if (query?.user_id) params.append('user_id', query.user_id);
    if (query?.limit) params.append('limit', query.limit.toString());
    if (query?.source_type) params.append('source_type', query.source_type);
    if (query?.document_id) params.append('document_id', query.document_id);

    const queryString = params.toString();
    return this.requestList<chatTypes.ChatSession>(`/v1/chat/sessions${queryString ? `?${queryString}` : ''}`);
  }

  /**
   * Get a specific chat session
   *
   * GET /v1/chat/sessions/:session_id
   *
   * @param sessionId - Session ID
   * @returns Chat session
   */
  async getChatSession(sessionId: string): Promise<chatTypes.ChatSession> {
    return this.request<chatTypes.ChatSession>(`/v1/chat/sessions/${encodeURIComponent(sessionId)}`);
  }

  /**
   * Delete a chat session
   *
   * DELETE /v1/chat/sessions/:session_id
   *
   * @param sessionId - Session ID
   */
  async deleteChatSession(sessionId: string): Promise<void> {
    logger.info('Deleting chat session', {
      component: 'ApiClient',
      operation: 'deleteChatSession',
      sessionId,
    });
    return this.request<void>(`/v1/chat/sessions/${encodeURIComponent(sessionId)}`, {
      method: 'DELETE',
    });
  }

  /**
   * Add a message to a chat session
   *
   * POST /v1/chat/sessions/:session_id/messages
   *
   * @param sessionId - Session ID
   * @param role - Message role (user, assistant, system)
   * @param content - Message content
   * @param metadata - Optional metadata
   * @returns Created message
   */
  async addChatMessage(
    sessionId: string,
    role: 'user' | 'assistant' | 'system',
    content: string,
    metadata?: Record<string, unknown>
  ): Promise<chatTypes.ChatMessage> {
    const payload: chatTypes.AddChatMessageRequest = {
      role,
      content,
      metadata,
    };

    // Convert metadata object to JSON string if present
    const requestBody = {
      role,
      content,
      metadata_json: metadata ? JSON.stringify(metadata) : undefined,
    };

    return this.request<chatTypes.ChatMessage>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
      {
        method: 'POST',
        body: JSON.stringify(requestBody),
      }
    );
  }

  /**
   * Get messages for a chat session
   *
   * GET /v1/chat/sessions/:session_id/messages
   *
   * @param sessionId - Session ID
   * @param limit - Optional limit on number of messages
   * @returns Array of chat messages
   */
  async getChatMessages(sessionId: string, limit?: number): Promise<chatTypes.ChatMessage[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    return this.requestList<chatTypes.ChatMessage>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Get evidence attached to a chat message
   *
   * GET /v1/chat/messages/:message_id/evidence
   *
   * @param messageId - Message ID
   * @returns Evidence items for the message
   */
  async getMessageEvidence(messageId: string): Promise<chatTypes.ChatEvidenceItem[]> {
    return this.requestList<chatTypes.ChatEvidenceItem>(
      `/v1/chat/messages/${encodeURIComponent(messageId)}/evidence`
    );
  }

  /**
   * Get session summary with message and trace counts
   *
   * GET /v1/chat/sessions/:session_id/summary
   *
   * @param sessionId - Session ID
   * @returns Session summary
   */
  async getSessionSummary(sessionId: string): Promise<chatTypes.SessionSummary> {
    return this.request<chatTypes.SessionSummary>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/summary`
    );
  }

  /**
   * Update session collection binding
   *
   * PUT /v1/chat/sessions/:session_id/collection
   *
   * @param sessionId - Session ID
   * @param collectionId - Collection ID (or null to clear)
   */
  async updateSessionCollection(sessionId: string, collectionId: string | null): Promise<void> {
    logger.info('Updating session collection', {
      component: 'ApiClient',
      operation: 'updateSessionCollection',
      sessionId,
      collectionId,
    });

    const payload: chatTypes.UpdateSessionCollectionRequest = {
      collection_id: collectionId,
    };

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/collection`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Archive a chat session
   *
   * POST /v1/chat/sessions/:session_id/archive
   *
   * @param sessionId - Session ID
   * @param reason - Optional reason for archiving
   */
  async archiveChatSession(sessionId: string, reason?: string): Promise<void> {
    logger.info('Archiving chat session', {
      component: 'ApiClient',
      operation: 'archiveChatSession',
      sessionId,
    });

    const payload: chatTypes.ArchiveSessionRequest = {
      reason,
    };

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/archive`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Restore a deleted or archived chat session (admin-only)
   *
   * POST /v1/chat/sessions/:session_id/restore
   *
   * @param sessionId - Session ID
   */
  async restoreChatSession(sessionId: string): Promise<void> {
    logger.info('Restoring chat session', {
      component: 'ApiClient',
      operation: 'restoreChatSession',
      sessionId,
    });

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/restore`,
      {
        method: 'POST',
      }
    );
  }

  /**
   * Permanently delete a chat session (admin-only)
   *
   * DELETE /v1/chat/sessions/:session_id/hard
   *
   * @param sessionId - Session ID
   */
  async hardDeleteChatSession(sessionId: string): Promise<void> {
    logger.info('Hard deleting chat session', {
      component: 'ApiClient',
      operation: 'hardDeleteChatSession',
      sessionId,
    });

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/permanent`,
      {
        method: 'DELETE',
      }
    );
  }

  /**
   * List archived chat sessions
   *
   * GET /v1/chat/sessions/archived
   *
   * @param limit - Optional limit on number of sessions
   * @returns Array of archived chat sessions with status
   */
  async listArchivedChatSessions(limit?: number): Promise<chatTypes.ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    return this.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/archived${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * List deleted chat sessions (trash)
   *
   * GET /v1/chat/sessions/trash
   *
   * @param limit - Optional limit on number of sessions
   * @returns Array of deleted chat sessions with status
   */
  async listDeletedChatSessions(limit?: number): Promise<chatTypes.ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    return this.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/trash${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Search chat sessions using FTS (Full-Text Search)
   *
   * GET /v1/chat/sessions/search
   *
   * @param query - Search query parameters
   * @returns Array of search results with highlighted matches
   */
  async searchChatSessions(query: chatTypes.SearchSessionsQuery): Promise<chatTypes.ChatSearchResult[]> {
    logger.info('Searching chat sessions', {
      component: 'ApiClient',
      operation: 'searchChatSessions',
      query: query.q,
      scope: query.scope,
    });

    const params = new URLSearchParams();
    params.append('q', query.q);
    if (query.scope) params.append('scope', query.scope);
    if (query.category_id) params.append('category_id', query.category_id);
    if (query.tags) params.append('tags', query.tags);
    if (query.include_archived !== undefined) params.append('include_archived', query.include_archived.toString());
    if (query.limit) params.append('limit', query.limit.toString());

    return this.requestList<chatTypes.ChatSearchResult>(`/v1/chat/sessions/search?${params.toString()}`);
  }

  // ============================================================================
  // Chat Session Sharing API Methods
  // ============================================================================

  /**
   * Share a chat session with users or workspace
   *
   * POST /v1/chat/sessions/:session_id/shares
   *
   * @param sessionId - Session ID
   * @param request - Share request with user_ids, workspace_id, and permission
   * @returns Share response with created share IDs
   */
  async shareSession(
    sessionId: string,
    request: chatTypes.ShareSessionRequest
  ): Promise<chatTypes.ShareSessionResponse> {
    logger.info('Sharing chat session', {
      component: 'ApiClient',
      operation: 'shareSession',
      sessionId,
      userCount: request.user_ids?.length || 0,
      hasWorkspace: !!request.workspace_id,
      permission: request.permission,
    });

    return this.request<chatTypes.ShareSessionResponse>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    );
  }

  /**
   * Get all shares for a session
   *
   * GET /v1/chat/sessions/:session_id/shares
   *
   * @param sessionId - Session ID
   * @returns Array of session shares
   */
  async getSessionShares(sessionId: string): Promise<chatTypes.SessionShare[]> {
    return this.requestList<chatTypes.SessionShare>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares`
    );
  }

  /**
   * Get chat sessions shared with the current user
   *
   * GET /v1/chat/sessions/shared-with-me
   *
   * @param query - Optional query parameters
   * @returns Array of chat sessions shared with the current user
   */
  async getSessionsSharedWithMe(
    query?: chatTypes.ListArchivedQuery
  ): Promise<chatTypes.ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (query?.limit) params.append('limit', query.limit.toString());

    const queryString = params.toString();
    return this.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/shared-with-me${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Revoke a session share
   *
   * DELETE /v1/chat/sessions/:session_id/shares/:share_id
   *
   * @param sessionId - Session ID
   * @param shareId - Share ID to revoke
   * @param shareType - Type of share ('user' or 'workspace'), defaults to 'user'
   */
  async revokeSessionShare(
    sessionId: string,
    shareId: string,
    shareType: 'user' | 'workspace' = 'user'
  ): Promise<void> {
    logger.info('Revoking session share', {
      component: 'ApiClient',
      operation: 'revokeSessionShare',
      sessionId,
      shareId,
      shareType,
    });

    const params = new URLSearchParams();
    params.append('type', shareType);

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares/${encodeURIComponent(shareId)}?${params.toString()}`,
      {
        method: 'DELETE',
      }
    );
  }

  // ============================================================================
  // Chat Tags API Methods
  // ============================================================================

  /**
   * List all chat tags for current tenant
   *
   * GET /v1/chat/tags
   *
   * @returns Array of chat tags
   */
  async listChatTags(): Promise<chatTypes.ChatTag[]> {
    return this.requestList<chatTypes.ChatTag>('/v1/chat/tags');
  }

  /**
   * Create a new chat tag
   *
   * POST /v1/chat/tags
   *
   * @param req - Tag creation request
   * @returns Created chat tag
   */
  async createChatTag(req: chatTypes.CreateTagRequest): Promise<chatTypes.ChatTag> {
    logger.info('Creating chat tag', {
      component: 'ApiClient',
      operation: 'createChatTag',
      name: req.name,
    });

    return this.request<chatTypes.ChatTag>('/v1/chat/tags', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  }

  /**
   * Update a chat tag
   *
   * PUT /v1/chat/tags/:tag_id
   *
   * @param tagId - Tag ID to update
   * @param req - Tag update request
   * @returns Updated chat tag
   */
  async updateChatTag(tagId: string, req: chatTypes.UpdateTagRequest): Promise<chatTypes.ChatTag> {
    logger.info('Updating chat tag', {
      component: 'ApiClient',
      operation: 'updateChatTag',
      tagId,
    });

    return this.request<chatTypes.ChatTag>(`/v1/chat/tags/${encodeURIComponent(tagId)}`, {
      method: 'PUT',
      body: JSON.stringify(req),
    });
  }

  /**
   * Delete a chat tag
   *
   * DELETE /v1/chat/tags/:tag_id
   *
   * @param tagId - Tag ID to delete
   */
  async deleteChatTag(tagId: string): Promise<void> {
    logger.info('Deleting chat tag', {
      component: 'ApiClient',
      operation: 'deleteChatTag',
      tagId,
    });

    return this.request<void>(`/v1/chat/tags/${encodeURIComponent(tagId)}`, {
      method: 'DELETE',
    });
  }

  /**
   * Assign tags to a chat session
   *
   * POST /v1/chat/sessions/:session_id/tags
   *
   * @param sessionId - Session ID
   * @param tagIds - Array of tag IDs to assign
   * @returns Array of assigned tags
   */
  async assignTagsToSession(sessionId: string, tagIds: string[]): Promise<chatTypes.ChatTag[]> {
    logger.info('Assigning tags to session', {
      component: 'ApiClient',
      operation: 'assignTagsToSession',
      sessionId,
      tagCount: tagIds.length,
    });

    const payload: chatTypes.AssignTagsRequest = {
      tag_ids: tagIds,
    };

    return this.requestList<chatTypes.ChatTag>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Get tags for a chat session
   *
   * GET /v1/chat/sessions/:session_id/tags
   *
   * @param sessionId - Session ID
   * @returns Array of tags assigned to the session
   */
  async getSessionTags(sessionId: string): Promise<chatTypes.ChatTag[]> {
    return this.requestList<chatTypes.ChatTag>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags`
    );
  }

  /**
   * Remove a tag from a chat session
   *
   * DELETE /v1/chat/sessions/:session_id/tags/:tag_id
   *
   * @param sessionId - Session ID
   * @param tagId - Tag ID to remove
   */
  async removeTagFromSession(sessionId: string, tagId: string): Promise<void> {
    logger.info('Removing tag from session', {
      component: 'ApiClient',
      operation: 'removeTagFromSession',
      sessionId,
      tagId,
    });

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags/${encodeURIComponent(tagId)}`,
      {
        method: 'DELETE',
      }
    );
  }

  // ============================================================================
  // Chat Categories API Methods
  // ============================================================================

  /**
   * List all chat categories for current tenant
   *
   * GET /v1/chat/categories
   *
   * @returns Array of chat categories (tree-sorted by path)
   */
  async listChatCategories(): Promise<chatTypes.ChatCategory[]> {
    return this.requestList<chatTypes.ChatCategory>('/v1/chat/categories');
  }

  /**
   * Create a new chat category
   *
   * POST /v1/chat/categories
   *
   * @param req - Category creation request
   * @returns Created category
   */
  async createChatCategory(req: chatTypes.CreateCategoryRequest): Promise<chatTypes.ChatCategory> {
    logger.info('Creating chat category', {
      component: 'ApiClient',
      operation: 'createChatCategory',
      name: req.name,
      parent_id: req.parent_id,
    });

    return this.request<chatTypes.ChatCategory>('/v1/chat/categories', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  }

  /**
   * Update a chat category
   *
   * PUT /v1/chat/categories/:category_id
   *
   * @param categoryId - Category ID
   * @param req - Category update request
   * @returns Updated category
   */
  async updateChatCategory(
    categoryId: string,
    req: chatTypes.UpdateCategoryRequest
  ): Promise<chatTypes.ChatCategory> {
    logger.info('Updating chat category', {
      component: 'ApiClient',
      operation: 'updateChatCategory',
      categoryId,
    });

    return this.request<chatTypes.ChatCategory>(
      `/v1/chat/categories/${encodeURIComponent(categoryId)}`,
      {
        method: 'PUT',
        body: JSON.stringify(req),
      }
    );
  }

  /**
   * Delete a chat category
   *
   * DELETE /v1/chat/categories/:category_id
   *
   * @param categoryId - Category ID
   */
  async deleteChatCategory(categoryId: string): Promise<void> {
    logger.info('Deleting chat category', {
      component: 'ApiClient',
      operation: 'deleteChatCategory',
      categoryId,
    });

    return this.request<void>(
      `/v1/chat/categories/${encodeURIComponent(categoryId)}`,
      {
        method: 'DELETE',
      }
    );
  }

  /**
   * Set the category for a chat session
   *
   * PUT /v1/chat/sessions/:session_id/category
   *
   * @param sessionId - Session ID
   * @param categoryId - Category ID (or null to clear)
   */
  async setSessionCategory(sessionId: string, categoryId: string | null): Promise<void> {
    logger.info('Setting session category', {
      component: 'ApiClient',
      operation: 'setSessionCategory',
      sessionId,
      categoryId,
    });

    const payload: chatTypes.SetCategoryRequest = {
      category_id: categoryId,
    };

    return this.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/category`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Send a message to the Owner System Chat endpoint
   *
   * POST /v1/chat/owner-system
   *
   * @param messages - Array of chat messages with role and content
   * @param context - Optional context (route, metrics_snapshot, user_role)
   * @returns Response with message, optional CLI suggestion, and relevant links
   */
  async sendOwnerChatMessage(
    messages: ownerTypes.OwnerChatMessage[],
    context?: ownerTypes.OwnerChatContext
  ): Promise<ownerTypes.OwnerChatResponse> {
    logger.info('Sending owner chat message', {
      component: 'ApiClient',
      operation: 'sendOwnerChatMessage',
      messageCount: messages.length,
      hasContext: !!context,
    });

    const request: ownerTypes.OwnerChatRequest = { messages, context };
    return this.request('/v1/chat/owner-system', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
  }

  // ============================================================================
  // Document API Methods
  // ============================================================================

  /**
   * Upload a document for RAG indexing
   *
   * POST /v1/documents (multipart/form-data)
   *
   * @param file - File to upload
   * @param name - Optional document name (defaults to filename)
   * @returns Uploaded document metadata
   */
  async uploadDocument(
    params: File | { file: File; name?: string; description?: string },
    name?: string
  ): Promise<documentTypes.Document> {
    const file = params instanceof File ? params : params.file;
    const providedName = params instanceof File ? name : params.name;
    const description = params instanceof File ? undefined : params.description;

    const formData = new FormData();
    formData.append('file', file);
    if (providedName) formData.append('name', providedName);
    if (description) formData.append('description', description);

    return this.request<documentTypes.Document>('/v1/documents/upload', {
      method: 'POST',
      body: formData,
      headers: {}, // Let browser set Content-Type for FormData
    });
  }

  /**
   * Process an uploaded document (parse, chunk, embed, index)
   *
   * POST /v1/documents/:id/process
   */
  async processDocument(
    documentId: string
  ): Promise<documentTypes.ProcessDocumentResponse> {
    return this.request<documentTypes.ProcessDocumentResponse>(
      `/v1/documents/${encodeURIComponent(documentId)}/process`,
      { method: 'POST' }
    );
  }

  /**
   * List all documents for the current tenant
   *
   * GET /v1/documents
   *
   * @returns Array of documents
   */
  async listDocuments(): Promise<documentTypes.Document[]> {
    return this.requestList<documentTypes.Document>('/v1/documents');
  }

  /**
   * Get a specific document by ID
   *
   * GET /v1/documents/:id
   *
   * @param documentId - Document ID
   * @returns Document metadata
   */
  async getDocument(documentId: string): Promise<documentTypes.Document> {
    return this.request<documentTypes.Document>(
      `/v1/documents/${encodeURIComponent(documentId)}`
    );
  }

  /**
   * Delete a document
   *
   * DELETE /v1/documents/:id
   *
   * @param documentId - Document ID
   */
  async deleteDocument(documentId: string): Promise<void> {
    await this.request<void>(
      `/v1/documents/${encodeURIComponent(documentId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * List chunks for a document
   *
   * GET /v1/documents/:id/chunks
   *
   * @param documentId - Document ID
   * @returns Array of document chunks
   */
  async listDocumentChunks(documentId: string): Promise<documentTypes.DocumentChunk[]> {
    return this.requestList<documentTypes.DocumentChunk>(
      `/v1/documents/${encodeURIComponent(documentId)}/chunks`
    );
  }

  /**
   * Download a document file
   *
   * GET /v1/documents/:id/download
   *
   * @param documentId - Document ID
   * @returns Blob of the document file
   */
  async downloadDocument(documentId: string): Promise<Blob> {
    const path = `/v1/documents/${encodeURIComponent(documentId)}/download`;
    const url = `${this.baseUrl}${path}`;
    const response = await fetch(url, {
      method: 'GET',
      credentials: 'include',
    });

    return handleBlobResponse(response, { method: 'GET', path });
  }

  // ============================================================================
  // Collection API Methods
  // ============================================================================

  /**
   * Create a new collection
   *
   * POST /v1/collections
   *
   * @param name - Collection name
   * @param description - Optional description
   * @returns Created collection
   */
  async createCollection(
    name: string,
    description?: string
  ): Promise<documentTypes.Collection> {
    const request: documentTypes.CreateCollectionRequest = { name, description };
    return this.request<documentTypes.Collection>('/v1/collections', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * List all collections for the current tenant
   *
   * GET /v1/collections
   *
   * @returns Array of collections
   */
  async listCollections(): Promise<documentTypes.Collection[]> {
    return this.requestList<documentTypes.Collection>('/v1/collections');
  }

  /**
   * Get a specific collection with documents
   *
   * GET /v1/collections/:id
   *
   * @param collectionId - Collection ID
   * @returns Collection detail with documents
   */
  async getCollection(collectionId: string): Promise<documentTypes.CollectionDetail> {
    return this.request<documentTypes.CollectionDetail>(
      `/v1/collections/${encodeURIComponent(collectionId)}`
    );
  }

  /**
   * List documents not yet in the specified collection
   *
   * GET /v1/collections/:id/available-documents
   *
   * @param collectionId - Collection ID
   * @returns Array of available documents
   */
  async listAvailableDocuments(collectionId: string): Promise<documentTypes.Document[]> {
    return this.requestList<documentTypes.Document>(
      `/v1/collections/${encodeURIComponent(collectionId)}/available-documents`
    );
  }

  /**
   * Delete a collection
   *
   * DELETE /v1/collections/:id
   *
   * @param collectionId - Collection ID
   */
  async deleteCollection(collectionId: string): Promise<void> {
    await this.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * Add a document to a collection
   *
   * POST /v1/collections/:id/documents
   *
   * @param collectionId - Collection ID
   * @param documentId - Document ID to add
   */
  async addDocumentToCollection(
    collectionId: string,
    documentId: string
  ): Promise<void> {
    const request: documentTypes.AddDocumentRequest = { document_id: documentId };
    await this.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    );
  }

  /**
   * Add multiple documents to a collection
   *
   * POST /v1/collections/:id/documents (bulk)
   */
  async addDocumentsToCollection(
    collectionId: string,
    documentIds: string[]
  ): Promise<void> {
    await this.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents`,
      {
        method: 'POST',
        body: JSON.stringify({ document_ids: documentIds }),
      }
    );
  }

  /**
   * Remove a document from a collection
   *
   * DELETE /v1/collections/:id/documents/:doc_id
   *
   * @param collectionId - Collection ID
   * @param documentId - Document ID to remove
   */
  async removeDocumentFromCollection(
    collectionId: string,
    documentId: string
  ): Promise<void> {
    await this.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents/${encodeURIComponent(documentId)}`,
      { method: 'DELETE' }
    );
  }

  // ============================================================================
  // Evidence API Methods
  // ============================================================================

  /**
   * List evidence entries with optional filters
   *
   * GET /v1/evidence
   *
   * @param query - Optional filter parameters
   * @returns Array of evidence entries
   */
  async listEvidence(query?: documentTypes.ListEvidenceQuery): Promise<documentTypes.Evidence[]> {
    const params = new URLSearchParams();
    if (query?.dataset_id) params.append('dataset_id', query.dataset_id);
    if (query?.adapter_id) params.append('adapter_id', query.adapter_id);
    if (query?.evidence_type) params.append('evidence_type', query.evidence_type);
    if (query?.confidence) params.append('confidence', query.confidence);
    if (query?.limit) params.append('limit', query.limit.toString());

    const queryString = params.toString();
    return this.requestList<documentTypes.Evidence>(
      `/v1/evidence${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Create a new evidence entry
   *
   * POST /v1/evidence
   *
   * @param request - Evidence creation request
   * @returns Created evidence entry
   */
  async createEvidence(
    request: documentTypes.CreateEvidenceRequest
  ): Promise<documentTypes.Evidence> {
    return this.request<documentTypes.Evidence>('/v1/evidence', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * Get a specific evidence entry
   *
   * GET /v1/evidence/:id
   *
   * @param evidenceId - Evidence entry ID
   * @returns Evidence entry
   */
  async getEvidence(evidenceId: string): Promise<documentTypes.Evidence> {
    return this.request<documentTypes.Evidence>(
      `/v1/evidence/${encodeURIComponent(evidenceId)}`
    );
  }

  /**
   * Delete an evidence entry
   *
   * DELETE /v1/evidence/:id
   *
   * @param evidenceId - Evidence entry ID
   */
  async deleteEvidence(evidenceId: string): Promise<void> {
    await this.request<void>(
      `/v1/evidence/${encodeURIComponent(evidenceId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * Get evidence entries for a specific dataset
   *
   * GET /v1/datasets/:dataset_id/evidence
   *
   * @param datasetId - Dataset ID
   * @returns Array of evidence entries
   */
  async getDatasetEvidence(datasetId: string): Promise<documentTypes.Evidence[]> {
    return this.requestList<documentTypes.Evidence>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/evidence`
    );
  }

  /**
   * Get evidence entries for a specific adapter
   *
   * GET /v1/adapters/:adapter_id/evidence
   *
   * @param adapterId - Adapter ID
   * @returns Array of evidence entries
   */
  async getAdapterEvidence(adapterId: string): Promise<documentTypes.Evidence[]> {
    return this.requestList<documentTypes.Evidence>(
      `/v1/adapters/${encodeURIComponent(adapterId)}/evidence`
    );
  }

  /**
   * Run an owner CLI command
   *
   * POST /v1/cli/owner-run
   *
   * @param command - CLI command to execute
   * @param sessionId - Optional session ID for command execution
   * @returns Command execution result with stdout, stderr, exit code, and duration
   */
  async runOwnerCli(command: string, sessionId?: string): Promise<ownerTypes.CliRunResponse> {
    const request: ownerTypes.CliRunRequest = { command, session_id: sessionId };
    return this.request('/v1/cli/owner-run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
  }

  // Behavior Training API

  /**
   * List behavior events with optional filtering
   *
   * GET /v1/behavior-events
   *
   * @param filters - Optional filters for events
   * @returns Array of behavior events
   */
  async getBehaviorEvents(
    filters?: adapterTypes.BehaviorEventFilters
  ): Promise<adapterTypes.BehaviorEvent[]> {
    const params = new URLSearchParams();
    if (filters) {
      Object.entries(filters).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          params.append(key, String(value));
        }
      });
    }
    const queryString = params.toString();
    return this.requestList<adapterTypes.BehaviorEvent>(
      `/v1/behavior-events${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Get behavior event statistics
   *
   * GET /v1/behavior-events/stats
   *
   * @param tenantId - Optional tenant ID filter
   * @returns Behavior statistics
   */
  async getBehaviorStats(tenantId?: string): Promise<adapterTypes.BehaviorStats> {
    const params = new URLSearchParams();
    if (tenantId) {
      params.append('tenant_id', tenantId);
    }
    const queryString = params.toString();
    return this.request<adapterTypes.BehaviorStats>(
      `/v1/behavior-events/stats${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Export behavior data to JSONL format
   *
   * POST /v1/behavior-events/export
   *
   * @param request - Export configuration
   * @returns Blob of JSONL data
   */
  async exportBehaviorData(request: adapterTypes.BehaviorExportRequest): Promise<Blob> {
    const response = await fetch(`${this.baseUrl}/v1/behavior-events/export`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.token && { Authorization: `Bearer ${this.token}` }),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      throw new Error(`Export failed: ${response.statusText}`);
    }

    return response.blob();
  }

  // ============================================================================
  // Repository API Methods
  // ============================================================================

  /**
   * List adapter repositories (system adapter repos, not code repos).
   *
   * GET /v1/adapter-repositories
   */
  async listAdapterRepositories(): Promise<repoTypes.AdapterRepositorySummary[]> {
    return this.requestList<repoTypes.AdapterRepositorySummary>('/v1/adapter-repositories');
  }

  /**
   * Get adapter repository policy (backend/coreml settings).
   *
   * GET /v1/adapter-repositories/:repoId/policy
   */
  async getAdapterRepositoryPolicy(repoId: string): Promise<repoTypes.AdapterRepositoryPolicy> {
    return this.request<repoTypes.AdapterRepositoryPolicy>(
      `/v1/adapter-repositories/${encodeURIComponent(repoId)}/policy`
    );
  }

  /**
   * Update adapter repository policy (backend/coreml settings).
   *
   * PUT /v1/adapter-repositories/:repoId/policy
   */
  async updateAdapterRepositoryPolicy(
    repoId: string,
    payload: repoTypes.UpdateAdapterRepositoryPolicyRequest
  ): Promise<repoTypes.AdapterRepositoryPolicy> {
    return this.request<repoTypes.AdapterRepositoryPolicy>(
      `/v1/adapter-repositories/${encodeURIComponent(repoId)}/policy`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * List repositories grouped by base model.
   *
   * GET /v1/repos
   */
  async listRepos(): Promise<repoTypes.RepoSummary[]> {
    return this.requestList<repoTypes.RepoSummary>('/v1/repos');
  }

  /**
   * Create a new repository.
   *
   * POST /v1/repos
   */
  async createRepo(payload: repoTypes.CreateRepoRequest): Promise<repoTypes.RepoDetail> {
    return this.request<repoTypes.RepoDetail>('/v1/repos', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  /**
   * Get repository metadata.
   *
   * GET /v1/repos/:repoId
   */
  async getRepo(repoId: string): Promise<repoTypes.RepoDetail> {
    return this.request<repoTypes.RepoDetail>(`/v1/repos/${encodeURIComponent(repoId)}`);
  }

  /**
   * Update repository metadata (description, tags, default branch, status).
   *
   * PATCH /v1/repos/:repoId
   */
  async updateRepo(
    repoId: string,
    payload: repoTypes.UpdateRepoRequest
  ): Promise<repoTypes.RepoDetail> {
    return this.request<repoTypes.RepoDetail>(
      `/v1/repos/${encodeURIComponent(repoId)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * List versions for a repository.
   *
   * GET /v1/repos/:repoId/versions
   */
  async listRepoVersions(repoId: string): Promise<repoTypes.RepoVersionSummary[]> {
    return this.requestList<repoTypes.RepoVersionSummary>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions`
    );
  }

  /**
   * Get version detail for a repository.
   *
   * GET /v1/repos/:repoId/versions/:versionId
   */
  async getRepoVersion(
    repoId: string,
    versionId: string
  ): Promise<repoTypes.RepoVersionDetail> {
    return this.request<repoTypes.RepoVersionDetail>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}`
    );
  }

  /**
   * Get combined timeline events from version history and training jobs.
   *
   * GET /v1/repos/:repoId/timeline
   */
  async getRepoTimeline(repoId: string): Promise<repoTypes.RepoTimelineEvent[]> {
    return this.requestList<repoTypes.RepoTimelineEvent>(
      `/v1/repos/${encodeURIComponent(repoId)}/timeline`
    );
  }

  /**
   * List training jobs associated with a repository.
   *
   * GET /v1/repos/:repoId/training-jobs
   */
  async listRepoTrainingJobs(repoId: string): Promise<repoTypes.RepoTrainingJobLink[]> {
    return this.requestList<repoTypes.RepoTrainingJobLink>(
      `/v1/repos/${encodeURIComponent(repoId)}/training-jobs`
    );
  }

  /**
   * Promote a version to Active (per branch).
   *
   * POST /v1/repos/:repoId/versions/:versionId/promote
   */
  async promoteRepoVersion(
    repoId: string,
    versionId: string,
    payload?: repoTypes.PromoteVersionRequest
  ): Promise<repoTypes.RepoVersionDetail> {
    return this.request<repoTypes.RepoVersionDetail>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}/promote`,
      {
        method: 'POST',
        body: JSON.stringify(payload ?? {}),
      },
      false,
      undefined,
      true
    );
  }

  /**
   * Rollback branch to a prior version.
   *
   * POST /v1/repos/:repoId/versions/:versionId/rollback
   */
  async rollbackRepoVersion(
    repoId: string,
    versionId: string,
    payload?: repoTypes.RollbackVersionRequest
  ): Promise<repoTypes.RepoVersionDetail> {
    return this.request<repoTypes.RepoVersionDetail>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}/rollback`,
      {
        method: 'POST',
        body: JSON.stringify(payload ?? {}),
      }
    );
  }

  /**
   * Tag a version.
   *
   * POST /v1/repos/:repoId/versions/:versionId/tags
   */
  async tagRepoVersion(
    repoId: string,
    versionId: string,
    payload: repoTypes.TagVersionRequest
  ): Promise<repoTypes.RepoVersionDetail> {
    return this.request<repoTypes.RepoVersionDetail>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}/tags`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Start new training based on a version.
   *
   * POST /v1/repos/:repoId/versions/:versionId/train
   */
  async startTrainingFromVersion(
    repoId: string,
    versionId: string,
    payload: repoTypes.StartTrainingFromVersionRequest
  ): Promise<repoTypes.RepoTrainingJobLink> {
    return this.request<repoTypes.RepoTrainingJobLink>(
      `/v1/repos/${encodeURIComponent(repoId)}/versions/${encodeURIComponent(versionId)}/train`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  // ============================================================================
  // Deterministic Replay API
  // ============================================================================

  /**
   * Check if replay is available for an inference
   *
   * GET /v1/replay/check/{inference_id}
   *
   * @param inferenceId - ID of the inference to check
   * @returns Replay availability information
   */
  async checkReplayAvailability(inferenceId: string): Promise<replayTypes.ReplayAvailabilityResponse> {
    return this.request<replayTypes.ReplayAvailabilityResponse>(
      `/v1/replay/check/${inferenceId}`
    );
  }

  /**
   * Execute a deterministic replay
   *
   * POST /v1/replay
   *
   * @param request - Replay execution request
   * @returns Replay execution result
   */
  async executeReplay(request: replayTypes.ReplayRequest): Promise<replayTypes.ReplayResponse> {
    return this.request<replayTypes.ReplayResponse>('/v1/replay', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * Get replay execution history for an inference
   *
   * GET /v1/replay/history/{inference_id}
   *
   * @param inferenceId - ID of the inference
   * @returns History of replay executions
   */
  async getReplayHistory(inferenceId: string): Promise<replayTypes.ReplayHistoryResponse> {
    return this.request<replayTypes.ReplayHistoryResponse>(
      `/v1/replay/history/${inferenceId}`
    );
  }
}

// Export singleton instance
export const apiClient = new ApiClient();
export default apiClient;
