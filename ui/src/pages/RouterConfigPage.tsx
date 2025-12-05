import { useLocation, useNavigate } from 'react-router-dom';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { RouterConfigPage as RouterConfig } from '@/components/RouterConfigPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { parsePreselectParams, removeParams } from '@/utils/urlParams';

export default function RouterConfigPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const location = useLocation();
  const navigate = useNavigate();
  const { adapterId } = parsePreselectParams(location.search, location.hash);

  const handleClearFocus = () => {
    const nextSearch = removeParams(location.search, ['adapterId']);
    navigate(`${location.pathname}${nextSearch}${location.hash}`, { replace: true });
  };

  return (
    <DensityProvider pageKey="router-config">
      <FeatureLayout title="Router Configuration" description="Configure K-sparse LoRA routing parameters">
        <RouterConfig selectedTenant={selectedTenant} focusAdapterId={adapterId} onClearFocus={handleClearFocus} />
      </FeatureLayout>
    </DensityProvider>
  );
}
