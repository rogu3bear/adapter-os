import FeatureLayout from '@/layout/FeatureLayout';
import { PersonaJourneyDemo } from '@/components/PersonaJourneyDemo';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';

export default function PersonasPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="personas">
      <FeatureLayout
        title="Personas"
        description="Persona journey demonstrations"
        brief="Explore different persona workflows and journeys"
      >
        <div className="space-y-6">
          <PersonaJourneyDemo />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
