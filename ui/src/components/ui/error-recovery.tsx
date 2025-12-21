import React from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

interface ErrorRecoveryProps {
  error: string;
  onRetry: () => void;
}

export const ErrorRecovery = ({ error, onRetry }: ErrorRecoveryProps) => (
  <Alert variant="destructive">
    <AlertDescription className="flex items-center justify-between">
      <span>{error}</span>
      <Button variant="outline" size="sm" onClick={onRetry}>Retry</Button>
    </AlertDescription>
  </Alert>
);

export const errorRecoveryTemplates = {
  networkError: (onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error="Network error occurred. Please check your connection and try again."
      onRetry={onRetry}
    />
  ),

  authError: (onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error="Authentication failed. Please log in again."
      onRetry={onRetry}
    />
  ),

  validationError: (message: string, onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error={`Validation error: ${message}`}
      onRetry={onRetry}
    />
  ),

  genericError: (error: Error | string, onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error={error instanceof Error ? error.message : (error || 'An unexpected error occurred. Please try again.')}
      onRetry={onRetry}
    />
  ),

  timeoutError: (onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error="Request timed out. Please try again."
      onRetry={onRetry}
    />
  ),

  notFoundError: (resource: string, onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error={`${resource} not found.`}
      onRetry={onRetry}
    />
  ),

  permissionError: (onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error="You do not have permission to perform this action."
      onRetry={onRetry}
    />
  ),

  pollingError: (errorMessage: string, onRetry: () => void): React.ReactElement => (
    <ErrorRecovery
      error={errorMessage || "Failed to fetch latest data. Will retry automatically."}
      onRetry={onRetry}
    />
  ),

  trainingError: (onRetry: () => void, onAlternate?: () => void): React.ReactElement => (
    <ErrorRecovery
      error="Training operation failed. Please check your configuration and try again."
      onRetry={onRetry}
    />
  ),
};
