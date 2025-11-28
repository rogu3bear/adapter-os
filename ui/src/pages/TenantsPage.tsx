import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tenants } from '@/components/Tenants';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TenantsPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="tenants">
      <FeatureLayout
        title="Organizations"
        description="Manage organization configurations and settings"
        helpContent="Configure and manage organization settings and isolation"
      >
        <div className="space-y-6">
          <Tenants user={user} selectedTenant={selectedTenant} />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
