import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tenants } from '@/components/Tenants';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function TenantsPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  if (!user) {
    return null;
  }

  return (
    <DensityProvider pageKey="tenants">
      <FeatureLayout
        title="Organizations"
        description="Manage organization configurations and settings"
        brief="Configure and manage organization settings and isolation"
      >
        <SectionErrorBoundary sectionName="Tenants">
          <Tenants user={user} selectedTenant={selectedTenant} />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}
