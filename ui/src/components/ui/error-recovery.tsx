import { Alert, AlertDescription } from '@/components/ui/alert';

interface ErrorRecoveryProps {
  error: string;
  onRetry: () => void;
}

export const ErrorRecovery = ({ error, onRetry }: ErrorRecoveryProps) => (
  <Alert variant="destructive">
    <AlertDescription>{error}</AlertDescription>
    <button onClick={onRetry}>Retry</button>
  </Alert>
);

export const ErrorRecoveryTemplates = {
  networkError: (onRetry: () => void) => ({
    error: 'Network error occurred. Please check your connection and try again.',
    onRetry,
  }),

  authError: (onRetry: () => void) => ({
    error: 'Authentication failed. Please log in again.',
    onRetry,
  }),

  validationError: (message: string, onRetry: () => void) => ({
    error: `Validation error: ${message}`,
    onRetry,
  }),

  genericError: (message: string, onRetry: () => void) => ({
    error: message || 'An unexpected error occurred. Please try again.',
    onRetry,
  }),

  timeoutError: (onRetry: () => void) => ({
    error: 'Request timed out. Please try again.',
    onRetry,
  }),

  notFoundError: (resource: string, onRetry: () => void) => ({
    error: `${resource} not found.`,
    onRetry,
  }),

  permissionError: (onRetry: () => void) => ({
    error: 'You do not have permission to perform this action.',
    onRetry,
  }),
};
