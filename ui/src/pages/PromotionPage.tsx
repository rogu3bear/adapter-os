import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Promotion } from '@/components/Promotion';
import { DensityProvider } from '@/contexts/DensityContext';

export default function PromotionPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="promotion">
      <FeatureLayout title="Promotion" description="Promotion gates and approvals">
        <Promotion user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}

