import { Link } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { Button } from '@/components/ui/button';

export default function NotFoundPage() {
  return (
    <PageWrapper
      pageKey="not-found"
      title="Page not found"
      description="This page does not exist or is no longer available."
      contentPadding="default"
      maxWidth="md"
    >
      <div className="space-y-4">
        <p className="text-muted-foreground">
          Check the URL or return to the dashboard.
        </p>
        <Button asChild>
          <Link to="/dashboard">Back to dashboard</Link>
        </Button>
      </div>
    </PageWrapper>
  );
}

