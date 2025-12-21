import { Link } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { Button } from '@/components/ui/button';
import { DemoRecoveryHints } from '@/components/ui/demo-recovery-hints';

interface ServerErrorPageProps {
  onRetry?: () => void;
  error?: Error;
}

export default function ServerErrorPage({ onRetry, error }: ServerErrorPageProps) {
  return (
    <PageWrapper
      pageKey="server-error"
      title="Something went wrong"
      description="We hit an unexpected error while loading this page."
      contentPadding="default"
      maxWidth="md"
    >
      <div className="space-y-4">
        {error?.message && (
          <p className="text-sm text-muted-foreground">
            <span className="font-medium text-foreground">Error:</span>{' '}
            <span className="font-mono break-all">{error.message}</span>
          </p>
        )}
        <p className="text-muted-foreground">
          Try again or return to the dashboard.
        </p>
        <div className="flex gap-2">
          <Button onClick={onRetry ?? (() => window.location.reload())}>Try again</Button>
          <Button variant="outline" asChild>
            <Link to="/dashboard">Back to dashboard</Link>
          </Button>
        </div>
        <DemoRecoveryHints />
      </div>
    </PageWrapper>
  );
}
