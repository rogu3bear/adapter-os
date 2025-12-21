/**
 * ChatErrorDisplay - User-friendly error display for chat loading errors
 *
 * Displays contextual error messages with:
 * - Error type icons (AlertTriangle for general, WifiOff for network, HardDrive for memory)
 * - User-friendly error messages with actionable suggestions
 * - Collapsible technical details section
 * - Retry button with countdown timer
 * - Retry attempt tracking: "Attempt X of Y"
 * - Help links to documentation
 * - Alternative action buttons (e.g., "Try different stack", "Free memory")
 * - ARIA: role="alert", aria-live="assertive"
 *
 * Supports both AutoLoadError (legacy) and ChatLoadingError (new model-loading hooks).
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import React, { useState, useEffect } from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { AlertTriangle, WifiOff, HardDrive, RefreshCw, ExternalLink, ChevronDown, ChevronUp } from 'lucide-react';
import type { AutoLoadError } from '@/hooks/model-loading';
import type { ChatLoadingError } from '@/hooks/model-loading/types';
import { logger } from '@/utils/logger';

/** Union error type supporting both legacy and new error formats */
export type ChatError = AutoLoadError | ChatLoadingError;

/** Type guard to check if error is ChatLoadingError */
function isChatLoadingError(error: ChatError): error is ChatLoadingError {
  return 'retryable' in error && 'maxRetries' in error;
}

/** Normalize error to common format */
function normalizeError(error: ChatError): {
  code: string;
  message: string;
  retryCount: number;
  canRetry: boolean;
  maxRetries: number;
  suggestion?: string;
} {
  if (isChatLoadingError(error)) {
    return {
      code: error.code,
      message: error.message,
      retryCount: error.retryCount,
      canRetry: error.retryable && error.retryCount < error.maxRetries,
      maxRetries: error.maxRetries,
      suggestion: error.suggestion,
    };
  }
  // AutoLoadError format
  return {
    code: error.code,
    message: error.message,
    retryCount: error.retryCount,
    canRetry: error.canRetry,
    maxRetries: 3, // Default for legacy format
    suggestion: undefined,
  };
}

export interface ChatErrorDisplayProps {
  /** The error object containing code, message, and retry information */
  error: ChatError;
  /** Callback to trigger retry */
  onRetry?: () => void;
  /** Optional countdown in seconds until next retry */
  retryCountdown?: number;
  /** Maximum number of retries allowed (overrides error.maxRetries) */
  maxRetries?: number;
  /** Current retry attempt (1-based, overrides error.retryCount) */
  currentRetry?: number;
  /** Alternative action buttons */
  alternativeActions?: Array<{
    label: string;
    onClick: () => void;
    variant?: 'default' | 'outline' | 'destructive' | 'secondary' | 'ghost' | 'link';
  }>;
  /** Custom help URL (overrides default based on error code) */
  helpUrl?: string;
  /** Additional CSS classes */
  className?: string;
}

// Map error codes to icons and help URLs
const ERROR_CONFIG: Record<
  string,
  {
    icon: React.ElementType;
    iconColor: string;
    helpUrl: string;
    actionSuggestion?: string;
  }
