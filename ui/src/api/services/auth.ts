/**
 * Authentication service - handles login, logout, sessions, and token management.
 */

import type { ApiClient, ApiError } from '@/api/client';
import * as types from '@/api/types';
import * as authTypes from '@/api/auth-types';
import { logger, toError } from '@/utils/logger';
import { LoginResponseSchema } from '@/schemas/common.schema';

export class AuthService {
  constructor(private client: ApiClient) {}

  async login(credentials: authTypes.LoginRequest): Promise<authTypes.LoginResponse> {
    const response = await this.client.request<unknown>('/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify(credentials),
    });

    // Defensive check: if we got an error response body instead of throw, handle it
    // This shouldn't happen but protects against edge cases in error handling
    if (response && typeof response === 'object' && 'code' in response && 'message' in response) {
      const errResp = response as { code: string; message: string; hint?: string };
      const error = new Error(errResp.message) as ApiError;
      error.code = errResp.code;
      if (errResp.hint) {
        error.details = { hint: errResp.hint };
      }
      throw error;
    }

    // Runtime validation of login response structure
    try {
      const validated = LoginResponseSchema.parse(response);
      validated.session_mode = validated.session_mode ?? 'normal';
      logger.info('User authentication successful', {
        component: 'AuthService',
        operation: 'login',
        user_id: validated.user_id,
        tenant_id: validated.tenant_id,
        email: credentials.email,
      });
      this.client.setToken(validated.token);
      return validated as authTypes.LoginResponse;
    } catch (validationError) {
      const error = toError(validationError);
      logger.validationError('Login response validation failed', {
        component: 'AuthService',
        operation: 'login',
        userJourney: 'login_flow',
        details: 'Server returned invalid login response structure',
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
        receivedResponse: typeof response === 'object' ? Object.keys(response as Record<string, unknown>) : String(response),
      }, ['Invalid response structure from authentication server'], error);

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
    await this.client.request('/v1/auth/logout', { method: 'POST' });
    this.client.clearToken();
  }

  async devBypass(): Promise<authTypes.LoginResponse> {
    const response = await this.client.request<unknown>('/v1/auth/dev-bypass', { method: 'POST' });

    // Defensive check: if we got an error response body instead of throw, handle it
    if (response && typeof response === 'object' && 'code' in response && 'message' in response) {
      const errResp = response as { code: string; message: string; hint?: string };
      const error = new Error(errResp.message) as ApiError;
      error.code = errResp.code;
      if (errResp.hint) {
        error.details = { hint: errResp.hint };
      }
      throw error;
    }

    try {
      const validated = LoginResponseSchema.parse(response);
      validated.session_mode = validated.session_mode ?? 'dev_bypass';
      logger.info('Dev bypass authentication successful', {
        component: 'AuthService',
        operation: 'devBypass',
        user_id: validated.user_id,
        tenant_id: validated.tenant_id,
      });
      this.client.setToken(validated.token);
      return validated as authTypes.LoginResponse;
    } catch (validationError) {
      const error = toError(validationError);
      logger.error('Dev bypass response validation failed', {
        component: 'AuthService',
        operation: 'devBypass',
        expectedFields: ['token', 'user_id', 'tenant_id', 'role', 'expires_in', 'tenants?'],
        receivedResponse: typeof response === 'object' ? Object.keys(response as Record<string, unknown>) : String(response),
      }, error);

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
    return this.client.request<authTypes.UserInfoResponse>('/v1/auth/me');
  }

  async refreshSession(): Promise<authTypes.UserInfoResponse> {
    logger.info('Refreshing auth session', {
      component: 'AuthService',
      operation: 'refreshSession',
    });
    const resp = await this.client.request<authTypes.RefreshResponse>('/v1/auth/refresh', { method: 'POST' });
    this.client.setToken(resp.token);
    return this.getCurrentUser();
  }

  async logoutAllSessions(): Promise<void> {
    logger.info('Logging out current session (logout-all fallback)', {
      component: 'AuthService',
      operation: 'logoutAllSessions',
    });
    await this.client.request('/v1/auth/logout', { method: 'POST' });
    this.client.clearToken();
  }

  async listSessions(): Promise<types.SessionInfo[]> {
    return this.client.requestList<types.SessionInfo>('/v1/auth/sessions');
  }

  async listUserTenants(): Promise<authTypes.TenantSummary[]> {
    const resp = await this.client.request<authTypes.TenantListResponse>('/v1/auth/tenants');
    return resp.tenants ?? [];
  }

  async switchTenant(tenantId: string): Promise<authTypes.SwitchTenantResponse> {
    const resp = await this.client.request<authTypes.SwitchTenantResponse>('/v1/auth/tenants/switch', {
      method: 'POST',
      body: JSON.stringify({ tenant_id: tenantId }),
    });
    if (resp?.token) {
      this.client.setToken(resp.token);
    }
    return resp;
  }

  async revokeSession(sessionId: string): Promise<void> {
    await this.client.request<void>(`/v1/auth/sessions/${sessionId}`, {
      method: 'DELETE',
    });
  }

  async rotateApiToken(): Promise<authTypes.RotateTokenResponse> {
    logger.info('Rotating API token', {
      component: 'AuthService',
      operation: 'rotateApiToken',
    });
    return this.client.request<authTypes.RotateTokenResponse>('/v1/auth/token/rotate', {
      method: 'POST',
    });
  }

  async getTokenMetadata(): Promise<types.TokenMetadata> {
    return this.client.request<types.TokenMetadata>('/v1/auth/token');
  }

  async updateUserProfile(data: types.UpdateProfileRequest): Promise<types.ProfileResponse> {
    return this.client.request<types.ProfileResponse>('/v1/auth/profile', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async getAuthConfig(cancelToken?: AbortSignal): Promise<types.AuthConfigResponse> {
    return this.client.request<types.AuthConfigResponse>('/v1/auth/config', {}, false, cancelToken);
  }

  async updateAuthConfig(data: types.UpdateAuthConfigRequest): Promise<types.AuthConfigResponse> {
    return this.client.request<types.AuthConfigResponse>('/v1/auth/config', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }
}
