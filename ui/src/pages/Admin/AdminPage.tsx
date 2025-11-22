import { useState } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { TenantsTab } from './TenantsTab';
import { AdapterStacksTab } from './AdapterStacksTab';
import { UsersTab } from './UsersTab';
import { Users, Layers, UserCog } from 'lucide-react';

export default function AdminPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [activeTab, setActiveTab] = useState('tenants');

  // Check if user has admin permissions
  if (!can('TenantManage') && userRole !== 'admin') {
    return (
      <DensityProvider pageKey="admin">
        <FeatureLayout
          title="Administration"
          maxWidth="xl"
          contentPadding="default"
        >
          <PageHeader
            title="Administration"
            description="System administration and management"
          />
          {errorRecoveryTemplates.permissionError(() => window.location.reload())}
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="admin">
      <FeatureLayout
        title="Administration"
        maxWidth="xl"
        contentPadding="default"
      >
        <PageHeader
          title="Administration"
          description="System administration and management"
        />

        <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
          <TabsList>
            <TabsTrigger value="tenants" className="flex items-center gap-2">
              <Users className="h-4 w-4" />
              Tenants
            </TabsTrigger>
            <TabsTrigger value="users" className="flex items-center gap-2">
              <UserCog className="h-4 w-4" />
              Users
            </TabsTrigger>
            <TabsTrigger value="adapter-stacks" className="flex items-center gap-2">
              <Layers className="h-4 w-4" />
              Adapter Stacks
            </TabsTrigger>
          </TabsList>

          <TabsContent value="tenants" className="space-y-4">
            <TenantsTab />
          </TabsContent>

          <TabsContent value="users" className="space-y-4">
            <UsersTab />
          </TabsContent>

          <TabsContent value="adapter-stacks" className="space-y-4">
            <AdapterStacksTab />
          </TabsContent>
        </Tabs>
      </FeatureLayout>
    </DensityProvider>
  );
}
