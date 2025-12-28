import { describe, it, expect } from 'vitest';
import { getUserFriendlyError } from '@/utils/errorMessages';

describe('errorMessages', () => {
  describe('Network and connectivity errors', () => {
    it('maps NETWORK_ERROR to connection problem guidance', () => {
      const error = getUserFriendlyError('NETWORK_ERROR');

      expect(error.title).toBe('Connection Problem');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('having trouble connecting');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/troubleshooting#network-issues');
    });

    it('maps TIMEOUT to timeout guidance', () => {
      const error = getUserFriendlyError('TIMEOUT');

      expect(error.title).toBe('Request Timed Out');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('took too long');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/troubleshooting#timeouts');
    });
  });

  describe('Rate limiting', () => {
    it('maps RATE_LIMIT without context', () => {
      const error = getUserFriendlyError('RATE_LIMIT');

      expect(error.title).toBe('Too Many Requests');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('too many requests');
      expect(error.actionText).toBe('Try Again Later');
      expect(error.helpUrl).toBe('/docs/limits#rate-limits');
    });

    it('maps RATE_LIMIT with retryAfter context', () => {
      const error = getUserFriendlyError('RATE_LIMIT', undefined, {
        retryAfter: 60,
      });

      expect(error.title).toBe('Too Many Requests');
      expect(error.message).toContain('wait 60 seconds');
    });
  });

  describe('Authentication and authorization', () => {
    it('maps UNAUTHORIZED to authentication guidance', () => {
      const error = getUserFriendlyError('UNAUTHORIZED');

      expect(error.title).toBe('Authentication Required');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('need to log in');
      expect(error.actionText).toBe('Log In');
      expect(error.helpUrl).toBe('/docs/getting-started#authentication');
    });

    it('maps FORBIDDEN to permission denied guidance', () => {
      const error = getUserFriendlyError('FORBIDDEN');

      expect(error.title).toBe('Permission Denied');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('don\'t have permission');
      expect(error.actionText).toBe('Go to Dashboard');
      expect(error.helpUrl).toBe('/docs/administration#permissions');
    });

    it('maps SESSION_EXPIRED to session guidance', () => {
      const error = getUserFriendlyError('SESSION_EXPIRED');

      expect(error.title).toBe('Session Expired');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('session has expired');
      expect(error.actionText).toBe('Log In');
      expect(error.helpUrl).toBe('/docs/getting-started#authentication');
    });
  });

  describe('Resource constraints', () => {
    it('maps INSUFFICIENT_MEMORY without context', () => {
      const error = getUserFriendlyError('INSUFFICIENT_MEMORY');

      expect(error.title).toBe('Not Enough Memory');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('Not enough memory');
      expect(error.actionText).toBe('Free Memory');
      expect(error.helpUrl).toBe('/docs/adapters#memory-management');
    });

    it('maps INSUFFICIENT_MEMORY with memory context', () => {
      const error = getUserFriendlyError('INSUFFICIENT_MEMORY', undefined, {
        memoryRequired: 4096,
        memoryAvailable: 2048,
      });

      expect(error.message).toContain('Need 4096MB');
      expect(error.message).toContain('only 2048MB available');
    });

    it('maps OUT_OF_MEMORY to memory guidance', () => {
      const error = getUserFriendlyError('OUT_OF_MEMORY', undefined, {
        memoryRequired: 5120,
        memoryAvailable: 2048,
      });

      expect(error.title).toBe('Not Enough Memory');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('Could not load the model');
      expect(error.actionText).toBe('Free Memory');
    });

    it('maps OUT_OF_MEMORY without context', () => {
      const error = getUserFriendlyError('OUT_OF_MEMORY');

      expect(error.title).toBe('Not Enough Memory');
      expect(error.message).toContain('system is out of memory');
    });

    it('maps DISK_FULL to storage guidance', () => {
      const error = getUserFriendlyError('DISK_FULL');

      expect(error.title).toBe('Storage Full');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('not enough disk space');
      expect(error.actionText).toBe('Free Space');
      expect(error.helpUrl).toBe('/docs/administration#storage');
    });

    it('maps RESOURCE_BUSY to busy guidance', () => {
      const error = getUserFriendlyError('RESOURCE_BUSY');

      expect(error.title).toBe('Resource Busy');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('currently in use');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/troubleshooting#resource-busy');
    });
  });

  describe('Database and system errors', () => {
    it('maps MIGRATION_INVALID to migration guidance', () => {
      const error = getUserFriendlyError('MIGRATION_INVALID');

      expect(error.title).toBe('Database Migration Error');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('Schema or signature mismatch');
      expect(error.actionText).toBe('Run migrations');
      expect(error.helpUrl).toBe('/docs/troubleshooting#database');
    });

    it('maps TRACE_WRITE_FAILED to trace guidance', () => {
      const error = getUserFriendlyError('TRACE_WRITE_FAILED');

      expect(error.title).toBe('Trace Persistence Failed');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('Could not persist trace');
      expect(error.actionText).toBe('Retry request');
      expect(error.helpUrl).toBe('/docs/troubleshooting#observability');
    });

    it('maps RECEIPT_MISMATCH to receipt guidance', () => {
      const error = getUserFriendlyError('RECEIPT_MISMATCH');

      expect(error.title).toBe('Receipt Verification Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('receipt did not match');
      expect(error.actionText).toBe('Retry with same manifest');
      expect(error.helpUrl).toBe('/docs/troubleshooting#replay');
    });

    it('maps POLICY_DIVERGENCE to policy guidance', () => {
      const error = getUserFriendlyError('POLICY_DIVERGENCE');

      expect(error.title).toBe('Policy Divergence Detected');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('policy check failed');
      expect(error.actionText).toBe('Review policies');
      expect(error.helpUrl).toBe('/docs/policies');
    });

    it('maps BACKEND_FALLBACK to backend guidance', () => {
      const error = getUserFriendlyError('BACKEND_FALLBACK');

      expect(error.title).toBe('Backend Fallback Triggered');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('fell back to a different backend');
      expect(error.actionText).toBe('Review backend settings');
      expect(error.helpUrl).toBe('/docs/backends');
    });

    it('maps TENANT_ACCESS_DENIED to tenant guidance', () => {
      const error = getUserFriendlyError('TENANT_ACCESS_DENIED');

      expect(error.title).toBe('Workspace Access Denied');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('do not have access to this tenant');
      expect(error.actionText).toBe('Select tenant');
      expect(error.helpUrl).toBe('/docs/administration#tenants');
    });
  });

  describe('Adapter-specific errors', () => {
    it('maps ADAPTER_NOT_FOUND without context', () => {
      const error = getUserFriendlyError('ADAPTER_NOT_FOUND');

      expect(error.title).toBe('Adapter Not Found');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('adapter was not found');
      expect(error.actionText).toBe('Refresh List');
      expect(error.helpUrl).toBe('/docs/adapters#managing-adapters');
    });

    it('maps ADAPTER_NOT_FOUND with adapterId context', () => {
      const error = getUserFriendlyError('ADAPTER_NOT_FOUND', undefined, {
        adapterId: 'my-adapter',
      });

      expect(error.message).toContain('my-adapter');
    });

    it('maps ADAPTER_ALREADY_LOADED without context', () => {
      const error = getUserFriendlyError('ADAPTER_ALREADY_LOADED');

      expect(error.title).toBe('Adapter Already Loaded');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('already loaded');
      expect(error.actionText).toBe('Use Adapter');
      expect(error.helpUrl).toBe('/docs/adapters#using-adapters');
    });

    it('maps ADAPTER_ALREADY_LOADED with adapterId context', () => {
      const error = getUserFriendlyError('ADAPTER_ALREADY_LOADED', undefined, {
        adapterId: 'test-adapter',
      });

      expect(error.message).toContain('test-adapter');
    });

    it('maps ADAPTER_LOAD_FAILED without context', () => {
      const error = getUserFriendlyError('ADAPTER_LOAD_FAILED');

      expect(error.title).toBe('Adapter Loading Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('couldn\'t load the adapter');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/adapters#loading-issues');
    });

    it('maps ADAPTER_LOAD_FAILED with adapterId context', () => {
      const error = getUserFriendlyError('ADAPTER_LOAD_FAILED', undefined, {
        adapterId: 'failed-adapter',
      });

      expect(error.message).toContain('failed-adapter');
    });

    it('maps ADAPTER_CORRUPTED without context', () => {
      const error = getUserFriendlyError('ADAPTER_CORRUPTED');

      expect(error.title).toBe('Adapter File Corrupted');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('file appears to be corrupted');
      expect(error.actionText).toBe('Re-upload Adapter');
      expect(error.helpUrl).toBe('/docs/adapters#corrupted-files');
    });

    it('maps ADAPTER_CORRUPTED with adapterId context', () => {
      const error = getUserFriendlyError('ADAPTER_CORRUPTED', undefined, {
        adapterId: 'bad-adapter',
      });

      expect(error.message).toContain('bad-adapter');
    });
  });

  describe('Training errors', () => {
    it('maps TRAINING_FAILED to training guidance', () => {
      const error = getUserFriendlyError('TRAINING_FAILED');

      expect(error.title).toBe('Training Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('training process encountered an error');
      expect(error.actionText).toBe('Check Configuration');
      expect(error.helpUrl).toBe('/docs/training#troubleshooting');
    });

    it('maps DATASET_TRUST_BLOCKED to dataset trust guidance', () => {
      const error = getUserFriendlyError('DATASET_TRUST_BLOCKED');

      expect(error.title).toBe('Dataset blocked by trust gate');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('trust_state is blocked');
      expect(error.actionText).toBe('Review dataset trust');
      expect(error.helpUrl).toBe('/docs/training/aos_adapters');
    });

    it('maps DATASET_TRUST_NEEDS_APPROVAL to approval guidance', () => {
      const error = getUserFriendlyError('DATASET_TRUST_NEEDS_APPROVAL');

      expect(error.title).toBe('Dataset needs approval');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('requires approval or validation');
      expect(error.actionText).toBe('Review dataset validation');
      expect(error.helpUrl).toBe('/docs/training/aos_adapters');
    });

    it('maps INVALID_TRAINING_DATA to data format guidance', () => {
      const error = getUserFriendlyError('INVALID_TRAINING_DATA');

      expect(error.title).toBe('Invalid Training Data');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('training data format is not valid');
      expect(error.actionText).toBe('Fix Data Format');
      expect(error.helpUrl).toBe('/docs/training#data-format');
    });

    it('maps TRAINING_TIMEOUT to timeout guidance', () => {
      const error = getUserFriendlyError('TRAINING_TIMEOUT');

      expect(error.title).toBe('Training Timed Out');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('took too long to complete');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/training#timeouts');
    });
  });

  describe('Model errors', () => {
    it('maps MODEL_NOT_FOUND without context', () => {
      const error = getUserFriendlyError('MODEL_NOT_FOUND');

      expect(error.title).toBe('Model Not Found');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('model was not found');
      expect(error.actionText).toBe('Choose Different Model');
      expect(error.helpUrl).toBe('/docs/models#available-models');
    });

    it('maps MODEL_NOT_FOUND with modelId context', () => {
      const error = getUserFriendlyError('MODEL_NOT_FOUND', undefined, {
        modelId: 'gpt-4',
      });

      expect(error.message).toContain('gpt-4');
    });

    it('maps MODEL_BUSY without context', () => {
      const error = getUserFriendlyError('MODEL_BUSY');

      expect(error.title).toBe('Model In Use');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('currently in use');
      expect(error.actionText).toBe('Try Again Later');
      expect(error.helpUrl).toBe('/docs/models#concurrency');
    });

    it('maps MODEL_BUSY with modelId context', () => {
      const error = getUserFriendlyError('MODEL_BUSY', undefined, {
        modelId: 'llama-2',
      });

      expect(error.message).toContain('llama-2');
    });

    it('maps MODEL_LOAD_FAILED without context', () => {
      const error = getUserFriendlyError('MODEL_LOAD_FAILED');

      expect(error.title).toBe('Model Loading Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('couldn\'t load the model');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/models#loading-issues');
    });

    it('maps MODEL_LOAD_FAILED with modelId context', () => {
      const error = getUserFriendlyError('MODEL_LOAD_FAILED', undefined, {
        modelId: 'mistral-7b',
      });

      expect(error.message).toContain('mistral-7b');
    });

    it('maps LOAD_FAILED to retryable guidance', () => {
      const error = getUserFriendlyError('LOAD_FAILED', undefined, {
        modelId: 'qwen7b',
      });

      expect(error.title).toBe('Model Loading Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('qwen7b');
      expect(error.actionText).toBe('Try Again');
    });

    it('maps LOAD_FAILED without context', () => {
      const error = getUserFriendlyError('LOAD_FAILED');

      expect(error.title).toBe('Model Loading Failed');
      expect(error.message).toContain('couldn\'t load the model');
    });
  });

  describe('File upload errors', () => {
    it('maps FILE_TOO_LARGE without context', () => {
      const error = getUserFriendlyError('FILE_TOO_LARGE');

      expect(error.title).toBe('File Too Large');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('file is too large');
      expect(error.actionText).toBe('Choose Smaller File');
      expect(error.helpUrl).toBe('/docs/uploads#file-limits');
    });

    it('maps FILE_TOO_LARGE with fileSize context', () => {
      const error = getUserFriendlyError('FILE_TOO_LARGE', undefined, {
        fileSize: 104857600, // 100MB in bytes
      });

      expect(error.message).toContain('100MB');
    });

    it('maps INVALID_FILE_FORMAT to format guidance', () => {
      const error = getUserFriendlyError('INVALID_FILE_FORMAT');

      expect(error.title).toBe('Invalid File Format');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('format is not supported');
      expect(error.actionText).toBe('Check Supported Formats');
      expect(error.helpUrl).toBe('/docs/uploads#supported-formats');
    });

    it('maps UPLOAD_FAILED to upload guidance', () => {
      const error = getUserFriendlyError('UPLOAD_FAILED');

      expect(error.title).toBe('Upload Failed');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('file upload failed');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/uploads#troubleshooting');
    });
  });

  describe('Inference errors', () => {
    it('maps INFERENCE_FAILED to inference guidance', () => {
      const error = getUserFriendlyError('INFERENCE_FAILED');

      expect(error.title).toBe('Inference Failed');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('couldn\'t generate a response');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/inference#common-issues');
    });

    it('maps INVALID_PROMPT to prompt guidance', () => {
      const error = getUserFriendlyError('INVALID_PROMPT');

      expect(error.title).toBe('Invalid Input');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('input prompt is not valid');
      expect(error.actionText).toBe('Fix Prompt');
      expect(error.helpUrl).toBe('/docs/inference#input-validation');
    });
  });

  describe('Generic server errors', () => {
    it('maps INTERNAL_SERVER_ERROR to server error guidance', () => {
      const error = getUserFriendlyError('INTERNAL_SERVER_ERROR');

      expect(error.title).toBe('Server Error');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('unexpected server error');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/support');
    });

    it('maps SERVICE_UNAVAILABLE to service guidance', () => {
      const error = getUserFriendlyError('SERVICE_UNAVAILABLE');

      expect(error.title).toBe('Service Unavailable');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('temporarily unavailable');
      expect(error.actionText).toBe('Try Again Later');
      expect(error.helpUrl).toBe('/docs/status');
    });

    it('maps SYSTEM_NOT_READY to startup guidance', () => {
      const error = getUserFriendlyError('SYSTEM_NOT_READY');

      expect(error.title).toBe('System Not Ready');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('still starting up');
      expect(error.actionText).toBe('Check Status');
      expect(error.helpUrl).toBe('/docs/troubleshooting#startup');
    });

    it('maps NO_WORKERS to worker guidance', () => {
      const error = getUserFriendlyError('NO_WORKERS');

      expect(error.title).toBe('No Workers Available');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('No inference workers');
      expect(error.actionText).toBe('Start Worker');
      expect(error.helpUrl).toBe('/docs/quickstart#workers');
    });

    it('maps NO_WORKER_AVAILABLE to worker guidance', () => {
      const error = getUserFriendlyError('NO_WORKER_AVAILABLE');

      expect(error.title).toBe('No Workers Available');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('No inference workers');
      expect(error.actionText).toBe('Start Worker');
      expect(error.helpUrl).toBe('/docs/quickstart#workers');
    });

    it('maps MAINTENANCE to maintenance guidance', () => {
      const error = getUserFriendlyError('MAINTENANCE');

      expect(error.title).toBe('Maintenance In Progress');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('undergoing maintenance');
      expect(error.actionText).toBe('Check Status');
      expect(error.helpUrl).toBe('/docs/status');
    });
  });

  describe('Response parsing errors', () => {
    it('maps PARSE_ERROR to parsing guidance', () => {
      const error = getUserFriendlyError('PARSE_ERROR');

      expect(error.title).toBe('Invalid Server Response');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('unexpected response format');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/troubleshooting#server-errors');
    });

    it('maps RESPONSE_FORMAT_ERROR to format guidance', () => {
      const error = getUserFriendlyError('RESPONSE_FORMAT_ERROR');

      expect(error.title).toBe('Response Format Error');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('unexpected format');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/support');
    });
  });

  describe('Chat-specific errors', () => {
    it('maps WORKER_UNAVAILABLE to inference service guidance', () => {
      const error = getUserFriendlyError('WORKER_UNAVAILABLE');

      expect(error.title).toBe('Inference Service Unavailable');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('inference service is temporarily unavailable');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/chat#inference-service');
    });

    it('maps LOADING_TIMEOUT to loading timeout guidance', () => {
      const error = getUserFriendlyError('LOADING_TIMEOUT');

      expect(error.title).toBe('Loading Timeout');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('took too long to complete');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/chat#timeouts');
    });
  });

  describe('Initial page load errors', () => {
    it('maps INITIAL_LOAD_TIMEOUT to initial load guidance', () => {
      const error = getUserFriendlyError('INITIAL_LOAD_TIMEOUT');

      expect(error.title).toBe('Loading Timeout');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('taking too long to load');
      expect(error.actionText).toBe('Retry All');
      expect(error.helpUrl).toBe('/docs/troubleshooting#timeouts');
    });

    it('maps NO_WORKERS_AVAILABLE to workers guidance', () => {
      const error = getUserFriendlyError('NO_WORKERS_AVAILABLE');

      expect(error.title).toBe('No Workers Available');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('No inference workers are available');
      expect(error.actionText).toBe('View Docs');
      expect(error.helpUrl).toBe('/docs/quickstart#starting-workers');
    });

    it('maps DRAINING to maintenance guidance', () => {
      const error = getUserFriendlyError('DRAINING');

      expect(error.title).toBe('System Maintenance');
      expect(error.variant).toBe('info');
      expect(error.message).toContain('draining for maintenance');
      expect(error.actionText).toBe('Check Status');
      expect(error.helpUrl).toBe('/docs/status');
    });
  });

  describe('Fallback behavior', () => {
    it('returns generic error for unknown error code', () => {
      const error = getUserFriendlyError('UNKNOWN_ERROR_CODE');

      expect(error.title).toBe('Something went wrong');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('unexpected error occurred');
      expect(error.actionText).toBe('Try Again');
      expect(error.helpUrl).toBe('/docs/support');
    });

    it('falls back to HTTP status when error code is not found', () => {
      const error = getUserFriendlyError(undefined, 404);

      expect(error.title).toBe('Not Found');
      expect(error.variant).toBe('warning');
      expect(error.message).toContain('resource was not found');
    });

    it('returns generic error when both error code and HTTP status are missing', () => {
      const error = getUserFriendlyError();

      expect(error.title).toBe('Something went wrong');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('unexpected error occurred');
    });
  });

  describe('Validation_error (commonly used but not in map)', () => {
    it('handles VALIDATION_ERROR as unknown code with fallback', () => {
      const error = getUserFriendlyError('VALIDATION_ERROR');

      expect(error.title).toBe('Something went wrong');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('unexpected error occurred');
    });
  });

  describe('NOT_FOUND error code', () => {
    it('handles NOT_FOUND as unknown code with fallback', () => {
      const error = getUserFriendlyError('NOT_FOUND');

      expect(error.title).toBe('Something went wrong');
      expect(error.variant).toBe('error');
      expect(error.message).toContain('unexpected error occurred');
    });
  });
});
