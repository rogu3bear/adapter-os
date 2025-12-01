//! Error Message Mapping Utility
//!
//! Maps backend error codes to user-friendly, actionable error messages.
//! Provides context-aware messaging based on operation type and error context.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L300-L350 - Error recovery UX patterns
//! - ui/src/components/ui/error-recovery.tsx L1-L50 - Error handling patterns

export interface ErrorContext {
  operation?: string;
  adapterId?: string;
  modelId?: string;
  tenantId?: string;
  fileSize?: number;
  memoryRequired?: number;
  memoryAvailable?: number;
  retryAfter?: number;
  [key: string]: unknown;
}

export interface UserFriendlyError {
  title: string;
  message: string;
  actionText?: string;
  helpUrl?: string;
  variant: 'error' | 'warning' | 'info';
}

// Error code to user-friendly message mapping
const ERROR_CODE_MAP: Record<string, (context?: ErrorContext) => UserFriendlyError> = {
  // Network and connectivity errors
  'NETWORK_ERROR': () => ({
    title: 'Connection Problem',
    message: 'We\'re having trouble connecting to the server. This is usually temporary.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#network-issues',
    variant: 'warning'
  }),

  'TIMEOUT': () => ({
    title: 'Request Timed Out',
    message: 'The request took too long to complete. This might be due to high server load.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#timeouts',
    variant: 'warning'
  }),

  // Rate limiting
  'RATE_LIMIT': (context) => ({
    title: 'Too Many Requests',
    message: context?.retryAfter
      ? `You've made too many requests. Please wait ${context.retryAfter} seconds before trying again.`
      : 'You\'ve made too many requests. Please wait a moment before trying again.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/limits#rate-limits',
    variant: 'info'
  }),

  // Authentication and authorization
  'UNAUTHORIZED': () => ({
    title: 'Authentication Required',
    message: 'You need to log in to perform this action.',
    actionText: 'Log In',
    helpUrl: '/docs/getting-started#authentication',
    variant: 'warning'
  }),

  'FORBIDDEN': () => ({
    title: 'Permission Denied',
    message: 'You don\'t have permission to perform this action. Please contact your administrator.',
    actionText: 'Go to Dashboard',
    helpUrl: '/docs/administration#permissions',
    variant: 'warning'
  }),

  'SESSION_EXPIRED': () => ({
    title: 'Session Expired',
    message: 'Your session has expired. Please log in again.',
    actionText: 'Log In',
    helpUrl: '/docs/getting-started#authentication',
    variant: 'warning'
  }),

  // Resource constraints
  'INSUFFICIENT_MEMORY': (context) => ({
    title: 'Not Enough Memory',
    message: context?.memoryRequired && context?.memoryAvailable
      ? `Not enough memory to complete this operation. Need ${context.memoryRequired}MB, but only ${context.memoryAvailable}MB available.`
      : 'Not enough memory to complete this operation. Try unloading some adapters first.',
    actionText: 'Free Memory',
    helpUrl: '/docs/adapters#memory-management',
    variant: 'warning'
  }),

  'DISK_FULL': () => ({
    title: 'Storage Full',
    message: 'There\'s not enough disk space to complete this operation.',
    actionText: 'Free Space',
    helpUrl: '/docs/administration#storage',
    variant: 'error'
  }),

  'RESOURCE_BUSY': () => ({
    title: 'Resource Busy',
    message: 'The requested resource is currently in use. Please try again later.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#resource-busy',
    variant: 'info'
  }),

  // Adapter-specific errors
  'ADAPTER_NOT_FOUND': (context) => ({
    title: 'Adapter Not Found',
    message: context?.adapterId
      ? `The adapter "${context.adapterId}" was not found. It may have been deleted or moved.`
      : 'The requested adapter was not found.',
    actionText: 'Refresh List',
    helpUrl: '/docs/adapters#managing-adapters',
    variant: 'warning'
  }),

  'ADAPTER_ALREADY_LOADED': (context) => ({
    title: 'Adapter Already Loaded',
    message: context?.adapterId
      ? `The adapter "${context.adapterId}" is already loaded and ready to use.`
      : 'This adapter is already loaded.',
    actionText: 'Use Adapter',
    helpUrl: '/docs/adapters#using-adapters',
    variant: 'info'
  }),

  'ADAPTER_LOAD_FAILED': (context) => ({
    title: 'Adapter Loading Failed',
    message: context?.adapterId
      ? `We couldn't load the adapter "${context.adapterId}". This might be due to insufficient memory or a corrupted adapter file.`
      : 'We couldn\'t load the adapter. This might be due to insufficient memory or a corrupted file.',
    actionText: 'Try Again',
    helpUrl: '/docs/adapters#loading-issues',
    variant: 'error'
  }),

  'ADAPTER_CORRUPTED': (context) => ({
    title: 'Adapter File Corrupted',
    message: context?.adapterId
      ? `The adapter file for "${context.adapterId}" appears to be corrupted.`
      : 'The adapter file appears to be corrupted.',
    actionText: 'Re-upload Adapter',
    helpUrl: '/docs/adapters#corrupted-files',
    variant: 'error'
  }),

  // Training errors
  'TRAINING_FAILED': (context) => ({
    title: 'Training Failed',
    message: 'The adapter training process encountered an error. This could be due to insufficient resources, invalid data, or configuration issues.',
    actionText: 'Check Configuration',
    helpUrl: '/docs/training#troubleshooting',
    variant: 'error'
  }),

  'INVALID_TRAINING_DATA': () => ({
    title: 'Invalid Training Data',
    message: 'The training data format is not valid. Please check your data format and try again.',
    actionText: 'Fix Data Format',
    helpUrl: '/docs/training#data-format',
    variant: 'error'
  }),

  'TRAINING_TIMEOUT': () => ({
    title: 'Training Timed Out',
    message: 'The training job took too long to complete and was cancelled.',
    actionText: 'Try Again',
    helpUrl: '/docs/training#timeouts',
    variant: 'warning'
  }),

  // Model errors
  'MODEL_NOT_FOUND': (context) => ({
    title: 'Model Not Found',
    message: context?.modelId
      ? `The model "${context.modelId}" was not found. It may have been deleted or is not available.`
      : 'The requested model was not found.',
    actionText: 'Choose Different Model',
    helpUrl: '/docs/models#available-models',
    variant: 'warning'
  }),

  'MODEL_BUSY': (context) => ({
    title: 'Model In Use',
    message: context?.modelId
      ? `The model "${context.modelId}" is currently being used by another operation.`
      : 'The model is currently in use by another operation.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/models#concurrency',
    variant: 'info'
  }),

  'MODEL_LOAD_FAILED': (context) => ({
    title: 'Model Loading Failed',
    message: context?.modelId
      ? `We couldn't load the model "${context.modelId}". This might be due to insufficient memory or network issues.`
      : 'We couldn\'t load the model. This might be due to insufficient memory or network issues.',
    actionText: 'Try Again',
    helpUrl: '/docs/models#loading-issues',
    variant: 'error'
  }),

  // File upload errors
  'FILE_TOO_LARGE': (context) => ({
    title: 'File Too Large',
    message: context?.fileSize
      ? `The file is too large (${Math.round(context.fileSize / 1024 / 1024)}MB). Please choose a smaller file.`
      : 'The file is too large. Please choose a smaller file.',
    actionText: 'Choose Smaller File',
    helpUrl: '/docs/uploads#file-limits',
    variant: 'warning'
  }),

  'INVALID_FILE_FORMAT': () => ({
    title: 'Invalid File Format',
    message: 'The file format is not supported. Please check the supported formats.',
    actionText: 'Check Supported Formats',
    helpUrl: '/docs/uploads#supported-formats',
    variant: 'error'
  }),

  'UPLOAD_FAILED': () => ({
    title: 'Upload Failed',
    message: 'The file upload failed. This might be due to network issues or server problems.',
    actionText: 'Try Again',
    helpUrl: '/docs/uploads#troubleshooting',
    variant: 'error'
  }),

  // Inference errors
  'INFERENCE_FAILED': () => ({
    title: 'Inference Failed',
    message: 'We couldn\'t generate a response. This might be due to model issues, resource constraints, or invalid input.',
    actionText: 'Try Again',
    helpUrl: '/docs/inference#common-issues',
    variant: 'warning'
  }),

  'INVALID_PROMPT': () => ({
    title: 'Invalid Input',
    message: 'The input prompt is not valid. Please check your input and try again.',
    actionText: 'Fix Prompt',
    helpUrl: '/docs/inference#input-validation',
    variant: 'warning'
  }),

  // Generic server errors
  'INTERNAL_SERVER_ERROR': () => ({
    title: 'Server Error',
    message: 'An unexpected server error occurred. Our team has been notified and is working to resolve this.',
    actionText: 'Try Again',
    helpUrl: '/docs/support',
    variant: 'error'
  }),

  'SERVICE_UNAVAILABLE': () => ({
    title: 'Service Unavailable',
    message: 'The service is temporarily unavailable. Please try again in a few minutes.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/status',
    variant: 'warning'
  }),

  'MAINTENANCE': () => ({
    title: 'Maintenance In Progress',
    message: 'The system is currently undergoing maintenance. Please try again later.',
    actionText: 'Check Status',
    helpUrl: '/docs/status',
    variant: 'info'
  }),

  // Response parsing errors
  'PARSE_ERROR': () => ({
    title: 'Invalid Server Response',
    message: 'The server returned an unexpected response format. This is usually a temporary issue.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#server-errors',
    variant: 'warning'
  }),

  'RESPONSE_FORMAT_ERROR': () => ({
    title: 'Response Format Error',
    message: 'We received data in an unexpected format. Please try again or contact support if the issue persists.',
    actionText: 'Try Again',
    helpUrl: '/docs/support',
    variant: 'warning'
  })
};

