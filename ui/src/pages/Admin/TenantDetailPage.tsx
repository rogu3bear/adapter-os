import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { useInstallPackage, usePackages, useTenantUsage, useUninstallPackage } from '@/hooks/useAdmin';
import type { AdapterPackage, Tenant } from '@/api/types';
import { PackageTable } from './PackageTable';
import {
  Activity,
  Database,
  Cpu,
  HardDrive,
  Users,
  Layers,
  Shield,
  DollarSign,
  Clock,
} from 'lucide-react';

interface TenantDetailPageProps {
  tenant: Tenant;
  open: boolean;
  onClose: () => void;
}

type TenantUsage = ReturnType<typeof useTenantUsage>['data'];

const OverviewCard = ({ tenant }: { tenant: Tenant }) => (
  <Card>
    <CardHeader>
      <CardTitle>General Information</CardTitle>
    </CardHeader>
    <CardContent className="grid gap-4">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <p className="text-sm font-medium text-muted-foreground">Status</p>
          <Badge variant="default" className="mt-1">
            {tenant.status || 'active'}
          </Badge>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Isolation Level</p>
          <Badge variant="outline" className="mt-1">
            {tenant.isolation_level || 'standard'}
          </Badge>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">User ID</p>
          <p className="mt-1 text-sm">{tenant.uid || 'N/A'}</p>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Group ID</p>
          <p className="mt-1 text-sm">{tenant.gid || 'N/A'}</p>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Created</p>
          <p className="mt-1 text-sm">
            {tenant.created_at ? new Date(tenant.created_at).toLocaleString() : 'N/A'}
          </p>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Data Classification</p>
          <p className="mt-1 text-sm">{tenant.data_classification || 'N/A'}</p>
        </div>
      </div>
    </CardContent>
  </Card>
);

const DescriptionCard = ({ description }: { description?: string | null }) =>
  description ? (
    <Card>
      <CardHeader>
        <CardTitle>Description</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">{description}</p>
      </CardContent>
    </Card>
  ) : null;

const StatCard = ({
  title,
  value,
  icon,
  subtitle,
}: {
  title: string;
  value: string | number;
  icon: React.ReactNode;
  subtitle?: string;
}) => (
  <Card>
    <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardTitle className="text-sm font-medium">{title}</CardTitle>
      {icon}
    </CardHeader>
    <CardContent>
      <div className="text-2xl font-bold">{value}</div>
      {subtitle && <p className="text-xs text-muted-foreground">{subtitle}</p>}
    </CardContent>
  </Card>
);

const UsageTab = ({
  usage,
  isLoading,
  error,
  onRetry,
}: {
  usage?: TenantUsage;
  isLoading: boolean;
  error: unknown;
  onRetry: () => void;
}) => (
  <TabsContent value="usage" className="space-y-4">
    {isLoading && <LoadingState message="Loading usage stats..." />}
    {error && (
      <ErrorRecovery
        error={error instanceof Error ? error.message : String(error)}
        onRetry={onRetry}
      />
    )}
    {usage && (
      <div className="grid gap-4 md:grid-cols-2">
        <StatCard
          title="Inference Count (24h)"
          value={usage.inference_count_24h?.toLocaleString() || '0'}
          icon={<Activity className="h-4 w-4 text-muted-foreground" />}
        />
        <StatCard
          title="Tokens Processed"
          value={usage.tokens_processed.toLocaleString()}
          icon={<Database className="h-4 w-4 text-muted-foreground" />}
        />
        <StatCard
          title="Memory Usage"
          value={`${usage.memory_used_gb?.toFixed(2) || '0'} GB`}
          subtitle={
            usage.memory_total_gb ? `of ${usage.memory_total_gb.toFixed(2)} GB total` : undefined
          }
          icon={<Cpu className="h-4 w-4 text-muted-foreground" />}
        />
        <StatCard
          title="Storage"
          value={`${(usage.storage_mb / 1024).toFixed(2)} GB`}
          icon={<HardDrive className="h-4 w-4 text-muted-foreground" />}
        />
        <StatCard
          title="Training Jobs"
          value={usage.training_jobs}
          icon={<Layers className="h-4 w-4 text-muted-foreground" />}
        />
        <StatCard
          title="Active Adapters"
          value={usage.active_adapters_count || '0'}
          icon={<Layers className="h-4 w-4 text-muted-foreground" />}
        />
      </div>
    )}
  </TabsContent>
);

