import { useEffect } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

type LegacyRedirectNoticeProps = {
  to: string;
  label?: string;
  /** Explains why this redirect exists - shown to user for context */
  reason?: string;
  autoRedirect?: boolean;
  delayMs?: number;
};

export default function LegacyRedirectNotice({
  to,
  label,
  reason,
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
            This page has moved to <strong>{label ?? to}</strong>. You&apos;ll be redirected
            automatically.
          </p>
          {reason && <p className="text-sm text-muted-foreground">{reason}</p>}
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
