"use client";

import * as React from "react";
import { AlertCircle, RefreshCw, ChevronDown, ChevronUp, Copy, Check } from "lucide-react";
import { cn } from "@/components/ui/utils";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

export interface ErrorDisplayProps {
  error: Error | string | null;
  title?: string;
  onRetry?: () => void;
  retryLabel?: string;
  showDetails?: boolean;
  dismissible?: boolean;
  onDismiss?: () => void;
  variant?: "inline" | "card" | "banner";
  className?: string;
}

export function ErrorDisplay({
  error,
  title = "Error",
  onRetry,
  retryLabel = "Try Again",
  showDetails = false,
  dismissible = false,
  onDismiss,
  variant = "inline",
  className,
}: ErrorDisplayProps) {
  const [dismissed, setDismissed] = React.useState(false);
  const [detailsExpanded, setDetailsExpanded] = React.useState(false);
  const [copied, setCopied] = React.useState(false);

  if (dismissed || !error) return null;

  const errorMessage = error instanceof Error ? error.message : error;
  const errorStack = error instanceof Error ? error.stack : undefined;

  const handleDismiss = () => {
    setDismissed(true);
    onDismiss?.();
  };

  const handleCopy = async () => {
    const textToCopy = errorStack || errorMessage;
    await navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (variant === "card") {
    return (
      <div
        className={cn(
          "rounded-lg border border-destructive/50 bg-destructive/5 p-4",
          className
        )}
      >
        <div className="flex items-start gap-3">
          <AlertCircle className="h-5 w-5 text-destructive shrink-0 mt-0.5" />
          <div className="flex-1 min-w-0">
            <h4 className="text-sm font-semibold text-destructive">{title}</h4>
            <p className="text-sm text-destructive/90 mt-1">{errorMessage}</p>

            {showDetails && errorStack && (
              <div className="mt-3">
                <button
                  type="button"
                  onClick={() => setDetailsExpanded(!detailsExpanded)}
                  className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                >
                  {detailsExpanded ? (
                    <ChevronUp className="h-3 w-3" />
                  ) : (
                    <ChevronDown className="h-3 w-3" />
                  )}
                  {detailsExpanded ? "Hide details" : "Show details"}
                </button>

                {detailsExpanded && (
                  <div className="mt-2 relative">
                    <pre className="text-xs font-mono bg-muted p-2 rounded-md overflow-auto max-h-32 whitespace-pre-wrap break-all">
                      {errorStack}
                    </pre>
                    <button
                      type="button"
                      onClick={handleCopy}
                      className="absolute top-1 right-1 p-1 rounded hover:bg-background/50 transition-colors"
                      aria-label="Copy error details"
                    >
                      {copied ? (
                        <Check className="h-3 w-3 text-green-500" />
                      ) : (
                        <Copy className="h-3 w-3 text-muted-foreground" />
                      )}
                    </button>
                  </div>
                )}
              </div>
            )}

            <div className="flex items-center gap-2 mt-3">
              {onRetry && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onRetry}
                  className="h-7 text-xs"
                >
                  <RefreshCw className="h-3 w-3 mr-1" />
                  {retryLabel}
                </Button>
              )}
              {dismissible && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleDismiss}
                  className="h-7 text-xs"
                >
                  Dismiss
                </Button>
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (variant === "banner") {
    return (
      <div
        className={cn(
          "w-full bg-destructive/10 border-b border-destructive/20 px-4 py-2",
          className
        )}
      >
        <div className="flex items-center justify-between gap-4 max-w-7xl mx-auto">
          <div className="flex items-center gap-2 min-w-0">
            <AlertCircle className="h-4 w-4 text-destructive shrink-0" />
            <span className="text-sm text-destructive truncate">
              {errorMessage}
            </span>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {onRetry && (
              <Button
                variant="ghost"
                size="sm"
                onClick={onRetry}
                className="h-6 px-2 text-xs text-destructive hover:text-destructive"
              >
                <RefreshCw className="h-3 w-3 mr-1" />
                Retry
              </Button>
            )}
            {dismissible && (
              <Button
                variant="ghost"
                size="sm"
                onClick={handleDismiss}
                className="h-6 px-2 text-xs"
              >
                Dismiss
              </Button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // Default inline variant using Alert component
  return (
    <Alert variant="destructive" className={className}>
      <AlertCircle className="h-4 w-4" />
      <AlertTitle className="flex items-center justify-between">
        <span>{title}</span>
        <div className="flex items-center gap-2">
          {onRetry && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onRetry}
              className="h-6 px-2 text-xs"
            >
              <RefreshCw className="h-3 w-3 mr-1" />
              {retryLabel}
            </Button>
          )}
          {dismissible && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleDismiss}
              className="h-6 w-6 p-0"
              aria-label="Dismiss"
            >
              &times;
            </Button>
          )}
        </div>
      </AlertTitle>
      <AlertDescription>
        {errorMessage}
        {showDetails && errorStack && (
          <details className="mt-2">
            <summary className="text-xs cursor-pointer text-muted-foreground hover:text-foreground">
              Show stack trace
            </summary>
            <pre className="mt-1 text-xs font-mono bg-muted p-2 rounded-md overflow-auto max-h-32 whitespace-pre-wrap break-all">
              {errorStack}
            </pre>
          </details>
        )}
      </AlertDescription>
    </Alert>
  );
}

// Pre-configured error display templates
export const errorTemplates = {
  networkError: (onRetry?: () => void) => (
    <ErrorDisplay
      error="Network error occurred. Please check your connection and try again."
      title="Connection Error"
      onRetry={onRetry}
    />
  ),

  authError: (onRetry?: () => void) => (
    <ErrorDisplay
      error="Your session has expired. Please log in again."
      title="Authentication Error"
      onRetry={onRetry}
      retryLabel="Log In"
    />
  ),

  notFoundError: (resource: string) => (
    <ErrorDisplay
      error={`The requested ${resource} could not be found.`}
      title="Not Found"
    />
  ),

  permissionError: () => (
    <ErrorDisplay
      error="You do not have permission to perform this action."
      title="Permission Denied"
    />
  ),

  validationError: (message: string, onRetry?: () => void) => (
    <ErrorDisplay
      error={message}
      title="Validation Error"
      onRetry={onRetry}
      retryLabel="Fix & Retry"
    />
  ),

  serverError: (onRetry?: () => void) => (
    <ErrorDisplay
      error="An internal server error occurred. Our team has been notified."
      title="Server Error"
      onRetry={onRetry}
    />
  ),
};

export default ErrorDisplay;
