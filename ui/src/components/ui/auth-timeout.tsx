import { AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

export function AuthTimeoutError() {
  const handleTryAgain = () => {
    window.location.reload();
  };

  const handleGoToLogin = () => {
    window.location.href = '/login';
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background px-6">
      <Card className="max-w-md w-full p-8 space-y-6">
        <div className="flex flex-col items-center text-center space-y-4">
          <div className="rounded-full bg-destructive/10 p-3">
            <AlertTriangle className="h-8 w-8 text-destructive" />
          </div>

          <div className="space-y-2">
            <h2 className="text-2xl font-semibold tracking-tight">
              Authentication Taking Too Long
            </h2>
            <p className="text-sm text-muted-foreground">
              The authentication check is taking longer than expected. This could be due to network issues or the server being slow to respond.
            </p>
          </div>
        </div>

        <div className="space-y-3">
          <Button
            onClick={handleTryAgain}
            className="w-full"
            size="lg"
          >
            Try Again
          </Button>

          <Button
            onClick={handleGoToLogin}
            variant="outline"
            className="w-full"
            size="lg"
          >
            Go to Login
          </Button>
        </div>

        <div className="text-center">
          <p className="text-xs text-muted-foreground">
            If this problem persists, please check your network connection or contact support.
          </p>
        </div>
      </Card>
    </div>
  );
}
