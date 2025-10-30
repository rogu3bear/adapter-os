//! Error Recovery Component
//!
//! Provides comprehensive error handling with recovery paths and trust-building messaging.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L300-L350 - Error recovery UX patterns
//! - ui/src/utils/logger.ts L1-L50 - Error handling patterns

import React from 'react';
import { Alert, AlertDescription, AlertTitle } from './alert';
import { Button } from './button';
import { AlertTriangle, RefreshCw, ArrowRight, HelpCircle, Home } from 'lucide-react';

export interface RecoveryAction {
  label: string;
  action: () => void | Promise<void>;
  variant?: 'default' | 'outline' | 'secondary';
  primary?: boolean;
}

export interface ErrorRecoveryProps {
  title?: string;
  message: string;
  error?: Error;
  recoveryActions?: RecoveryAction[];
  showHelp?: boolean;
  helpUrl?: string;
  variant?: 'error' | 'warning' | 'info';
  className?: string;
}

export function ErrorRecovery({
  title = 'Something went wrong',
  message,
  error,
  recoveryActions = [],
  showHelp = true,
  helpUrl,
  variant = 'error',
  className = ''
}: ErrorRecoveryProps) {
  const getIcon = () => {
    switch (variant) {
      case 'warning':
        return <AlertTriangle className="h-5 w-5 text-amber-600" />;
      case 'info':
        return <AlertTriangle className="h-5 w-5 text-blue-600" />;
      default:
        return <AlertTriangle className="h-5 w-5 text-red-600" />;
    }
  };

  const getAlertClass = () => {
    switch (variant) {
      case 'warning':
        return 'border-amber-200 bg-amber-50';
      case 'info':
        return 'border-blue-200 bg-blue-50';
      default:
        return 'border-red-200 bg-red-50';
    }
  };

  const getTitleClass = () => {
    switch (variant) {
      case 'warning':
        return 'text-amber-800';
      case 'info':
        return 'text-blue-800';
      default:
        return 'text-red-800';
    }
  };

  const getMessageClass = () => {
    switch (variant) {
      case 'warning':
        return 'text-amber-700';
      case 'info':
        return 'text-blue-700';
      default:
        return 'text-red-700';
    }
  };

  return (
    <Alert className={`${getAlertClass()} ${className}`}>
      {getIcon()}
      <div className="flex-1">
        <AlertTitle className={`${getTitleClass()} font-semibold`}>
          {title}
        </AlertTitle>
        <AlertDescription className={`mt-1 ${getMessageClass()}`}>
          {message}

          {error && (
            <details className="mt-2">
              <summary className="cursor-pointer text-sm font-medium">
                Technical Details
              </summary>
              <pre className="mt-1 text-xs bg-background/50 p-2 rounded border overflow-auto max-h-32">
                {error.message}
                {error.stack && (
                  <>
                    {'\n\nStack Trace:'}
                    {error.stack}
                  </>
                )}
              </pre>
            </details>
          )}

          {(recoveryActions.length > 0 || showHelp) && (
            <div className="mt-3 flex flex-wrap gap-2">
              {recoveryActions.map((action, index) => (
                <Button
                  key={index}
                  variant={action.variant || (action.primary ? 'default' : 'outline')}
                  size="sm"
                  onClick={action.action}
                  className="text-xs"
                >
                  {action.label}
                  {action.primary && <ArrowRight className="h-3 w-3 ml-1" />}
                </Button>
              ))}

              {showHelp && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    if (helpUrl) {
                      window.open(helpUrl, '_blank');
                    } else {
                      // Default help action - could open help modal
                      console.log('Help requested');
                    }
                  }}
                  className="text-xs"
                >
                  <HelpCircle className="h-3 w-3 mr-1" />
                  Get Help
                </Button>
              )}
            </div>
          )}
        </AlertDescription>
      </div>
    </Alert>
  );
}

// Pre-configured error recovery templates for common scenarios
export const ErrorRecoveryTemplates = {
  networkError: (retryAction?: () => void) => (
    <ErrorRecovery
      title="Connection Problem"
      message="We're having trouble connecting to the server. This is usually temporary."
      recoveryActions={
        retryAction
          ? [
              { label: 'Try Again', action: retryAction, primary: true },
              { label: 'Check Status', action: () => window.location.href = '/dashboard' }
            ]
          : []
      }
      helpUrl="/docs/troubleshooting#network-issues"
    />
  ),

  adapterLoadError: (adapterName: string, retryAction?: () => void) => (
    <ErrorRecovery
      title="Adapter Loading Failed"
      message={`We couldn't load the adapter "${adapterName}". This might be due to insufficient memory or a corrupted adapter file.`}
      recoveryActions={
        retryAction
          ? [
              { label: 'Retry Loading', action: retryAction },
              { label: 'Free Memory', action: () => window.location.href = '/adapters' },
              { label: 'Check Logs', action: () => window.location.href = '/telemetry' }
            ]
          : []
      }
      helpUrl="/docs/adapters#loading-issues"
    />
  ),

  trainingError: (retryAction?: () => void, alternativeAction?: () => void) => (
    <ErrorRecovery
      title="Training Failed"
      message="The adapter training process encountered an error. This could be due to insufficient resources, invalid data, or configuration issues."
      recoveryActions={
        retryAction && alternativeAction
          ? [
              { label: 'Retry Training', action: retryAction },
              { label: 'Adjust Settings', action: alternativeAction },
              { label: 'View Logs', action: () => window.location.href = '/telemetry' }
            ]
          : []
      }
      helpUrl="/docs/training#troubleshooting"
    />
  ),

  inferenceError: (retryAction?: () => void) => (
    <ErrorRecovery
      title="Inference Failed"
      message="We couldn't generate a response. This might be due to model issues, resource constraints, or invalid input."
      variant="warning"
      recoveryActions={
        retryAction
          ? [
              { label: 'Try Again', action: retryAction, primary: true },
              { label: 'Simplify Prompt', action: () => {/* Could focus input */} },
              { label: 'Check Model Status', action: () => window.location.href = '/adapters' }
            ]
          : []
      }
      helpUrl="/docs/inference#common-issues"
    />
  ),

  permissionError: () => (
    <ErrorRecovery
      title="Permission Denied"
      message="You don't have the required permissions to perform this action. Please contact your administrator."
      variant="warning"
      recoveryActions={[
        { label: 'Go to Dashboard', action: () => window.location.href = '/dashboard' }
      ]}
      showHelp={false}
    />
  ),

  genericError: (error?: Error, retryAction?: () => void) => (
    <ErrorRecovery
      message="An unexpected error occurred. Our team has been notified and is working to resolve this."
      error={error}
      recoveryActions={
        retryAction
          ? [
              { label: 'Try Again', action: retryAction },
              { label: 'Go Home', action: () => window.location.href = '/dashboard' }
            ]
          : [
              { label: 'Go Home', action: () => window.location.href = '/dashboard', primary: true }
            ]
      }
      helpUrl="/docs/support"
    />
  )
};

export default ErrorRecovery;
