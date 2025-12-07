import { useLocation, useNavigate } from 'react-router-dom';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { RouterConfigPage as RouterConfig } from '@/components/RouterConfigPage';
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
    <PageWrapper
      pageKey="router-config"
      title="Router Configuration"
      description="Configure K-sparse LoRA routing parameters"
      maxWidth="xl"
      contentPadding="default"
    >
      <RouterConfig selectedTenant={selectedTenant} focusAdapterId={adapterId} onClearFocus={handleClearFocus} />
    </PageWrapper>
  );
}
