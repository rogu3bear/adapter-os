// 【ui/src/pages/InferencePage.tsx§1-15】 - Page structure pattern
// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth, useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tenants } from '@/components/Tenants';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TenantsPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  return (
    <RequireAuth>
      <DensityProvider pageKey="tenants">
        <FeatureLayout title="Tenants" description="Manage tenant configurations and settings">
          <Tenants user={user} selectedTenant={selectedTenant} />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
