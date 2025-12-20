/**
 * Base service class providing access to the core ApiClient request methods.
 * All domain services extend this class to access HTTP infrastructure.
 */

import type { ApiClient } from '@/api/client';

export abstract class BaseService {
  constructor(protected client: ApiClient) {}

  /**
   * Access to the underlying request method for domain services.
   */
  protected request<T>(
    path: string,
    options?: RequestInit,
    skipRetry?: boolean,
    cancelToken?: AbortSignal,
    allowMutationRetry?: boolean
  ): Promise<T> {
    return this.client.request<T>(path, options, skipRetry, cancelToken, allowMutationRetry);
  }

  /**
   * Access to requestList for array responses.
   */
  protected requestList<T>(
    path: string,
    options?: RequestInit,
    skipRetry?: boolean,
    cancelToken?: AbortSignal
  ): Promise<T[]> {
    return this.client.requestList<T>(path, options, skipRetry, cancelToken);
  }

  /**
   * Build full URL for direct fetch calls (e.g., blob downloads).
   */
  protected buildUrl(path: string): string {
    return this.client.buildUrl(path);
  }

  /**
   * Get current auth token for manual requests.
   */
  protected getToken(): string | undefined {
    return this.client.getToken();
  }

  /**
   * Set auth token (for login flows).
   */
  protected setToken(token: string): void {
    this.client.setToken(token);
  }

}