> = {
  // Model loading error codes
  BASE_MODEL_NOT_READY: {
    icon: AlertTriangle,
    iconColor: 'text-yellow-500',
    helpUrl: '/docs/models#loading',
    actionSuggestion: 'Wait for base model to load',
  },
  BASE_MODEL_LOAD_FAILED: {
    icon: AlertTriangle,
    iconColor: 'text-red-500',
    helpUrl: '/docs/troubleshooting#loading-issues',
    actionSuggestion: 'Check model configuration',
  },
  ADAPTER_LOAD_FAILED: {
    icon: AlertTriangle,
    iconColor: 'text-red-500',
    helpUrl: '/docs/adapters#troubleshooting',
    actionSuggestion: 'Check adapter configuration',
  },
  ADAPTER_NOT_FOUND: {
    icon: AlertTriangle,
    iconColor: 'text-gray-500',
    helpUrl: '/docs/adapters#importing',
    actionSuggestion: 'Verify adapter exists',
  },
  STACK_NOT_FOUND: {
    icon: AlertTriangle,
    iconColor: 'text-gray-500',
    helpUrl: '/docs/stacks#creating',
    actionSuggestion: 'Select a valid stack',
  },
  MEMORY_INSUFFICIENT: {
    icon: HardDrive,
    iconColor: 'text-red-500',
    helpUrl: '/docs/models#memory-management',
    actionSuggestion: 'Free up system memory',
  },
  // Legacy error codes
  NETWORK_ERROR: {
    icon: WifiOff,
    iconColor: 'text-orange-500',
    helpUrl: '/docs/troubleshooting#network-issues',
    actionSuggestion: 'Check your network connection',
  },
  TIMEOUT: {
    icon: AlertTriangle,
    iconColor: 'text-yellow-500',
    helpUrl: '/docs/troubleshooting#timeouts',
    actionSuggestion: 'The server may be busy',
  },
  OUT_OF_MEMORY: {
    icon: HardDrive,
    iconColor: 'text-red-500',
    helpUrl: '/docs/models#memory-management',
    actionSuggestion: 'Free up system memory',
  },
  ALREADY_LOADING: {
    icon: AlertTriangle,
    iconColor: 'text-blue-500',
    helpUrl: '/docs/models#loading',
    actionSuggestion: 'Wait for current operation to complete',
  },
  NO_MODELS: {
    icon: AlertTriangle,
    iconColor: 'text-gray-500',
    helpUrl: '/docs/models#importing',
    actionSuggestion: 'Import a model first',
  },
  LOAD_FAILED: {
    icon: AlertTriangle,
    iconColor: 'text-red-500',
    helpUrl: '/docs/troubleshooting#loading-issues',
    actionSuggestion: 'Check model configuration',
  },
  UNKNOWN: {
    icon: AlertTriangle,
    iconColor: 'text-gray-500',
    helpUrl: '/docs/support',
    actionSuggestion: 'Contact support if issue persists',
  },
};

