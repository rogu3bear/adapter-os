import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Dashboard } from '@/components/Dashboard';
import { ModelSelector } from '@/components/ModelSelector';
import { DensityProvider } from '@/contexts/DensityContext';

export default function DashboardPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
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
        <Dashboard user={user} selectedTenant={selectedTenant} onNavigate={() => {}} />
      </FeatureLayout>
    </DensityProvider>
  );
}
