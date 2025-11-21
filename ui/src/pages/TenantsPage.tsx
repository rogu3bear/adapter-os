import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tenants } from '@/components/Tenants';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageHeader } from '@/components/ui/page-header';

export default function TenantsPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="tenants">
      <FeatureLayout title="Tenants" description="Manage tenant configurations and settings">
        <div className="space-y-6">
          <PageHeader
            title="Tenants"
            description="Manage tenant configurations and settings"
            helpContent="Configure and manage tenant settings and isolation"
          />
          <Tenants user={user} selectedTenant={selectedTenant} />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
