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
