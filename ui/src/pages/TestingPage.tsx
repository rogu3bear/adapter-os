import FeatureLayout from '@/layout/FeatureLayout';
import { TestingPage as TestingPageComponent } from '@/components/TestingPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TestingPage() {
  return (
    <DensityProvider pageKey="testing">
      <FeatureLayout title="Testing" description="Compare against golden baselines">
        <TestingPageComponent />
      </FeatureLayout>
    </DensityProvider>
  );
}

