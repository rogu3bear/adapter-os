import React from 'react';
import { useNavigate } from 'react-router-dom';
import { useTenant } from '@/providers/FeatureProviders';
import { useAuth } from '@/providers/CoreProviders';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { PageSkeleton } from '@/components/ui/page-skeleton';

interface TenantRequiredGateProps {
  children: React.ReactNode;
}

export function TenantRequiredGate({ children }: TenantRequiredGateProps) {
  const { user } = useAuth();
  const { selectedTenant, isLoading, loadError, loadTimedOut, refreshTenants } = useTenant();
  const navigate = useNavigate();

  if (!user) {
    return <>{children}</>;
  }

  if (isLoading) {
    return <PageSkeleton variant="table" />;
  }

  // Show error state if tenant loading failed or timed out
  if (loadError || loadTimedOut) {
    return (
      <div className="p-4 space-y-4">
        <Alert variant="destructive">
          <AlertTitle>
            {loadTimedOut ? 'Tenant loading timed out' : 'Failed to load tenants'}
          </AlertTitle>
          <AlertDescription>
            {loadTimedOut
              ? 'The server took too long to respond. Please check your connection and try again.'
              : `Unable to fetch tenant information: ${loadError?.message || 'Unknown error'}`}
          </AlertDescription>
        </Alert>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void refreshTenants()}>
            Retry
          </Button>
          <Button size="sm" variant="outline" onClick={() => navigate('/login')}>
            Back to login
          </Button>
        </div>
      </div>
    );
  }

  if (!selectedTenant) {
    return (
      <div className="p-4 space-y-4">
        <Alert variant="warning">
          <AlertTitle>Tenant required</AlertTitle>
          <AlertDescription>
            Select a tenant to continue. Use the header tenant switcher or reload tenants.
          </AlertDescription>
        </Alert>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void refreshTenants()}>
            Reload tenants
          </Button>
          <Button size="sm" variant="outline" onClick={() => navigate('/login')}>
            Back to login
          </Button>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}
