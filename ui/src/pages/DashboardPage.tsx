import { useAuth } from '@/layout/LayoutProvider';
import PageWrapper from '@/layout/PageWrapper';
import RoleBasedDashboard from '@/components/dashboard/index';
import { DashboardProvider } from '@/components/dashboard/DashboardProvider';
import { ModelSelector } from '@/components/ModelSelector';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { Button } from '@/components/ui/button';
import { useNavigate } from 'react-router-dom';

export default function DashboardPage() {
  const { user } = useAuth();
  const navigate = useNavigate();
  const greeting = user
    ? `Welcome back, ${user.display_name || user.email}`
    : 'System overview, health monitoring, and alerts';

  return (
    <PageWrapper
      pageKey="dashboard"
      title="Dashboard"
      description={greeting}
      maxWidth="xl"
      customHeader={
        <IaPageHeader
          cluster="Run"
          title="Dashboard"
          description={greeting}
          secondaryActions={[
            {
              label: 'Onboarding checklist',
              onClick: () => navigate('/workflow'),
            },
            {
              label: 'Run probe',
              onClick: () => navigate('/inference'),
            },
          ]}
        >
          <ModelSelector />
        </IaPageHeader>
      }
    >
      <DashboardProvider>
        <SectionErrorBoundary sectionName="Dashboard">
          <RoleBasedDashboard />
        </SectionErrorBoundary>
      </DashboardProvider>
    </PageWrapper>
  );
}
