/**
 * Common Hook Types
 * Types for custom React hooks
 */

export interface UseQueryOptions<TData = unknown, TError = Error> {
  enabled?: boolean;
  refetchOnWindowFocus?: boolean;
  refetchInterval?: number;
  retry?: boolean | number;
  retryDelay?: number;
  staleTime?: number;
  cacheTime?: number;
  onSuccess?: (data: TData) => void;
  onError?: (error: TError) => void;
}

export interface UseQueryResult<TData = unknown, TError = Error> {
  data?: TData;
  error?: TError;
  isLoading: boolean;
  isFetching: boolean;
  isError: boolean;
  isSuccess: boolean;
  refetch: () => Promise<void>;
}

export interface UseMutationOptions<TData = unknown, TVariables = void, TError = Error> {
  onSuccess?: (data: TData, variables: TVariables) => void;
  onError?: (error: TError, variables: TVariables) => void;
  onSettled?: (data: TData | undefined, error: TError | null, variables: TVariables) => void;
  retry?: boolean | number;
}

export interface UseMutationResult<TData = unknown, TVariables = void, TError = Error> {
  mutate: (variables: TVariables) => void;
  mutateAsync: (variables: TVariables) => Promise<TData>;
  data?: TData;
  error?: TError;
  isLoading: boolean;
  isError: boolean;
  isSuccess: boolean;
  reset: () => void;
}

export interface UseDebounceOptions {
  delay?: number;
  leading?: boolean;
  trailing?: boolean;
}

export interface UseThrottleOptions {
  interval?: number;
  leading?: boolean;
  trailing?: boolean;
}

export interface UseLocalStorageOptions<T> {
  serializer?: (value: T) => string;
  deserializer?: (value: string) => T;
  syncData?: boolean;
}

export interface UseLocalStorageResult<T> {
  value: T;
  setValue: (value: T | ((prev: T) => T)) => void;
  removeValue: () => void;
}

export interface UseMediaQueryResult {
  matches: boolean;
}

export interface UseIntersectionObserverOptions {
  threshold?: number | number[];
  root?: Element | null;
  rootMargin?: string;
}

export interface UseIntersectionObserverResult {
  isIntersecting: boolean;
  entry?: IntersectionObserverEntry;
}

export interface UseClickOutsideOptions {
  enabled?: boolean;
  onClickOutside: (event: MouseEvent | TouchEvent) => void;
}

export interface UseClipboardResult {
  copy: (text: string) => Promise<void>;
  copied: boolean;
  error?: Error;
}

export interface UsePaginationOptions {
  total: number;
  initialPage?: number;
  pageSize?: number;
  onPageChange?: (page: number) => void;
}

export interface UsePaginationResult {
  currentPage: number;
  pageSize: number;
  totalPages: number;
  hasNextPage: boolean;
  hasPreviousPage: boolean;
  nextPage: () => void;
  previousPage: () => void;
  goToPage: (page: number) => void;
  setPageSize: (size: number) => void;
}

export interface UseFormValidationRule {
  validate: (value: any) => boolean | string;
  message?: string;
}

export interface UseFormFieldConfig {
  defaultValue?: any;
  required?: boolean;
  rules?: UseFormValidationRule[];
}

export interface UseWebSocketOptions {
  onOpen?: (event: Event) => void;
  onClose?: (event: CloseEvent) => void;
  onMessage?: (event: MessageEvent) => void;
  onError?: (event: Event) => void;
  reconnect?: boolean;
  reconnectAttempts?: number;
  reconnectInterval?: number;
}

export interface UseWebSocketResult {
  readyState: number;
  send: (data: string | ArrayBufferLike | Blob | ArrayBufferView) => void;
  lastMessage?: MessageEvent;
  isConnected: boolean;
}
