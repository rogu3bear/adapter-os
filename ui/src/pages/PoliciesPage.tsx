import FeatureLayout from '@/layout/FeatureLayout';
import { Policies } from '@/components/Policies';
import { DensityProvider } from '@/contexts/DensityContext';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';

export default function PoliciesPage() {
  return (
    <DensityProvider pageKey="policies">
      <FeatureLayout title="Policies" description="Security policies and compliance rules">
        <div className="flex justify-end mb-4">
          <Button asChild variant="outline" size="sm">
            <Link to="/replay#runs">Open related replay</Link>
          </Button>
        </div>
        <Policies />
      </FeatureLayout>
    </DensityProvider>
  );
}
