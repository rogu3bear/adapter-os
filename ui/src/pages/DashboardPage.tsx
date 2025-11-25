import { useAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import RoleBasedDashboard from '@/components/dashboard/index';
import { DashboardProvider } from '@/components/dashboard/DashboardProvider';
import { ModelSelector } from '@/components/ModelSelector';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';

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
          <RoleBasedDashboard />
        </DashboardProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}
