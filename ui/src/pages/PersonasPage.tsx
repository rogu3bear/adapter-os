import FeatureLayout from '@/layout/FeatureLayout';
import { PersonaJourneyDemo } from '../components/PersonaJourneyDemo';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';

export default function PersonasPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="personas">
      <FeatureLayout
        title="Personas"
        description="Persona journey demonstrations"
      >
        <div className="space-y-6">
          <PageHeader
            title="Personas"
            description="Persona journey demonstrations"
            helpContent="Explore different persona workflows and journeys"
          />
          <PersonaJourneyDemo />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
