/**
 * Canonical admin shell with tabbed experience for organizations, users, capacity, stacks, and policies.
 */
import { useState } from 'react';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/security/useRBAC';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { TenantsTab } from './TenantsTab';
import { AdapterStacksTab } from './AdapterStacksTab';
import { UsersTab } from './UsersTab';
import { CapacityTab } from './CapacityTab';
import { AdminBanner } from '@/components/AdminBanner';
import { Users, Layers, UserCog, HardDrive, Shield } from 'lucide-react';
import AdminPolicyConsole from './AdminPolicyConsole';

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
        title="Administration"
        description="System administration and management"
        maxWidth="xl"
        contentPadding="default"
      >
        <AdminBanner />

        <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
          <TabsList>
            <TabsTrigger value="tenants" className="flex items-center gap-2">
              <Users className="h-4 w-4" />
              Workspaces
            </TabsTrigger>
            <TabsTrigger value="users" className="flex items-center gap-2">
              <UserCog className="h-4 w-4" />
              Users
            </TabsTrigger>
            <TabsTrigger value="adapter-stacks" className="flex items-center gap-2">
              <Layers className="h-4 w-4" />
              Adapter Stacks
            </TabsTrigger>
            <TabsTrigger value="capacity" className="flex items-center gap-2">
              <HardDrive className="h-4 w-4" />
              Capacity
            </TabsTrigger>
            <TabsTrigger value="policies" className="flex items-center gap-2">
              <Shield className="h-4 w-4" />
              Policies
            </TabsTrigger>
          </TabsList>

          <TabsContent value="tenants" className="space-y-4">
            <SectionErrorBoundary sectionName="Workspaces">
              <TenantsTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="users" className="space-y-4">
            <SectionErrorBoundary sectionName="Users">
              <UsersTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="adapter-stacks" className="space-y-4">
            <SectionErrorBoundary sectionName="Adapter Stacks">
              <AdapterStacksTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="capacity" className="space-y-4">
            <SectionErrorBoundary sectionName="Capacity">
              <CapacityTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="policies" className="space-y-4">
            <SectionErrorBoundary sectionName="Policies">
              <AdminPolicyConsole />
            </SectionErrorBoundary>
          </TabsContent>
        </Tabs>
      </FeatureLayout>
    </DensityProvider>
  );
}
