import FeatureLayout from '@/layout/FeatureLayout';
import { Policies } from '@/components/Policies';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageHeader } from '@/components/ui/page-header';

export default function PoliciesPage() {
  return (
    <DensityProvider pageKey="policies">
      <FeatureLayout title="Policies">
        <PageHeader
          title="Policies"
          description="Security policies and compliance rules"
        />
        <Policies />
      </FeatureLayout>
    </DensityProvider>
  );
}
