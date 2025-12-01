import FeatureLayout from '@/layout/FeatureLayout';
import { AdaptersPage as AdaptersComponent } from '@/components/AdaptersPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { Code } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useRBAC } from '@/hooks/useRBAC';

export default function AdaptersPage() {
  const navigate = useNavigate();
  const { can } = useRBAC();

  return (
    <DensityProvider pageKey="adapters">
      <FeatureLayout
        title="Adapters"
        description="Manage and monitor adapters"
        maxWidth="xl"
        contentPadding="default"
        primaryAction={{
          label: 'Train New Adapter',
          icon: Code,
          onClick: () => navigate('/training'),
          disabled: !can('TrainingStart'),
          size: 'sm'
        }}
        brief="Train a new LoRA adapter from your documents"
      >
        <AdaptersComponent />
      </FeatureLayout>
    </DensityProvider>
  );
}