// HTTP status code mappings (fallback when no specific error code)
const HTTP_STATUS_MAP: Record<number, (context?: ErrorContext) => UserFriendlyError> = {
  400: () => ({
    title: 'Bad Request',
    message: 'The request was invalid. Please check your input and try again.',
    actionText: 'Fix Input',
    helpUrl: '/docs/api#bad-request',
    variant: 'warning'
  }),

  401: () => ({
    title: 'Authentication Required',
    message: 'You need to log in to perform this action.',
    actionText: 'Log In',
    helpUrl: '/docs/getting-started#authentication',
    variant: 'warning'
  }),

  403: () => ({
    title: 'Permission Denied',
    message: 'You don\'t have permission to perform this action.',
    actionText: 'Contact Admin',
    helpUrl: '/docs/administration#permissions',
    variant: 'warning'
  }),

  404: () => ({
    title: 'Not Found',
    message: 'The requested resource was not found.',
    actionText: 'Go Back',
    helpUrl: '/docs/navigation',
    variant: 'warning'
  }),

  409: () => ({
    title: 'Conflict',
    message: 'There was a conflict with the current state. Please refresh and try again.',
    actionText: 'Refresh',
    helpUrl: '/docs/troubleshooting#conflicts',
    variant: 'warning'
  }),

  429: (context) => ({
    title: 'Too Many Requests',
    message: context?.retryAfter
      ? `You've made too many requests. Please wait ${context.retryAfter} seconds.`
      : 'You\'ve made too many requests. Please wait before trying again.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/limits#rate-limits',
    variant: 'info'
  }),

  500: () => ({
    title: 'Server Error',
    message: 'An unexpected server error occurred. Our team has been notified.',
    actionText: 'Try Again',
    helpUrl: '/docs/support',
    variant: 'error'
  }),

  502: () => ({
    title: 'Bad Gateway',
    message: 'The server received an invalid response from an upstream server.',
    actionText: 'Try Again',
    helpUrl: '/docs/status',
    variant: 'warning'
  }),

  503: () => ({
    title: 'Service Unavailable',
    message: 'The service is temporarily unavailable.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/status',
    variant: 'warning'
  }),

  504: () => ({
    title: 'Gateway Timeout',
    message: 'The request timed out. Please try again.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#timeouts',
    variant: 'warning'
  })
};