const CostSummaryCard = ({ usage }: { usage: TenantUsage }) => (
  <Card>
    <CardHeader>
      <CardTitle className="flex items-center gap-2">
        <DollarSign className="h-5 w-5" />
        Cost Summary
      </CardTitle>
      <CardDescription>
        Estimated usage costs for current period (placeholder for future billing integration)
      </CardDescription>
    </CardHeader>
    <CardContent className="space-y-4">
      <div className="grid gap-4 md:grid-cols-2">
        <div className="rounded-lg border p-4">
          <p className="text-sm font-medium text-muted-foreground">Inference Requests</p>
          <p className="mt-2 text-2xl font-bold">{usage.inference_count?.toLocaleString() || '0'}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {usage.inference_count_24h?.toLocaleString() || '0'} in last 24h
          </p>
        </div>
        <div className="rounded-lg border p-4">
          <p className="text-sm font-medium text-muted-foreground">Tokens Processed</p>
          <p className="mt-2 text-2xl font-bold">{usage.tokens_processed?.toLocaleString() || '0'}</p>
          <p className="mt-1 text-xs text-muted-foreground">Total across all requests</p>
        </div>
      </div>
      <div className="rounded-lg border bg-muted/50 p-4">
        <div className="flex items-start gap-3">
          <Clock className="mt-0.5 h-5 w-5 text-muted-foreground" />
          <div className="flex-1">
            <p className="text-sm font-medium">Training Time</p>
            <p className="mt-1 text-2xl font-bold">{usage.training_jobs || 0} jobs</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Training hours calculation available in future release
            </p>
          </div>
        </div>
      </div>
      <div className="rounded-lg border bg-muted/50 p-4">
        <div className="flex items-start gap-3">
          <HardDrive className="mt-0.5 h-5 w-5 text-muted-foreground" />
          <div className="flex-1">
            <p className="text-sm font-medium">Storage Usage</p>
            <p className="mt-1 text-2xl font-bold">{(usage.storage_mb / 1024).toFixed(2)} GB</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Includes adapters, datasets, and artifacts
            </p>
          </div>
        </div>
      </div>
    </CardContent>
  </Card>
);

const BillingNotesCard = () => (
  <Card>
    <CardHeader>
      <CardTitle>Billing Notes</CardTitle>
      <CardDescription>Future cost calculation integration</CardDescription>
    </CardHeader>
    <CardContent>
      <div className="space-y-2 text-sm text-muted-foreground">
        <p>Cost calculation and billing features will be added in a future release.</p>
        <p>Current usage metrics are tracked and available for review.</p>
        <ul className="mt-4 list-inside list-disc space-y-1">
          <li>Inference request counting</li>
          <li>Token usage tracking</li>
          <li>Training job monitoring</li>
          <li>Storage allocation tracking</li>
          <li>Resource utilization metrics</li>
        </ul>
      </div>
    </CardContent>
  </Card>
);

const BillingTab = ({ usage, isLoading, error, onRetry }: { usage?: TenantUsage; isLoading: boolean; error: unknown; onRetry: () => void }) => (
  <TabsContent value="billing" className="space-y-4">
    {isLoading && <LoadingState message="Loading billing data..." />}
    {error && (
      <ErrorRecovery
        error={error instanceof Error ? error.message : String(error)}
        onRetry={onRetry}
      />
    )}
    {usage && (
      <>
        <CostSummaryCard usage={usage} />
        <BillingNotesCard />
      </>
    )}
  </TabsContent>
);