export function ChatErrorDisplay({
  error,
  onRetry,
  retryCountdown,
  maxRetries: maxRetriesOverride,
  currentRetry: currentRetryOverride,
  alternativeActions,
  helpUrl,
  className = '',
}: ChatErrorDisplayProps) {
  const [showTechnicalDetails, setShowTechnicalDetails] = useState(false);
  const [countdown, setCountdown] = useState(retryCountdown ?? 0);

  // Normalize error to common format
  const normalizedError = normalizeError(error);
  const maxRetries = maxRetriesOverride ?? normalizedError.maxRetries;
  const currentRetry = currentRetryOverride ?? normalizedError.retryCount;

  // Update countdown timer
  useEffect(() => {
    if (retryCountdown !== undefined && retryCountdown > 0) {
      setCountdown(retryCountdown);
      const interval = window.setInterval(() => {
        setCountdown((prev) => {
          const next = Math.max(prev - 1, 0);
          if (next === 0) {
            window.clearInterval(interval);
          }
          return next;
        });
      }, 1000);

      return () => window.clearInterval(interval);
    }
    return undefined;
  }, [retryCountdown]);

  const config = ERROR_CONFIG[normalizedError.code] || ERROR_CONFIG.UNKNOWN;
  const Icon = config.icon;
  const finalHelpUrl = helpUrl || config.helpUrl;
  const suggestion = normalizedError.suggestion || config.actionSuggestion;

  // Determine if retry is possible
  const canRetry = normalizedError.canRetry && onRetry && currentRetry < maxRetries;
  const isRetrying = countdown > 0;

  // Log error display for debugging
  useEffect(() => {
    logger.error('ChatErrorDisplay shown', {
      component: 'ChatErrorDisplay',
      errorCode: error.code,
      errorMessage: error.message,
      retryCount: currentRetry,
      maxRetries,
      canRetry,
    });
  }, [error, currentRetry, maxRetries, canRetry]);

  return (
    <Alert
      variant="destructive"
      className={`${className}`}
      role="alert"
      aria-live="assertive"
      aria-atomic="true"
    >
      <Icon className={`h-4 w-4 ${config.iconColor}`} aria-hidden="true" />
      <AlertTitle className="font-semibold">
        {error.code === 'NETWORK_ERROR'
          ? 'Connection Problem'
          : error.code === 'TIMEOUT'
            ? 'Request Timed Out'
            : error.code === 'OUT_OF_MEMORY'
              ? 'Not Enough Memory'
              : error.code === 'ALREADY_LOADING'
                ? 'Operation In Progress'
                : error.code === 'NO_MODELS'
                  ? 'No Models Available'
                  : error.code === 'LOAD_FAILED'
                    ? 'Loading Failed'
                    : 'Error'}
      </AlertTitle>

      <AlertDescription className="mt-2 space-y-3">
        {/* User-friendly message */}
        <p className="text-sm">{error.message}</p>

        {/* Action suggestion */}
        {config.actionSuggestion && (
          <p className="text-sm text-muted-foreground italic">
            Suggestion: {config.actionSuggestion}
          </p>
        )}

        {/* Retry status */}
        {currentRetry > 0 && (
          <p className="text-xs text-muted-foreground">
            Attempt {currentRetry} of {maxRetries}
          </p>
        )}

        {/* Technical details (collapsible) */}
        {error.code !== 'NO_MODELS' && error.code !== 'ALREADY_LOADING' && (
          <div className="border-t border-destructive/20 pt-2">
            <button
              onClick={() => setShowTechnicalDetails(!showTechnicalDetails)}
              className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
              aria-expanded={showTechnicalDetails}
              aria-controls="technical-details"
            >
              {showTechnicalDetails ? (
                <ChevronUp className="h-3 w-3" aria-hidden="true" />
              ) : (
                <ChevronDown className="h-3 w-3" aria-hidden="true" />
              )}
              <span>Technical details</span>
            </button>

            {showTechnicalDetails && (
              <div
                id="technical-details"
                className="mt-2 p-2 bg-destructive/5 rounded text-xs font-mono overflow-auto max-h-32"
              >
                <div>
                  <strong>Error Code:</strong> {error.code}
                </div>
                <div className="mt-1">
                  <strong>Message:</strong> {error.message}
                </div>
                {error.retryCount > 0 && (
                  <div className="mt-1">
                    <strong>Retries:</strong> {error.retryCount}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex flex-wrap gap-2 pt-2">
          {/* Retry button */}
          {canRetry && (
            <Button
              variant="outline"
              size="sm"
              onClick={onRetry}
              disabled={isRetrying}
              className="gap-1.5"
              aria-label={isRetrying ? `Retrying in ${countdown} seconds` : 'Retry loading model'}
            >
              <RefreshCw
                className={`h-3.5 w-3.5 ${isRetrying ? 'animate-spin' : ''}`}
                aria-hidden="true"
              />
              {isRetrying ? `Retry in ${countdown}s` : 'Try Again'}
            </Button>
          )}

          {/* Alternative action buttons */}
          {alternativeActions?.map((action, index) => (
            <Button
              key={index}
              variant={action.variant || 'outline'}
              size="sm"
              onClick={action.onClick}
              className="gap-1.5"
            >
              {action.label}
            </Button>
          ))}

          {/* Help link */}
          <Button
            variant="ghost"
            size="sm"
            asChild
            className="gap-1.5 ml-auto"
          >
            <a
              href={finalHelpUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label="Get help with this error"
            >
              <span className="text-xs">Get Help</span>
              <ExternalLink className="h-3 w-3" aria-hidden="true" />
            </a>
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  );
}
