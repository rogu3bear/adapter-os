import { RequireAuth, useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tenants } from '@/components/Tenants';

export default function TenantsPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  return (
    <RequireAuth>
      <FeatureLayout title="Tenants" description="Manage tenant configurations and settings">
        <Tenants user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    </RequireAuth>
  );
}
