import FeatureLayout from '@/layout/FeatureLayout';
import { AdaptersPage as AdaptersComponent } from '@/components/AdaptersPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { Button } from '@/components/ui/button';
import { Code } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

export default function AdaptersPage() {
  const navigate = useNavigate();

  return (
    <DensityProvider pageKey="adapters">
      <FeatureLayout
        title="Adapters"
        description="Manage and monitor adapters"
        maxWidth="xl"
        contentPadding="default"
        headerActions={
          <Button size="sm" onClick={() => navigate('/training')}>
            <Code className="mr-2 h-4 w-4" />
            Train New Adapter
          </Button>
        }
      >
        <AdaptersComponent />
      </FeatureLayout>
    </DensityProvider>
  );
}
