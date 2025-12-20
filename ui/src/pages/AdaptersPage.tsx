import PageWrapper from '@/layout/PageWrapper';
import { AdaptersPage as AdaptersComponent } from '@/components/AdaptersPage';
import { Code } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';
import { buildTrainingOverviewLink } from '@/utils/navLinks';

export default function AdaptersPage() {
  const navigate = useNavigate();
  const { can } = useRBAC();

  return (
    <PageWrapper
      pageKey="adapters"
      title="Adapters"
      description="Manage and monitor adapters"
      maxWidth="xl"
      contentPadding="default"
      customHeader={
        <IaPageHeader
          cluster="Build"
          title="Adapters"
          description="Manage and monitor adapters"
          brief="Train an adapter to learn patterns from your documents for consistent responses"
          primaryAction={{
            label: 'Train New Adapter',
            icon: Code,
            onClick: () => navigate(buildTrainingOverviewLink()),
            disabled: !can('TrainingStart'),
            size: 'sm',
          }}
        />
      }
    >
      <AdaptersComponent />
    </PageWrapper>
  );
}