const PillsCard = ({
  title,
  description,
  values,
  icon,
  variant = 'outline',
}: {
  title: string;
  description: string;
  values?: string[];
  icon: React.ReactNode;
  variant?: 'outline' | 'secondary';
}) => (
  <Card>
    <CardHeader>
      <CardTitle className="flex items-center gap-2">
        {icon}
        {title}
      </CardTitle>
      <CardDescription>{description}</CardDescription>
    </CardHeader>
    <CardContent>
      {values && values.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {values.map((value) => (
            <Badge key={value} variant={variant}>
              {value}
            </Badge>
          ))}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">No items assigned</p>
      )}
    </CardContent>
  </Card>
);

const PermissionsTab = ({ tenant }: { tenant: Tenant }) => (
  <TabsContent value="permissions" className="space-y-4">
    <PillsCard
      title="Assigned Policies"
      description="Policies applied to this organization"
      values={tenant.policies}
      icon={<Shield className="h-5 w-5" />}
      variant="secondary"
    />
    <PillsCard
      title="Assigned Adapters"
      description="Adapters accessible to this organization"
      values={tenant.adapters}
      icon={<Layers className="h-5 w-5" />}
    />
    <PillsCard
      title="Users"
      description="Users with access to this organization"
      values={tenant.users}
      icon={<Users className="h-5 w-5" />}
    />
  </TabsContent>
);

export function TenantDetailPage({ tenant, open, onClose }: TenantDetailPageProps) {
  const { data: usage, isLoading, error, refetch } = useTenantUsage(tenant.id);
  const {
    data: packages,
    isLoading: packagesLoading,
    error: packagesError,
    refetch: refetchPackages,
  } = usePackages({ tenantId: tenant.id });
  const installPackage = useInstallPackage();
  const uninstallPackage = useUninstallPackage();

  const handleInstall = async (pkg: AdapterPackage) => {
    await installPackage.mutateAsync({ tenantId: tenant.id, packageId: pkg.id });
  };

  const handleUninstall = async (pkg: AdapterPackage) => {
    await uninstallPackage.mutateAsync({ tenantId: tenant.id, packageId: pkg.id });
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Organization Details: {tenant.name}</DialogTitle>
          <DialogDescription>
            Organization ID: <span className="font-mono">{tenant.id}</span>
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="overview" className="w-full">
          <TabsList className="grid w-full grid-cols-5">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="usage">Usage Stats</TabsTrigger>
            <TabsTrigger value="packages">Packages</TabsTrigger>
            <TabsTrigger value="billing">Cost & Billing</TabsTrigger>
            <TabsTrigger value="permissions">Permissions</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="space-y-4">
            <OverviewCard tenant={tenant} />
            <DescriptionCard description={tenant.description} />
          </TabsContent>

          <UsageTab usage={usage} isLoading={isLoading} error={error} onRetry={refetch} />

          <TabsContent value="packages" className="space-y-4">
            {packagesLoading && <LoadingState message="Loading packages..." />}
            {packagesError && (
              <ErrorRecovery
                error={packagesError instanceof Error ? packagesError.message : String(packagesError)}
                onRetry={refetchPackages}
              />
            )}
            {!packagesLoading && !packagesError && (
              <Card>
                <CardHeader>
                  <CardTitle>Domain Packages</CardTitle>
                  <CardDescription>Install or remove domain-tagged packages for this tenant</CardDescription>
                </CardHeader>
                <CardContent>
                  <PackageTable
                    packages={packages || []}
                    onInstall={handleInstall}
                    onUninstall={handleUninstall}
                  />
                </CardContent>
              </Card>
            )}
          </TabsContent>

          <BillingTab usage={usage} isLoading={isLoading} error={error} onRetry={refetch} />

          <PermissionsTab tenant={tenant} />
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
