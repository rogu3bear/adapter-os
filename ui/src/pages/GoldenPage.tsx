import FeatureLayout from '@/layout/FeatureLayout';
import { GoldenRuns } from '@/components/GoldenRuns';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';
import { buildReplayCompareLink } from '@/utils/navLinks';

export default function GoldenPage() {

  return (
    <DensityProvider pageKey="golden">
      <FeatureLayout title="Golden" description="Baselines and summaries">
        <div className="flex justify-end mb-4">
          <Button asChild variant="outline" size="sm">
            <Link to={buildReplayCompareLink()}>Compare to live run</Link>
          </Button>
        </div>
        <SectionErrorBoundary sectionName="Golden">
          <GoldenRuns />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

