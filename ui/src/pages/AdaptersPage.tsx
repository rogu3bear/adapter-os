// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { AdaptersPage as AdaptersComponent } from '@/components/AdaptersPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function AdaptersPage() {
  return (
    <RequireAuth>
      <DensityProvider pageKey="adapters">
        <FeatureLayout title="Adapters" description="Manage and monitor LoRA adapters">
          <AdaptersComponent />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