/**
 * Maps an error code or HTTP status to a user-friendly error message
 */
export function getUserFriendlyError(
  errorCode?: string,
  httpStatus?: number,
  context?: ErrorContext
): UserFriendlyError {
  // Try error code first
  if (errorCode && ERROR_CODE_MAP[errorCode]) {
    return ERROR_CODE_MAP[errorCode](context);
  }

  // Fall back to HTTP status
  if (httpStatus && HTTP_STATUS_MAP[httpStatus]) {
    return HTTP_STATUS_MAP[httpStatus](context);
  }

  // Generic fallback
  return {
    title: 'Something went wrong',
    message: 'An unexpected error occurred. Our team has been notified.',
    actionText: 'Try Again',
    helpUrl: '/docs/support',
    variant: 'error'
  };
}

/**
 * Creates an enhanced error object with user-friendly messaging
 */
export function enhanceError(
  originalError: unknown,
  context?: ErrorContext
): Error & { userFriendly: UserFriendlyError; originalError: unknown } {
  const errorCode = (originalError as { code?: string }).code;
  const httpStatus = (originalError as { status?: number }).status;
  const userFriendly = getUserFriendlyError(errorCode, httpStatus, context);

  const enhancedError = new Error(userFriendly.message) as Error & {
    userFriendly: UserFriendlyError;
    originalError: unknown;
    code?: string;
    status?: number;
  };

  enhancedError.name = 'UserFriendlyError';
  enhancedError.userFriendly = userFriendly;
  enhancedError.originalError = originalError;
  enhancedError.code = errorCode;
  enhancedError.status = httpStatus;

  return enhancedError;
}

/**
 * Checks if an error is transient and should be retried
 */
export function isTransientError(error: unknown): boolean {
  const errorCode = (error as { code?: string }).code;
  const httpStatus = (error as { status?: number }).status;

  // Error codes that indicate transient failures
  const transientCodes = [
    'NETWORK_ERROR',
    'TIMEOUT',
    'RATE_LIMIT',
    'RESOURCE_BUSY',
    'SERVICE_UNAVAILABLE',
    'MAINTENANCE'
  ];

  // HTTP status codes that indicate transient failures
  const transientStatuses = [429, 500, 502, 503, 504];

  return transientCodes.includes(errorCode) || transientStatuses.includes(httpStatus);
}

/**
 * Checks if an error is specifically a timeout error
 */
export function isTimeoutError(error: Error): boolean {
  return error.name === 'TimeoutError' ||
         error.name === 'AbortError' ||
         error.message.includes('timeout') ||
         error.message.includes('timed out') ||
         error.message.includes('Request timeout') ||
         error.message.includes('ETIMEDOUT') ||
         error.message.includes('ESOCKETTIMEDOUT');
}
