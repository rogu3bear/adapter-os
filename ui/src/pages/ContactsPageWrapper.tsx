import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ContactsPage } from '@/components/ContactsPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function ContactsPageWrapper() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="contacts">
      <FeatureLayout
        title="Contacts"
        description="Discovered contacts during inference with real-time updates"
      >
        <ContactsPage selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
