import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

export const ErrorRecoveryTemplates = {
  default: 'An error occurred. Please try again.',
  network: 'Network error. Check your connection.',
  auth: 'Authentication failed. Please log in again.',
  // Add more as needed based on common errors

  genericError: (error: Error, onRetry: () => void, options: Partial<ErrorRecoveryProps> = {}) => (
    <ErrorRecovery 
      title={options.title || "Error"}
      message={error.message || ErrorRecoveryTemplates.default} 
      variant={options.variant || "destructive"}
      recoveryActions={[{ label: "Retry", action: onRetry, primary: true }]}
      showHelp={options.showHelp || false}
    />
  ),

  networkError: (onRetry: () => void, options: Partial<ErrorRecoveryProps> = {}) => (
    <ErrorRecovery 
      title={options.title || "Network Error"}
      message={ErrorRecoveryTemplates.network} 
      variant={options.variant || "destructive"}
      recoveryActions={[{ label: "Retry", action: onRetry, primary: true }]}
      showHelp={options.showHelp || false}
    />
  ),

  authError: (onRetry: () => void, options: Partial<ErrorRecoveryProps> = {}) => (
    <ErrorRecovery 
      title={options.title || "Authentication Error"}
      message={ErrorRecoveryTemplates.auth} 
      variant={options.variant || "destructive"}
      recoveryActions={[{ label: "Retry", action: onRetry, primary: true }]}
      showHelp={options.showHelp || false}
    />
  ),
};

interface RecoveryAction {
  label: string;
  action: () => void;
  primary?: boolean;
  variant?: string;
}

interface ErrorRecoveryProps {
  title?: string;
  message?: string;
  error?: Error;
  variant?: "default" | "destructive" | "warning";
  recoveryActions?: RecoveryAction[];
  showHelp?: boolean;
}

export const ErrorRecovery = ({ 
  title = "Error", 
  message, 
  error,
  variant = "destructive", 
  recoveryActions = [], 
  showHelp = false 
}: ErrorRecoveryProps) => {
  const displayMessage = message || error?.message || ErrorRecoveryTemplates.default;

  return (
    <Alert variant={variant} className="mb-4">
      {title && <AlertTitle>{title}</AlertTitle>}
      <AlertDescription>{displayMessage}</AlertDescription>
      <div className="flex flex-col sm:flex-row gap-2 mt-2">
        {recoveryActions.map((action, index) => (
          <Button 
            key={index} 
            onClick={action.action} 
            variant={action.primary ? "default" : action.variant || "outline"} 
            size="sm"
          >
            {action.label}
          </Button>
        ))}
      </div>
      {showHelp && (
        <div className="mt-2 text-sm text-muted-foreground">
          If the problem persists, check the console or contact support.
        </div>
      )}
    </Alert>
  );
};
