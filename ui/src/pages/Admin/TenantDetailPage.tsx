import { useNavigate, useParams } from 'react-router-dom';
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
import { Button } from '@/components/ui/button';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { useTenantUsage, useTenants } from '@/hooks/admin/useAdmin';
import type { Tenant } from '@/api/types';
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
import { buildAdminTenantsLink } from '@/utils/navLinks';

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
}) => {
  return (
    <TabsContent value="usage" className="space-y-4">
      {isLoading && <LoadingState message="Loading usage stats..." />}
      {!isLoading && !!error && (
        <ErrorRecovery
          error={error instanceof Error ? error.message : String(error)}
          onRetry={onRetry}
        />
      )}
      {usage && (
        <div className="grid gap-4 md:grid-cols-2">
          <StatCard
            title="Inference Count (24h)"
            value={usage.inference_count_24h?.toLocaleString() ?? '0'}
            icon={<Activity className="h-4 w-4 text-muted-foreground" />}
          />
          <StatCard
            title="Tokens Processed"
            value={usage.tokens_processed?.toLocaleString() ?? '0'}
            icon={<Database className="h-4 w-4 text-muted-foreground" />}
          />
          <StatCard
            title="Memory Usage"
            value={`${usage.memory_used_gb?.toFixed(2) ?? '0'} GB`}
            subtitle={
              usage.memory_total_gb ? `of ${usage.memory_total_gb.toFixed(2)} GB total` : undefined
            }
            icon={<Cpu className="h-4 w-4 text-muted-foreground" />}
          />
          <StatCard
            title="Storage"
            value={`${((usage.storage_mb ?? 0) / 1024).toFixed(2)} GB`}
            icon={<HardDrive className="h-4 w-4 text-muted-foreground" />}
          />
          <StatCard
            title="Training Jobs"
            value={usage.training_jobs ?? 0}
            icon={<Layers className="h-4 w-4 text-muted-foreground" />}
          />
          <StatCard
            title="Active Adapters"
            value={usage.active_adapters_count ?? '0'}
            icon={<Layers className="h-4 w-4 text-muted-foreground" />}
          />
        </div>
      )}
    </TabsContent>
  );
};

const CostSummaryCard = ({ usage }: { usage: NonNullable<TenantUsage> }) => (
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
          <p className="mt-2 text-2xl font-bold">{usage.inference_count?.toLocaleString() ?? '0'}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {usage.inference_count_24h?.toLocaleString() ?? '0'} in last 24h
          </p>
        </div>
        <div className="rounded-lg border p-4">
          <p className="text-sm font-medium text-muted-foreground">Tokens Processed</p>
          <p className="mt-2 text-2xl font-bold">{usage.tokens_processed?.toLocaleString() ?? '0'}</p>
          <p className="mt-1 text-xs text-muted-foreground">Total across all requests</p>
        </div>
      </div>
      <div className="rounded-lg border bg-muted/50 p-4">
        <div className="flex items-start gap-3">
          <Clock className="mt-0.5 h-5 w-5 text-muted-foreground" />
          <div className="flex-1">
            <p className="text-sm font-medium">Training Time</p>
            <p className="mt-1 text-2xl font-bold">{usage.training_jobs ?? 0} jobs</p>
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
            <p className="mt-1 text-2xl font-bold">{((usage.storage_mb ?? 0) / 1024).toFixed(2)} GB</p>
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

const BillingTab = ({ usage, isLoading, error, onRetry }: { usage?: TenantUsage; isLoading: boolean; error: unknown; onRetry: () => void }) => {
  return (
    <TabsContent value="billing" className="space-y-4">
      {isLoading && <LoadingState message="Loading billing data..." />}
      {!isLoading && !!error && (
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
};

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
      description="Policies applied to this workspace"
      values={tenant.policies}
      icon={<Shield className="h-5 w-5" />}
      variant="secondary"
    />
    <PillsCard
      title="Assigned Adapters"
      description="Adapters accessible to this workspace"
      values={tenant.adapters}
      icon={<Layers className="h-5 w-5" />}
    />
    <PillsCard
      title="Users"
      description="Users with access to this workspace"
      values={tenant.users}
      icon={<Users className="h-5 w-5" />}
    />
  </TabsContent>
);

export function TenantDetailPage({ tenant, open, onClose }: TenantDetailPageProps) {
  const { data: usage, isLoading, error, refetch } = useTenantUsage(tenant.id);

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Workspace Details: {tenant.name}</DialogTitle>
          <DialogDescription>
            Workspace ID: <span className="font-mono">{tenant.id}</span>
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="overview" className="w-full">
          <TabsList className="grid w-full grid-cols-4">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="usage">Usage Stats</TabsTrigger>
            <TabsTrigger value="billing">Cost & Billing</TabsTrigger>
            <TabsTrigger value="permissions">Permissions</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="space-y-4">
            <OverviewCard tenant={tenant} />
            <DescriptionCard description={tenant.description} />
          </TabsContent>

          <UsageTab usage={usage} isLoading={isLoading} error={error} onRetry={refetch} />

          <BillingTab usage={usage} isLoading={isLoading} error={error} onRetry={refetch} />

          <PermissionsTab tenant={tenant} />
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Route-safe wrapper for TenantDetailPage.
 * This component reads tenantId from URL params and fetches the tenant data,
 * rendering the modal TenantDetailPage with proper props.
 *
 * NOTE: Modal components with required props should NEVER be used directly as route components.
 * Always use a *RoutePage wrapper that reads params and fetches data.
 */
export default function TenantDetailRoutePage() {
  const navigate = useNavigate();
  const { tenantId } = useParams<{ tenantId: string }>();
  const { data: tenants, isLoading, error, refetch } = useTenants();

  const handleClose = () => {
    navigate(buildAdminTenantsLink());
  };

  // Loading state
  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <LoadingState message="Loading workspace details..." />
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <ErrorRecovery
          error={error instanceof Error ? error.message : String(error)}
          onRetry={refetch}
        />
      </div>
    );
  }

  // Find the tenant from the list
  const tenant = tenants?.find((t) => t.id === tenantId);

  // Not found state
  if (!tenant) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] gap-4">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>Workspace Not Found</CardTitle>
            <CardDescription>
              The workspace with ID <span className="font-mono">{tenantId}</span> could not be found.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={handleClose} variant="outline">
              Back to Workspaces
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Render the modal with tenant data
  return <TenantDetailPage tenant={tenant} open={true} onClose={handleClose} />;
}
