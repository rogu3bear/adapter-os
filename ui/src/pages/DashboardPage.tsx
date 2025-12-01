import { useAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import RoleBasedDashboard from '@/components/dashboard/index';
import { DashboardProvider } from '@/components/dashboard/DashboardProvider';
import { ModelSelector } from '@/components/ModelSelector';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function DashboardPage() {
  const { user } = useAuth();
  const { userRole } = useRBAC();
  const greeting = user
    ? `Welcome back, ${user.display_name || user.email}`
    : 'System overview, health monitoring, and alerts';

  return (
    <DensityProvider pageKey="dashboard">
      <FeatureLayout
        title="Dashboard"
        description={greeting}
        maxWidth="xl"
        headerActions={<ModelSelector />}
      >
        <DashboardProvider>
          <SectionErrorBoundary sectionName="Dashboard">
            <RoleBasedDashboard />
          </SectionErrorBoundary>
        </DashboardProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}
