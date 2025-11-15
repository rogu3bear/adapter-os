import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ContactsPage } from '@/components/ContactsPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ContactsPageWrapper() {
  const { selectedTenant } = useTenant();

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
