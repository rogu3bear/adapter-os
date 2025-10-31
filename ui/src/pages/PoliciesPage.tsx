// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Policies } from '@/components/Policies';
import { DensityProvider } from '@/contexts/DensityContext';

export default function PoliciesPage() {
  return (
    <RequireAuth>
      <DensityProvider pageKey="policies">
        <FeatureLayout title="Policies" description="Security policies and compliance rules">
          <Policies />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
