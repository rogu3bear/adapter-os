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
import { useTenantUsage } from '@/hooks/useAdmin';
import type { Tenant } from '@/api/types';
import {
  Activity,
  Database,
  Cpu,
  HardDrive,
  Users,
  Layers,
  Shield,
} from 'lucide-react';

interface TenantDetailPageProps {
  tenant: Tenant;
  open: boolean;
  onClose: () => void;
}

export function TenantDetailPage({ tenant, open, onClose }: TenantDetailPageProps) {
  const { data: usage, isLoading, error, refetch } = useTenantUsage(tenant.id);

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Tenant Details: {tenant.name}</DialogTitle>
          <DialogDescription>
            Tenant ID: <span className="font-mono">{tenant.id}</span>
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="overview" className="w-full">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="usage">Usage Stats</TabsTrigger>
            <TabsTrigger value="permissions">Permissions</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="space-y-4">
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
                    <p className="text-sm font-medium text-muted-foreground">UID</p>
                    <p className="text-sm mt-1">{tenant.uid || 'N/A'}</p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">GID</p>
                    <p className="text-sm mt-1">{tenant.gid || 'N/A'}</p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Created</p>
                    <p className="text-sm mt-1">
                      {tenant.created_at
                        ? new Date(tenant.created_at).toLocaleString()
                        : 'N/A'}
                    </p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Data Classification</p>
                    <p className="text-sm mt-1">{tenant.data_classification || 'N/A'}</p>
                  </div>
                </div>
              </CardContent>
            </Card>

            {tenant.description && (
              <Card>
                <CardHeader>
                  <CardTitle>Description</CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-sm text-muted-foreground">{tenant.description}</p>
                </CardContent>
              </Card>
            )}
          </TabsContent>

          <TabsContent value="usage" className="space-y-4">
            {isLoading && <LoadingState message="Loading usage stats..." />}
            {error && (
              <ErrorRecovery
                error={error instanceof Error ? error.message : String(error)}
                onRetry={refetch}
              />
            )}
            {usage && (
              <div className="grid gap-4 md:grid-cols-2">
                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Inference Count (24h)</CardTitle>
                    <Activity className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">
                      {usage.inference_count_24h?.toLocaleString() || '0'}
                    </div>
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Tokens Processed</CardTitle>
                    <Database className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">
                      {usage.tokens_processed.toLocaleString()}
                    </div>
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Memory Usage</CardTitle>
                    <Cpu className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">
                      {usage.memory_used_gb?.toFixed(2) || '0'} GB
                    </div>
                    {usage.memory_total_gb && (
                      <p className="text-xs text-muted-foreground">
                        of {usage.memory_total_gb.toFixed(2)} GB total
                      </p>
                    )}
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Storage</CardTitle>
                    <HardDrive className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">
                      {(usage.storage_mb / 1024).toFixed(2)} GB
                    </div>
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Training Jobs</CardTitle>
                    <Layers className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">{usage.training_jobs}</div>
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                    <CardTitle className="text-sm font-medium">Active Adapters</CardTitle>
                    <Layers className="h-4 w-4 text-muted-foreground" />
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold">
                      {usage.active_adapters_count || '0'}
                    </div>
                  </CardContent>
                </Card>
              </div>
            )}
          </TabsContent>

          <TabsContent value="permissions" className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Shield className="h-5 w-5" />
                  Assigned Policies
                </CardTitle>
                <CardDescription>Policies applied to this tenant</CardDescription>
              </CardHeader>
              <CardContent>
                {tenant.policies && tenant.policies.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {tenant.policies.map((policy) => (
                      <Badge key={policy} variant="secondary">
                        {policy}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">No policies assigned</p>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Layers className="h-5 w-5" />
                  Assigned Adapters
                </CardTitle>
                <CardDescription>Adapters accessible to this tenant</CardDescription>
              </CardHeader>
              <CardContent>
                {tenant.adapters && tenant.adapters.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {tenant.adapters.map((adapter) => (
                      <Badge key={adapter} variant="outline">
                        {adapter}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">No adapters assigned</p>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Users className="h-5 w-5" />
                  Users
                </CardTitle>
                <CardDescription>Users with access to this tenant</CardDescription>
              </CardHeader>
              <CardContent>
                {tenant.users && tenant.users.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {tenant.users.map((user) => (
                      <Badge key={user} variant="outline">
                        {user}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">No users assigned</p>
                )}
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
