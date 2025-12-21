import { logger, toError } from '@/utils/logger';
import { ApiError } from '@/api/client';

/**
 * Security: Input sanitization to prevent XSS and other injection attacks.
 * Removes potentially dangerous HTML/script content from user input.
 */
export const sanitizeInput = (input: string): string => {
  if (!input) return input;

  // Basic XSS prevention - remove potentially dangerous HTML/script tags
  const sanitized = input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '') // Remove script tags
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '') // Remove iframe tags
    .replace(/javascript:/gi, '') // Remove javascript: protocols
    .replace(/on\w+\s*=/gi, '') // Remove event handlers
    .replace(/<[^>]*>/g, '') // Remove all HTML tags as final fallback
    .trim();

  // Log if input was modified for security monitoring
  if (sanitized !== input) {
    logger.warn('Input sanitized for security', {
      component: 'InferencePlayground',
      operation: 'input_sanitization',
      originalLength: input.length,
      sanitizedLength: sanitized.length,
    });
  }

  return sanitized;
};

/**
 * Privacy-aware monitoring (anonymized metrics only).
 * Removes any personally identifiable information before logging.
 */
export const recordPrivacySafeMetrics = (
  operation: string,
  data: Record<string, unknown>
): void => {
  // Remove any personally identifiable information
  const anonymized = { ...data };
  delete anonymized.userId;
  delete anonymized.email;
  delete anonymized.ip;
  delete anonymized.sessionId;

  logger.info(`Privacy-safe ${operation}`, {
    component: 'InferencePlayground',
    operation: `privacy_${operation}`,
    ...anonymized,
  });
};

/**
 * Format a status value for display.
 * Converts snake_case to Title Case and handles empty values.
 */
export const formatStatusLabel = (
  value?: string,
  fallback: string = 'Unknown'
): string => {
  if (!value) return fallback;
  const normalized = value.replace(/_/g, ' ').trim();
  if (!normalized) return fallback;
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
};

/**
 * Extract a user-friendly error message from a CoreML API error.
 */
export const extractCoremlErrorMessage = (
  error: unknown,
  fallback: string
): string => {
  const apiErr = error as ApiError;
  if (apiErr?.detail) return apiErr.detail;
  if (apiErr?.message) return apiErr.message;
  const parsed = toError(error);
  return parsed.message || fallback;
};

/**
 * Determine the badge variant for CoreML export status.
 */
export const getExportBadgeVariant = (
  isEnabled: boolean,
  status?: string,
  isAvailable?: boolean
): 'default' | 'secondary' | 'destructive' | 'outline' => {
  if (!isEnabled) return 'outline';
  if (status === 'failed') return 'destructive';
  if (status === 'pending') return 'secondary';
  if (isAvailable) return 'default';
  return 'outline';
};

/**
 * Determine the badge variant for CoreML verification status.
 */
export const getVerificationBadgeVariant = (
  isEnabled: boolean,
  hasMismatch: boolean,
  status?: string,
  isVerified?: boolean
): 'default' | 'secondary' | 'destructive' | 'outline' => {
  if (!isEnabled) return 'outline';
  if (hasMismatch || status === 'failed') return 'destructive';
  if (status === 'pending') return 'secondary';
  if (isVerified || status === 'passed') return 'default';
  return 'outline';
};

/**
 * Get display label for CoreML export status.
 */
export const getExportStatusLabel = (
  isEnabled: boolean,
  status?: string,
  isAvailable?: boolean
): string => {
  if (!isEnabled) return 'Not supported yet';
  if (status) {
    return formatStatusLabel(status, isAvailable ? 'Ready' : 'Not exported');
  }
  return isAvailable ? 'Ready' : 'Not exported';
};

/**
 * Get display label for CoreML verification status.
 */
export const getVerificationStatusLabel = (
  isEnabled: boolean,
  hasMismatch: boolean,
  status?: string,
  isVerified?: boolean
): string => {
  if (!isEnabled) return 'Not supported yet';
  if (hasMismatch) return 'Mismatch';
  if (status) {
    return formatStatusLabel(status, isVerified ? 'Passed' : 'Not verified');
  }
  return isVerified ? 'Passed' : 'Not verified';
};
