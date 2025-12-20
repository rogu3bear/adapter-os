/**
 * Common API Types
 * Shared types for API interactions (UI-specific wrappers)
 */

export interface ApiResponse<T = any> {
  data?: T;
  error?: ApiError;
  status: number;
  message?: string;
}

export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, any>;
  stack?: string;
}

export interface PaginatedResponse<T = any> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

export interface ApiRequestConfig {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  headers?: Record<string, string>;
  params?: Record<string, any>;
  body?: any;
  timeout?: number;
  signal?: AbortSignal;
}

export interface ApiClientOptions {
  baseURL: string;
  timeout?: number;
  headers?: Record<string, string>;
  credentials?: RequestCredentials;
  onRequest?: (config: ApiRequestConfig) => ApiRequestConfig | Promise<ApiRequestConfig>;
  onResponse?: <T>(response: ApiResponse<T>) => ApiResponse<T> | Promise<ApiResponse<T>>;
  onError?: (error: ApiError) => void | Promise<void>;
}

export interface QueryParams {
  page?: number;
  pageSize?: number;
  sort?: string;
  order?: 'asc' | 'desc';
  search?: string;
  filters?: Record<string, any>;
}

export interface StreamEvent<T = any> {
  type: string;
  data: T;
  timestamp: number;
}

export interface StreamOptions {
  onMessage: (event: StreamEvent) => void;
  onError?: (error: Error) => void;
  onComplete?: () => void;
  reconnect?: boolean;
}

export interface UploadProgress {
  loaded: number;
  total: number;
  percentage: number;
}

export interface UploadOptions {
  onProgress?: (progress: UploadProgress) => void;
  onSuccess?: (response: any) => void;
  onError?: (error: ApiError) => void;
  chunkSize?: number;
}

export interface BatchRequest<T = any> {
  id: string;
  endpoint: string;
  method: string;
  body?: T;
}

export interface BatchResponse<T = any> {
  id: string;
  status: number;
  data?: T;
  error?: ApiError;
}
