import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { AdaptersPage as AdaptersComponent } from '@/components/AdaptersPage';

export default function AdaptersPage() {
  return (
    <RequireAuth>
      <FeatureLayout title="Adapters" description="Manage and monitor LoRA adapters">
        <AdaptersComponent />
      </FeatureLayout>
    </RequireAuth>
  );
}
