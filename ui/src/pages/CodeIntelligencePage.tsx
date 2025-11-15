import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { CodeIntelligence } from '@/components/CodeIntelligence';
import { DensityProvider } from '@/contexts/DensityContext';

export default function CodeIntelligencePage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="code-intelligence">
      <FeatureLayout title="Code Intelligence" description="Repository scanning and code analysis">
        <CodeIntelligence selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
