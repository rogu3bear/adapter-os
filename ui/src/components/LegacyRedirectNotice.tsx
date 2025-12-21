import { useEffect } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

type LegacyRedirectNoticeProps = {
  to: string;
  label?: string;
  autoRedirect?: boolean;
  delayMs?: number;
};

export default function LegacyRedirectNotice({
  to,
  label,
  autoRedirect = true,
  delayMs = 150,
}: LegacyRedirectNoticeProps) {
  const navigate = useNavigate();

  useEffect(() => {
    if (!autoRedirect) return;
    const timer = setTimeout(() => navigate(to, { replace: true }), delayMs);
    return () => clearTimeout(timer);
  }, [autoRedirect, delayMs, navigate, to]);

  return (
    <div className="max-w-2xl mx-auto py-10">
      <Alert>
        <AlertTitle>Redirecting to the updated flow</AlertTitle>
        <AlertDescription className="space-y-3">
          <p>
            This page is deprecated and now lives in {label ?? to}. You&apos;ll be redirected
            automatically.
          </p>
          <div className="flex gap-3">
            <Button asChild size="sm">
              <Link to={to} replace>
                Go now
              </Link>
            </Button>
            <Button size="sm" variant="secondary" onClick={() => navigate(-1)}>
              Go back
            </Button>
          </div>
        </AlertDescription>
      </Alert>
    </div>
  );
}

