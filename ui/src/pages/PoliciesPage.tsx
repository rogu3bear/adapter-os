import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Policies } from '@/components/Policies';

export default function PoliciesPage() {
  return (
    <RequireAuth>
      <FeatureLayout title="Policies" description="Security policies and compliance rules">
        <Policies />
      </FeatureLayout>
    </RequireAuth>
  );
}
