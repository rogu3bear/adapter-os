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

  'OUT_OF_MEMORY': (context) => ({
    title: 'Not Enough Memory',
    message: context?.memoryRequired && context?.memoryAvailable
      ? `Could not load the model: need ${context.memoryRequired}MB, but only ${context.memoryAvailable}MB available.`
      : 'Could not load the model because the system is out of memory. Try unloading other models or closing applications.',
    actionText: 'Free Memory',
    helpUrl: '/docs/models#memory-management',
    variant: 'error'
  }),

  'MIGRATION_INVALID': () => ({
    title: 'Database Migration Error',
    message: 'Schema or signature mismatch detected. Re-run migrations before continuing.',
    actionText: 'Run migrations',
    helpUrl: '/docs/troubleshooting#database',
    variant: 'error'
  }),

  'TRACE_WRITE_FAILED': () => ({
    title: 'Trace Persistence Failed',
    message: 'Could not persist trace or telemetry for this request. Check database and disk space.',
    actionText: 'Retry request',
    helpUrl: '/docs/troubleshooting#observability',
    variant: 'warning'
  }),

  'RECEIPT_MISMATCH': () => ({
    title: 'Receipt Verification Failed',
    message: 'The run receipt did not match the recorded trace. Re-run with matching manifest and backend.',
    actionText: 'Retry with same manifest',
    helpUrl: '/docs/troubleshooting#replay',
    variant: 'error'
  }),

  'POLICY_DIVERGENCE': () => ({
    title: 'Policy Divergence Detected',
    message: 'A policy check failed or diverged from expected state. Review active policy packs.',
    actionText: 'Review policies',
    helpUrl: '/docs/policies',
    variant: 'warning'
  }),

  'BACKEND_FALLBACK': () => ({
    title: 'Backend Fallback Triggered',
    message: 'Execution fell back to a different backend. Performance or determinism may differ.',
    actionText: 'Review backend settings',
    helpUrl: '/docs/backends',
    variant: 'info'
  }),

  'TENANT_ACCESS_DENIED': () => ({
    title: 'Workspace Access Denied',
    message: 'You do not have access to this workspace. Switch to an allowed workspace.',
    actionText: 'Select workspace',
    helpUrl: '/docs/administration#tenants',
    variant: 'warning'
  }),

  'LOAD_FAILED': (context) => ({
    title: 'Model Loading Failed',
    message: context?.modelId
      ? `We couldn't load the model "${context.modelId}". This might be temporary; please try again.`
      : 'We couldn\'t load the model. This might be temporary; please try again.',
    actionText: 'Try Again',
    helpUrl: '/docs/troubleshooting#loading-issues',
    variant: 'error'
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

  'DATASET_TRUST_BLOCKED': () => ({
    title: 'Dataset blocked by trust gate',
    message: 'Dataset trust_state is blocked; override or adjust the dataset to proceed.',
    actionText: 'Review dataset trust',
    helpUrl: '/docs/training/aos_adapters',
    variant: 'error'
  }),

  'DATASET_TRUST_NEEDS_APPROVAL': () => ({
    title: 'Dataset needs approval',
    message: 'Dataset trust_state requires approval or validation before training.',
    actionText: 'Review dataset validation',
    helpUrl: '/docs/training/aos_adapters',
    variant: 'warning'
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

  'SYSTEM_NOT_READY': () => ({
    title: 'System Not Ready',
    message: 'The system is still starting up. Please wait a moment and try again.',
    actionText: 'Check Status',
    helpUrl: '/docs/troubleshooting#startup',
    variant: 'warning'
  }),

  'NO_WORKERS': () => ({
    title: 'No Workers Available',
    message: 'No inference workers are currently running. Start a worker to enable chat.',
    actionText: 'Start Worker',
    helpUrl: '/docs/quickstart#workers',
    variant: 'warning'
  }),

  'NO_WORKER_AVAILABLE': () => ({
    title: 'No Workers Available',
    message: 'No inference workers are currently running. Start a worker to enable chat.',
    actionText: 'Start Worker',
    helpUrl: '/docs/quickstart#workers',
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
  }),

  // Chat-specific errors
  'WORKER_UNAVAILABLE': () => ({
    title: 'Inference Service Unavailable',
    message: 'The inference service is temporarily unavailable. This might be due to high load or maintenance.',
    actionText: 'Try Again',
    helpUrl: '/docs/chat#inference-service',
    variant: 'warning'
  }),

  'LOADING_TIMEOUT': () => ({
    title: 'Loading Timeout',
    message: 'The request took too long to complete. This might be due to loading large models or high server load.',
    actionText: 'Try Again',
    helpUrl: '/docs/chat#timeouts',
    variant: 'warning'
  }),

  // Initial page load errors
  'INITIAL_LOAD_TIMEOUT': () => ({
    title: 'Loading Timeout',
    message: 'The page is taking too long to load. The server may be busy or unavailable.',
    actionText: 'Retry All',
    helpUrl: '/docs/troubleshooting#timeouts',
    variant: 'warning'
  }),

  'NO_WORKERS_AVAILABLE': () => ({
    title: 'No Workers Available',
    message: 'No inference workers are available. Start a worker to enable chat.',
    actionText: 'View Docs',
    helpUrl: '/docs/quickstart#starting-workers',
    variant: 'warning'
  }),

  'DRAINING': () => ({
    title: 'System Maintenance',
    message: 'The system is currently draining for maintenance. Please try again shortly.',
    actionText: 'Check Status',
    helpUrl: '/docs/status',
    variant: 'info'
  }),

  // Migration errors (Category 5)
  'MIGRATION_FILE_MISSING': () => ({
    title: 'Migration File Missing',
    message: 'A required database migration file is missing. Please ensure all migration files are present.',
    actionText: 'Check Migrations',
    helpUrl: '/docs/troubleshooting#migrations',
    variant: 'error'
  }),

  'MIGRATION_CHECKSUM_MISMATCH': () => ({
    title: 'Migration Checksum Mismatch',
    message: 'A migration file has been modified after being applied. This may cause inconsistencies.',
    actionText: 'Review Migrations',
    helpUrl: '/docs/troubleshooting#migration-checksums',
    variant: 'error'
  }),

  'MIGRATION_OUT_OF_ORDER': () => ({
    title: 'Migration Out of Order',
    message: 'Migration files are being applied out of sequence. Please check migration timestamps.',
    actionText: 'Fix Order',
    helpUrl: '/docs/troubleshooting#migration-order',
    variant: 'error'
  }),

  'DOWN_MIGRATION_BLOCKED': () => ({
    title: 'Rollback Blocked',
    message: 'Cannot roll back migration because the table contains data. Back up data before proceeding.',
    actionText: 'View Table',
    helpUrl: '/docs/troubleshooting#migration-rollback',
    variant: 'warning'
  }),

  'SCHEMA_VERSION_MISMATCH': () => ({
    title: 'Schema Version Mismatch',
    message: 'The database schema version doesn\'t match the application. Run migrations to sync.',
    actionText: 'Run Migrations',
    helpUrl: '/docs/troubleshooting#schema-version',
    variant: 'error'
  }),

  'SCHEMA_VERSION_AHEAD': () => ({
    title: 'Schema Version Ahead',
    message: 'The database schema is newer than this application version. Update the application.',
    actionText: 'Update App',
    helpUrl: '/docs/troubleshooting#schema-version',
    variant: 'warning'
  }),

  // Cache errors (Category 6)
  'CACHE_STALE': (context) => ({
    title: 'Cache Data Stale',
    message: context?.retryAfter
      ? `Cached data has expired. Refreshing in ${context.retryAfter} seconds.`
      : 'Cached data has expired and is being refreshed.',
    actionText: 'Refresh Now',
    helpUrl: '/docs/troubleshooting#cache',
    variant: 'info'
  }),

  'CACHE_EVICTION': () => ({
    title: 'Cache Eviction',
    message: 'Some cached data was evicted to free up memory. This is normal under high load.',
    actionText: 'Dismiss',
    helpUrl: '/docs/troubleshooting#cache-eviction',
    variant: 'info'
  }),

  'CACHE_KEY_NONDETERMINISTIC': () => ({
    title: 'Cache Key Issue',
    message: 'A nondeterministic cache key was detected. Results may vary between requests.',
    actionText: 'Report Issue',
    helpUrl: '/docs/troubleshooting#cache-determinism',
    variant: 'warning'
  }),

  'CACHE_SERIALIZATION_ERROR': () => ({
    title: 'Cache Serialization Error',
    message: 'Failed to serialize or deserialize cached data. The cache will be refreshed.',
    actionText: 'Retry',
    helpUrl: '/docs/troubleshooting#cache-serialization',
    variant: 'warning'
  }),

  'CACHE_INVALIDATION_FAILED': () => ({
    title: 'Cache Invalidation Failed',
    message: 'Failed to invalidate stale cache entries. Some data may be outdated.',
    actionText: 'Clear Cache',
    helpUrl: '/docs/troubleshooting#cache-invalidation',
    variant: 'warning'
  }),

  // Rate limiting errors (Category 23)
  'RATE_LIMITER_NOT_CONFIGURED': () => ({
    title: 'Rate Limiter Not Configured',
    message: 'The rate limiter is not properly configured for this resource.',
    actionText: 'Contact Admin',
    helpUrl: '/docs/administration#rate-limiting',
    variant: 'error'
  }),

  'INVALID_RATE_LIMIT_CONFIG': () => ({
    title: 'Invalid Rate Limit Configuration',
    message: 'The rate limiting configuration is invalid. Using default limits.',
    actionText: 'Review Config',
    helpUrl: '/docs/administration#rate-limit-config',
    variant: 'warning'
  }),

  'THUNDERING_HERD_REJECTED': (context) => ({
    title: 'Request Temporarily Blocked',
    message: context?.retryAfter
      ? `Too many simultaneous requests detected. Please retry in ${context.retryAfter} seconds.`
      : 'Too many simultaneous requests detected. Please wait a moment and try again.',
    actionText: 'Try Again Later',
    helpUrl: '/docs/troubleshooting#thundering-herd',
    variant: 'warning'
  }),

  // Config errors (Category 1)
  'CONFIG_FILE_NOT_FOUND': () => ({
    title: 'Configuration Not Found',
    message: 'The configuration file could not be found. Using default settings.',
    actionText: 'Create Config',
    helpUrl: '/docs/configuration#file-locations',
    variant: 'warning'
  }),

  'CONFIG_FILE_PERMISSION_DENIED': () => ({
    title: 'Configuration Access Denied',
    message: 'Cannot read configuration file due to permission issues.',
    actionText: 'Fix Permissions',
    helpUrl: '/docs/configuration#permissions',
    variant: 'error'
  }),

  'CONFIG_SCHEMA_VIOLATION': () => ({
    title: 'Invalid Configuration',
    message: 'The configuration file contains invalid values. Check the schema requirements.',
    actionText: 'Fix Config',
    helpUrl: '/docs/configuration#schema',
    variant: 'error'
  }),

  'EMPTY_ENV_OVERRIDE': () => ({
    title: 'Empty Environment Variable',
    message: 'An environment variable override is set but empty. Using default value instead.',
    actionText: 'Fix Environment',
    helpUrl: '/docs/configuration#environment',
    variant: 'warning'
  }),

  'BLANK_SECRET': () => ({
    title: 'Missing Secret',
    message: 'A required secret is blank or not configured. Please set it before continuing.',
    actionText: 'Configure Secret',
    helpUrl: '/docs/configuration#secrets',
    variant: 'error'
  }),

  // Toolchain/Build errors (Category 20)
  'TOOLCHAIN_MISMATCH': () => ({
    title: 'Toolchain Version Mismatch',
    message: 'The required toolchain version doesn\'t match. Some features may not work correctly.',
    actionText: 'Update Toolchain',
    helpUrl: '/docs/installation#toolchain',
    variant: 'warning'
  }),

  'STALE_BUILD_CACHE': () => ({
    title: 'Stale Build Cache',
    message: 'The build cache is outdated and may cause issues. Consider clearing it.',
    actionText: 'Clear Cache',
    helpUrl: '/docs/troubleshooting#build-cache',
    variant: 'info'
  }),

  // Network/DNS errors (Category 3)
  'DNS_RESOLUTION_FAILED': () => ({
    title: 'DNS Resolution Failed',
    message: 'Could not resolve the server hostname. Check your network connection or DNS settings.',
    actionText: 'Check Network',
    helpUrl: '/docs/troubleshooting#dns',
    variant: 'error'
  }),

  'TLS_CERTIFICATE_ERROR': () => ({
    title: 'TLS Certificate Error',
    message: 'Could not verify the server\'s TLS certificate. The connection may not be secure.',
    actionText: 'View Details',
    helpUrl: '/docs/troubleshooting#tls',
    variant: 'error'
  }),

  'PROXY_CONNECTION_FAILED': () => ({
    title: 'Proxy Connection Failed',
    message: 'Could not connect through the configured proxy server.',
    actionText: 'Check Proxy',
    helpUrl: '/docs/configuration#proxy',
    variant: 'error'
  }),

  // SSE/Streaming errors (Category 18)
  'STREAM_DISCONNECTED': (context) => ({
    title: 'Stream Disconnected',
    message: context?.retryAfter
      ? `The real-time connection was lost. Reconnecting in ${context.retryAfter} seconds.`
      : 'The real-time connection was lost. Attempting to reconnect.',
    actionText: 'Reconnect Now',
    helpUrl: '/docs/troubleshooting#streaming',
    variant: 'warning'
  }),

  'BUFFER_OVERFLOW': () => ({
    title: 'Event Buffer Overflow',
    message: 'Some real-time events were dropped due to high volume. Data will be refreshed.',
    actionText: 'Refresh',
    helpUrl: '/docs/troubleshooting#streaming-buffer',
    variant: 'warning'
  }),

  'EVENT_GAP_DETECTED': () => ({
    title: 'Events Missed',
    message: 'Some real-time events were missed during a disconnection. Data may need refreshing.',
    actionText: 'Refresh Data',
    helpUrl: '/docs/troubleshooting#event-gaps',
    variant: 'warning'
  }),

  // Storage errors (Category 9)
  'STORAGE_QUOTA_EXCEEDED': () => ({
    title: 'Storage Quota Exceeded',
    message: 'You\'ve exceeded your storage quota. Delete some files or upgrade your plan.',
    actionText: 'Manage Storage',
    helpUrl: '/docs/storage#quotas',
    variant: 'error'
  }),

  'STATIC_ASSET_NOT_FOUND': () => ({
    title: 'Asset Not Found',
    message: 'A required static asset could not be loaded. Try refreshing the page.',
    actionText: 'Refresh',
    helpUrl: '/docs/troubleshooting#assets',
    variant: 'warning'
  }),

  // Security errors
  'CSP_VIOLATION': () => ({
    title: 'Security Policy Violation',
    message: 'A content security policy violation was detected. Some features may not work.',
    actionText: 'Report Issue',
    helpUrl: '/docs/security#csp',
    variant: 'warning'
  }),

  // CLI errors (Category 21)
  'DEPRECATED_FLAG': () => ({
    title: 'Deprecated Flag Used',
    message: 'A deprecated command-line flag was used. Please update to the new syntax.',
    actionText: 'View Migration',
    helpUrl: '/docs/cli#deprecated-flags',
    variant: 'warning'
  }),

  'OUTPUT_FORMAT_MISMATCH': () => ({
    title: 'Output Format Error',
    message: 'The requested output format doesn\'t match the available data format.',
    actionText: 'Fix Format',
    helpUrl: '/docs/cli#output-formats',
    variant: 'warning'
  }),

  'INVALID_INPUT_ENCODING': () => ({
    title: 'Invalid Input Encoding',
    message: 'The input contains invalid character encoding. Please use UTF-8.',
    actionText: 'Fix Encoding',
    helpUrl: '/docs/troubleshooting#encoding',
    variant: 'error'
  }),

  'INVALID_RETRY_ATTEMPT': () => ({
    title: 'Cannot Retry',
    message: 'This error type cannot be automatically retried. Please fix the underlying issue.',
    actionText: 'View Details',
    helpUrl: '/docs/troubleshooting#retries',
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
): Error & { userFriendly: UserFriendlyError; originalError: unknown; requestId?: string } {
  const failureCode = (originalError as { failure_code?: string }).failure_code;
  const errorCode = failureCode ?? (originalError as { code?: string }).code;
  const httpStatus = (originalError as { status?: number }).status;
  const backendMessage = (originalError as { message?: string }).message;
  
  // Get user-friendly error template
  const userFriendlyTemplate = getUserFriendlyError(errorCode, httpStatus, context);
  
  // Prefer backend error message if it's more specific than the generic HTTP status message
  // Check if backend message exists and is different from the generic template message
  const finalMessage = backendMessage && 
    backendMessage !== userFriendlyTemplate.message &&
    !backendMessage.startsWith('HTTP ') // Skip generic HTTP status messages
    ? backendMessage
    : userFriendlyTemplate.message;

  const userFriendly: UserFriendlyError = {
    ...userFriendlyTemplate,
    message: finalMessage,
  };

  const enhancedError = new Error(finalMessage) as Error & {
    userFriendly: UserFriendlyError;
    originalError: unknown;
    code?: string;
    status?: number;
    failure_code?: string;
  };

  enhancedError.name = 'UserFriendlyError';
  enhancedError.userFriendly = userFriendly;
  enhancedError.originalError = originalError;
  enhancedError.code = errorCode;
  enhancedError.status = httpStatus;
  if (failureCode) {
    enhancedError.failure_code = failureCode;
  }

  return enhancedError;
}

/**
 * Checks if an error is transient and should be retried
 */
export function isTransientError(error: unknown): boolean {
  const errorCode = (error as { failure_code?: string }).failure_code ?? (error as { code?: string }).code;
  const httpStatus = (error as { status?: number }).status;

  // Error codes that indicate transient failures
  const transientCodes = [
    'NETWORK_ERROR',
    'TIMEOUT',
    'RATE_LIMIT',
    'RESOURCE_BUSY',
    'SERVICE_UNAVAILABLE',
    'MAINTENANCE',
    // Streaming/SSE transient errors
    'STREAM_DISCONNECTED',
    'BUFFER_OVERFLOW',
    'EVENT_GAP_DETECTED',
    // Cache transient errors
    'CACHE_STALE',
    'CACHE_EVICTION',
    'CACHE_INVALIDATION_FAILED',
    // Rate limiting transient errors
    'THUNDERING_HERD_REJECTED',
    // Network transient errors
    'DNS_RESOLUTION_FAILED',
    'PROXY_CONNECTION_FAILED',
  ];

  // HTTP status codes that indicate transient failures
  const transientStatuses = [429, 500, 502, 503, 504];

  return (errorCode !== undefined && transientCodes.includes(errorCode)) ||
         (httpStatus !== undefined && transientStatuses.includes(httpStatus));
}

/**
 * Error codes that should NOT be retried automatically
 * These require user intervention to fix the underlying issue
 */
export function isNonRetryableError(error: unknown): boolean {
  const errorCode = (error as { failure_code?: string }).failure_code ?? (error as { code?: string }).code;

  const nonRetryableCodes = [
    // Auth errors - need user action
    'UNAUTHORIZED',
    'FORBIDDEN',
    'SESSION_EXPIRED',
    // Validation errors - need input correction
    'CONFIG_SCHEMA_VIOLATION',
    'INVALID_PROMPT',
    'INVALID_FILE_FORMAT',
    'INVALID_TRAINING_DATA',
    'INVALID_INPUT_ENCODING',
    // Resource not found - won't magically appear
    'ADAPTER_NOT_FOUND',
    'MODEL_NOT_FOUND',
    // Corruption - needs manual intervention
    'ADAPTER_CORRUPTED',
    // Migration errors - need manual intervention
    'MIGRATION_CHECKSUM_MISMATCH',
    'MIGRATION_OUT_OF_ORDER',
    'DOWN_MIGRATION_BLOCKED',
    // Config errors - need configuration change
    'BLANK_SECRET',
    'CONFIG_FILE_PERMISSION_DENIED',
    'RATE_LIMITER_NOT_CONFIGURED',
    // Policy errors - need policy review
    'POLICY_DIVERGENCE',
    'TENANT_ACCESS_DENIED',
  ];

  return errorCode !== undefined && nonRetryableCodes.includes(errorCode);
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
