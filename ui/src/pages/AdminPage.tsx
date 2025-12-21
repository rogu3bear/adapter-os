import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/security/useRBAC';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import AdminPolicyConsole from '@/pages/Admin/AdminPolicyConsole';

export default function AdminPage() {
  const { can, userRole } = useRBAC();

  // Check if user has admin permissions
  if (!can('TenantManage') && userRole !== 'admin') {
    return (
      <DensityProvider pageKey="admin">
        <FeatureLayout
          title="IT Admin"
          description="System administration and management"
          maxWidth="xl"
          contentPadding="default"
        >
          {errorRecoveryTemplates.permissionError(() => window.location.reload())}
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="admin">
      <FeatureLayout
        title="IT Admin"
        description="Admin policies, overrides, and quotas"
        maxWidth="xl"
        contentPadding="default"
      >
        <AdminPolicyConsole />
      </FeatureLayout>
    </DensityProvider>
  );
}
