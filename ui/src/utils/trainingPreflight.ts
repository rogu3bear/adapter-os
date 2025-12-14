/**
 * Training Preflight Checks
 *
 * Client-side validation for training jobs before submission.
 * Provides instant feedback on dataset readiness.
 */

import type { Dataset, TrustState } from '@/api/training-types';
import type { PolicyCheck } from '@/components/PolicyPreflightDialog';

/** Size limit warning threshold (1GB) */
const SIZE_WARNING_BYTES = 1024 * 1024 * 1024;

/** Size limit error threshold (10GB) */
const SIZE_ERROR_BYTES = 10 * 1024 * 1024 * 1024;

/**
 * Allowed trust states for training
 */
const ALLOWED_TRUST_STATES: TrustState[] = ['allowed', 'allowed_with_warning'];

/**
 * Format bytes to human-readable string
 */
function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} bytes`;
}

export interface ClientPreflightResult {
  /** Whether all critical checks passed */
  passed: boolean;
  /** Whether all checks passed (including warnings) */
  clean: boolean;
  /** Individual check results */
  checks: PolicyCheck[];
}

/**
 * Run client-side preflight checks on a dataset
 *
 * @param dataset - The dataset to validate
 * @returns Preflight result with individual check statuses
 */
export function runClientPreflight(dataset: Dataset): ClientPreflightResult {
  const checks: PolicyCheck[] = [];

  // 1. Validation status check
  const isValidated = dataset.validation_status === 'valid';
  checks.push({
    policy_id: 'validation_status',
    policy_name: 'Dataset Validated',
    passed: isValidated,
    severity: isValidated ? 'info' : 'error',
    message: isValidated
      ? 'Dataset has passed validation'
      : `Dataset status: ${dataset.validation_status}`,
    details: !isValidated
      ? 'Run validation from the Dataset Details page before training.'
      : undefined,
  });

  // 2. Trust state check
  const trustState = dataset.trust_state;
  const isTrustAllowed = trustState && ALLOWED_TRUST_STATES.includes(trustState);
  const isTrustWarning = trustState === 'allowed_with_warning';

  checks.push({
    policy_id: 'trust_state',
    policy_name: 'Trust State',
    passed: !!isTrustAllowed,
    severity: !isTrustAllowed ? 'error' : isTrustWarning ? 'warning' : 'info',
    message: isTrustAllowed
      ? isTrustWarning
        ? `Trust: ${trustState} - Proceed with caution`
        : `Trust: ${trustState}`
      : `Trust blocked: ${trustState || 'unknown'}`,
    details: !isTrustAllowed
      ? dataset.trust_reason || 'Request trust approval or review dataset contents.'
      : isTrustWarning
        ? dataset.trust_reason || 'Dataset has minor trust concerns but is allowed for training.'
        : undefined,
  });

  // 3. File count check
  const hasFiles = dataset.file_count > 0;
  checks.push({
    policy_id: 'file_count',
    policy_name: 'Files Present',
    passed: hasFiles,
    severity: hasFiles ? 'info' : 'error',
    message: hasFiles
      ? `${dataset.file_count} file${dataset.file_count !== 1 ? 's' : ''} in dataset`
      : 'No files found in dataset',
    details: !hasFiles ? 'Add files to the dataset before training.' : undefined,
  });

  // 4. Size check (warning for large, error for huge)
  const sizeBytes = dataset.total_size_bytes || 0;
  const isSizeError = sizeBytes > SIZE_ERROR_BYTES;
  const isSizeWarning = sizeBytes > SIZE_WARNING_BYTES && !isSizeError;
  const isSizeOk = !isSizeError && !isSizeWarning;

  checks.push({
    policy_id: 'size_limit',
    policy_name: 'Dataset Size',
    passed: !isSizeError,
    severity: isSizeError ? 'error' : isSizeWarning ? 'warning' : 'info',
    message: isSizeError
      ? `Dataset too large: ${formatBytes(sizeBytes)} (max 10GB)`
      : isSizeWarning
        ? `Large dataset: ${formatBytes(sizeBytes)} - may take longer`
        : `Size: ${formatBytes(sizeBytes)}`,
    details: isSizeError
      ? 'Split the dataset into smaller parts or remove unnecessary files.'
      : isSizeWarning
        ? 'Training may take significantly longer with large datasets.'
        : undefined,
  });

  // 5. Token count check (sanity check)
  const hasTokens = dataset.total_tokens > 0;
  if (!hasTokens && hasFiles) {
    checks.push({
      policy_id: 'token_count',
      policy_name: 'Token Count',
      passed: false,
      severity: 'warning',
      message: 'No tokens detected in dataset',
      details:
        'The dataset may not be properly tokenized. Validation should detect this, but training may fail if tokens are missing.',
    });
  }

  // Calculate overall results
  const hasError = checks.some((c) => !c.passed && c.severity === 'error');
  const hasWarning = checks.some((c) => c.severity === 'warning');

  return {
    passed: !hasError,
    clean: !hasError && !hasWarning,
    checks,
  };
}

/**
 * Check if a dataset is ready for quick training (bypassing wizard)
 *
 * @param dataset - The dataset to check
 * @returns true if dataset can use quick train modal
 */
export function canUseQuickTrain(dataset: Dataset): boolean {
  const preflight = runClientPreflight(dataset);
  return preflight.passed;
}

/**
 * Get a summary message for preflight results
 */
export function getPreflightSummary(result: ClientPreflightResult): string {
  if (result.clean) {
    return 'All checks passed. Ready to start training.';
  }
  if (result.passed) {
    const warnings = result.checks.filter((c) => c.severity === 'warning').length;
    return `Ready with ${warnings} warning${warnings !== 1 ? 's' : ''}. Review before proceeding.`;
  }
  const errors = result.checks.filter((c) => !c.passed && c.severity === 'error').length;
  return `${errors} issue${errors !== 1 ? 's' : ''} must be resolved before training.`;
}
