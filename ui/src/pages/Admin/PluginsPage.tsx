//! Plugin Management Page
//!
//! Admin interface for managing system plugins.
//! Allows enabling/disabling plugins and viewing plugin status.
//!
//! Citation: Table patterns from ui/src/pages/Admin/AdapterStacksTab.tsx
//! - Card layout with table
//! - Action buttons for enable/disable

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useRBAC } from '@/hooks/useRBAC';
import apiClient from '@/api/client';
import type { PluginInfo, EnablePluginRequest, DisablePluginRequest, PluginConfigRecord } from '@/api/plugin-types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import {
  Plug,
  RefreshCw,
  Search,
  AlertCircle,
  CheckCircle,
  XCircle,
  Info,
  Loader2,
  Settings2,
  ShieldCheck,
  Package,
  Save,
} from 'lucide-react';

const PermissionDeniedView = () => (
  <DensityProvider pageKey="plugins">
    <FeatureLayout title="Plugin Management" description="Manage system plugins">
      <Alert variant="destructive">
        <AlertCircle className="h-4 w-4" />
        <AlertTitle>Permission Denied</AlertTitle>
        <AlertDescription>
          You do not have permission to manage plugins. Admin role required.
        </AlertDescription>
      </Alert>
    </FeatureLayout>
  </DensityProvider>
);

const LoadingView = () => (
  <DensityProvider pageKey="plugins">
    <FeatureLayout title="Plugin Management" description="Enable, disable, and configure system plugins">
      <LoadingState message="Loading plugins..." />
    </FeatureLayout>
  </DensityProvider>
);

const ErrorView = ({ message, onRetry }: { message: string; onRetry: () => void }) => (
  <DensityProvider pageKey="plugins">
    <FeatureLayout title="Plugin Management" description="Enable, disable, and configure system plugins">
      <ErrorRecovery error={message} onRetry={onRetry} />
    </FeatureLayout>
  </DensityProvider>
);

const StatTile = ({
  title,
  value,
  icon,
  iconClassName,
}: {
  title: string;
  value: number;
  icon: React.ReactNode;
  iconClassName?: string;
}) => (
  <Card>
    <CardContent className="pt-6">
      <div className="flex items-center gap-4">
        <div className={`rounded-lg p-3 ${iconClassName || 'bg-muted'}`}>{icon}</div>
        <div>
          <p className="text-sm text-muted-foreground">{title}</p>
          <p className="text-2xl font-bold">{value}</p>
        </div>
      </div>
    </CardContent>
  </Card>
);

const StatsGrid = ({ total, enabled, disabled }: { total: number; enabled: number; disabled: number }) => (
  <div className="mb-6 grid grid-cols-3 gap-4">
    <StatTile title="Total Plugins" value={total} icon={<Package className="h-6 w-6 text-primary" />} iconClassName="bg-primary/10" />
    <StatTile title="Enabled" value={enabled} icon={<CheckCircle className="h-6 w-6 text-green-500" />} iconClassName="bg-green-500/10" />
    <StatTile title="Disabled" value={disabled} icon={<XCircle className="h-6 w-6 text-muted-foreground" />} />
  </div>
);

const StatusBadge = ({ status }: { status: PluginInfo['status'] }) => {
  switch (status) {
    case 'enabled':
      return (
        <Badge variant="default" className="gap-1">
          <CheckCircle className="h-3 w-3" />
          Enabled
        </Badge>
      );
    case 'disabled':
      return (
        <Badge variant="secondary" className="gap-1">
          <XCircle className="h-3 w-3" />
          Disabled
        </Badge>
      );
    default:
      return <Badge variant="outline">{status}</Badge>;
  }
};

const PluginRow = ({
  plugin,
  onConfigure,
  onToggle,
  isMutating,
}: {
  plugin: PluginInfo;
  onConfigure: () => void;
  onToggle: () => void;
  isMutating: boolean;
}) => (
  <TableRow>
    <TableCell>
      <div className="flex flex-col gap-1">
        <span className="font-medium">{plugin.display_name}</span>
        <span className="text-sm text-muted-foreground">{plugin.description}</span>
        <code className="text-xs text-muted-foreground">{plugin.name}</code>
      </div>
    </TableCell>
    <TableCell>
      <Badge variant="outline">{plugin.version}</Badge>
    </TableCell>
    <TableCell>
      <StatusBadge status={plugin.status} />
    </TableCell>
    <TableCell>{plugin.author || <span className="text-muted-foreground">-</span>}</TableCell>
    <TableCell>
      {plugin.enabled_tenants && plugin.enabled_tenants.length > 0 ? (
        <div className="flex flex-wrap gap-1">
          {plugin.enabled_tenants.slice(0, 3).map((tenant) => (
            <Badge key={tenant} variant="outline" className="text-xs">
              {tenant}
            </Badge>
          ))}
          {plugin.enabled_tenants.length > 3 && (
            <Badge variant="outline" className="text-xs">
              +{plugin.enabled_tenants.length - 3}
            </Badge>
          )}
        </div>
      ) : (
        <span className="text-sm text-muted-foreground">Global</span>
      )}
    </TableCell>
    <TableCell className="text-right">
      <div className="flex items-center justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onConfigure} title="Configure plugin">
          <Settings2 className="h-4 w-4" />
        </Button>
        <Switch
          checked={plugin.status === 'enabled'}
          onCheckedChange={onToggle}
          disabled={isMutating}
        />
      </div>
    </TableCell>
  </TableRow>
);

const PluginsTableCard = ({
  plugins,
  searchQuery,
  onSearchChange,
  onConfigure,
  onToggle,
  isMutating,
}: {
  plugins: PluginInfo[];
  searchQuery: string;
  onSearchChange: (value: string) => void;
  onConfigure: (plugin: PluginInfo) => void;
  onToggle: (plugin: PluginInfo) => void;
  isMutating: boolean;
}) => (
  <Card>
    <CardHeader>
      <div className="flex items-center justify-between">
        <div>
          <CardTitle className="flex items-center gap-2">
            <Plug className="h-5 w-5" />
            Installed Plugins
          </CardTitle>
          <CardDescription>Manage plugins installed on this system</CardDescription>
        </div>
        <div className="relative w-64">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Search plugins..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>
    </CardHeader>
    <CardContent>
      {plugins.length === 0 ? (
        <div className="py-12 text-center">
          <Plug className="mx-auto mb-4 h-12 w-12 text-muted-foreground" />
          <h3 className="mb-2 text-lg font-semibold">No Matching Plugins</h3>
          <p className="text-muted-foreground">Try adjusting your search criteria.</p>
        </div>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Plugin</TableHead>
              <TableHead>Version</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Author</TableHead>
              <TableHead>Organizations</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {plugins.map((plugin) => (
              <PluginRow
                key={plugin.name}
                plugin={plugin}
                onConfigure={() => onConfigure(plugin)}
                onToggle={() => onToggle(plugin)}
                isMutating={isMutating}
              />
            ))}
          </TableBody>
        </Table>
      )}
    </CardContent>
  </Card>
);

const ActionDialog = ({
  open,
  onOpenChange,
  actionType,
  plugin,
  onConfirm,
  onCancel,
  isPending,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  actionType: 'enable' | 'disable';
  plugin: PluginInfo | null;
  onConfirm: () => void;
  onCancel: () => void;
  isPending: boolean;
}) => (
  <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent>
      <DialogHeader>
        <DialogTitle>{actionType === 'enable' ? 'Enable Plugin' : 'Disable Plugin'}</DialogTitle>
        <DialogDescription>
          {actionType === 'enable'
            ? `Enable "${plugin?.display_name}" for all organizations.`
            : `Disable "${plugin?.display_name}". This may affect organizations using it.`}
        </DialogDescription>
      </DialogHeader>
      {plugin && (
        <div className="space-y-4 py-4">
          <div className="flex items-center gap-4 rounded-lg bg-muted p-4">
            <Plug className="h-8 w-8 text-muted-foreground" />
            <div>
              <p className="font-medium">{plugin.display_name}</p>
              <p className="text-sm text-muted-foreground">{plugin.description}</p>
              <p className="mt-1 text-xs text-muted-foreground">Version: {plugin.version}</p>
            </div>
          </div>
          {actionType === 'disable' &&
            plugin.enabled_tenants &&
            plugin.enabled_tenants.length > 0 && (
              <Alert>
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Warning</AlertTitle>
                <AlertDescription>
                  This plugin is currently enabled for {plugin.enabled_tenants.length} organization(s).
                  Disabling it may affect their workflows.
                </AlertDescription>
              </Alert>
            )}
        </div>
      )}
      <DialogFooter>
        <Button variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          variant={actionType === 'disable' ? 'destructive' : 'default'}
          onClick={onConfirm}
          disabled={isPending}
        >
          {isPending ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              {actionType === 'enable' ? 'Enabling...' : 'Disabling...'}
            </>
          ) : (
            <>
              {actionType === 'enable' ? (
                <CheckCircle className="mr-2 h-4 w-4" />
              ) : (
                <XCircle className="mr-2 h-4 w-4" />
              )}
              {actionType === 'enable' ? 'Enable' : 'Disable'}
            </>
          )}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
);

const ConfigDialog = ({
  open,
  onOpenChange,
  plugin,
  configJson,
  onConfigChange,
  error,
  isLoading,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  plugin: PluginInfo | null;
  configJson: string;
  onConfigChange: (value: string) => void;
  error: string | null;
  isLoading: boolean;
  onSave: () => void;
}) => (
  <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="max-w-2xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <Settings2 className="h-5 w-5" />
          Plugin Configuration
        </DialogTitle>
        <DialogDescription>Configure settings for {plugin?.display_name}</DialogDescription>
      </DialogHeader>
      {plugin && (
        <div className="space-y-4 py-4">
          <div className="flex items-center gap-4 rounded-lg bg-muted p-4">
            <Plug className="h-8 w-8 text-muted-foreground" />
            <div className="flex-1">
              <p className="font-medium">{plugin.display_name}</p>
              <p className="text-sm text-muted-foreground">{plugin.description}</p>
              <div className="mt-1 flex items-center gap-2">
                <Badge variant="outline" className="text-xs">
                  {plugin.version}
                </Badge>
                <StatusBadge status={plugin.status} />
              </div>
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="config-json">Configuration JSON</Label>
            <Textarea
              id="config-json"
              value={configJson}
              onChange={(e) => onConfigChange(e.target.value)}
              placeholder='{"key": "value"}'
              className="min-h-[300px] font-mono text-sm"
              disabled={isLoading}
            />
            {error && <p className="text-sm text-destructive">{error}</p>}
            <p className="text-xs text-muted-foreground">
              Enter plugin configuration as JSON. Leave empty or enter {} for default settings.
            </p>
          </div>
          {isLoading && (
            <div className="flex items-center justify-center py-4">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          )}
        </div>
      )}
      <DialogFooter>
        <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isLoading}>
          Cancel
        </Button>
        <Button onClick={onSave} disabled={isLoading || !!error}>
          {isLoading ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              Saving...
            </>
          ) : (
            <>
              <Save className="mr-2 h-4 w-4" />
              Save Configuration
            </>
          )}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
);

export function PluginsPage() {
  const queryClient = useQueryClient();
  const { can, userRole } = useRBAC();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedPlugin, setSelectedPlugin] = useState<PluginInfo | null>(null);
  const [actionDialogOpen, setActionDialogOpen] = useState(false);
  const [actionType, setActionType] = useState<'enable' | 'disable'>('enable');

  // Configuration dialog state
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [configPlugin, setConfigPlugin] = useState<PluginInfo | null>(null);
  const [configJson, setConfigJson] = useState<string>('');
  const [configJsonError, setConfigJsonError] = useState<string | null>(null);
  const [loadingConfig, setLoadingConfig] = useState(false);

  // Check admin permissions
  if (userRole !== 'admin' && !can('TenantManage')) {
    return (
      <DensityProvider pageKey="plugins">
        <FeatureLayout
          title="Plugin Management"
          description="Manage system plugins"
        >
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Permission Denied</AlertTitle>
            <AlertDescription>
              You do not have permission to manage plugins. Admin role required.
            </AlertDescription>
          </Alert>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  // Fetch plugins
  const { data: pluginsResponse, isLoading, error, refetch } = useQuery({
    queryKey: ['plugins'],
    queryFn: () => apiClient.listPlugins(),
    staleTime: 30000,
  });

  // Enable plugin mutation
  const enableMutation = useMutation({
    mutationFn: async ({ pluginId, options }: { pluginId: string; options?: EnablePluginRequest }) => {
      return apiClient.enablePlugin(pluginId, options);
    },
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['plugins'] });
      toast.success(response.message || 'Plugin enabled successfully');
      logger.info('Plugin enabled', {
        component: 'PluginsPage',
        operation: 'enablePlugin',
        pluginId: response.plugin.name,
      });
      setActionDialogOpen(false);
      setSelectedPlugin(null);
    },
    onError: (error: Error) => {
      toast.error(`Failed to enable plugin: ${error.message}`);
      logger.error('Failed to enable plugin', {
        component: 'PluginsPage',
        operation: 'enablePlugin',
      }, error);
    },
  });

  // Disable plugin mutation
  const disableMutation = useMutation({
    mutationFn: async ({ pluginId, options }: { pluginId: string; options?: DisablePluginRequest }) => {
      return apiClient.disablePlugin(pluginId, options);
    },
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['plugins'] });
      toast.success(response.message || 'Plugin disabled successfully');
      if (response.warnings && response.warnings.length > 0) {
        response.warnings.forEach(warning => {
          toast.warning(warning);
        });
      }
      logger.info('Plugin disabled', {
        component: 'PluginsPage',
        operation: 'disablePlugin',
        pluginId: response.plugin.name,
      });
      setActionDialogOpen(false);
      setSelectedPlugin(null);
    },
    onError: (error: Error) => {
      toast.error(`Failed to disable plugin: ${error.message}`);
      logger.error('Failed to disable plugin', {
        component: 'PluginsPage',
        operation: 'disablePlugin',
      }, error);
    },
  });

  // Update plugin config mutation
  const updateConfigMutation = useMutation({
    mutationFn: async ({ pluginId, configJson }: { pluginId: string; configJson: string | null }) => {
      return apiClient.updatePluginConfig(pluginId, { config_json: configJson });
    },
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['plugins'] });
      toast.success('Plugin configuration updated successfully');
      logger.info('Plugin config updated', {
        component: 'PluginsPage',
        operation: 'updatePluginConfig',
        pluginId: response.config.plugin_name,
      });
      setConfigDialogOpen(false);
      setConfigPlugin(null);
      setConfigJson('');
      setConfigJsonError(null);
    },
    onError: (error: Error) => {
      toast.error(`Failed to update plugin config: ${error.message}`);
      logger.error('Failed to update plugin config', {
        component: 'PluginsPage',
        operation: 'updatePluginConfig',
      }, error);
    },
  });

  const plugins = pluginsResponse?.plugins || [];

  // Filter plugins based on search
  const filteredPlugins = plugins.filter((plugin) =>
    plugin.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    plugin.display_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    plugin.description.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleTogglePlugin = (plugin: PluginInfo) => {
    setSelectedPlugin(plugin);
    setActionType(plugin.status === 'enabled' ? 'disable' : 'enable');
    setActionDialogOpen(true);
  };

  const handleConfirmAction = () => {
    if (!selectedPlugin) return;

    if (actionType === 'enable') {
      enableMutation.mutate({ pluginId: selectedPlugin.name });
    } else {
      disableMutation.mutate({ pluginId: selectedPlugin.name });
    }
  };

  const handleOpenConfig = async (plugin: PluginInfo) => {
    setConfigPlugin(plugin);
    setLoadingConfig(true);
    setConfigJsonError(null);

    try {
      const response = await apiClient.getPluginConfig(plugin.name);
      if (response.config && response.config.config_json) {
        setConfigJson(response.config.config_json);
      } else {
        setConfigJson('{}');
      }
    } catch (error) {
      logger.error('Failed to load plugin config', {
        component: 'PluginsPage',
        operation: 'getPluginConfig',
        pluginName: plugin.name,
      }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to load plugin configuration');
      setConfigJson('{}');
    } finally {
      setLoadingConfig(false);
      setConfigDialogOpen(true);
    }
  };

  const handleSaveConfig = () => {
    if (!configPlugin) return;

    setConfigJsonError(null);

    if (configJson.trim()) {
      try {
        JSON.parse(configJson);
      } catch (e) {
        setConfigJsonError('Invalid JSON format');
        return;
      }
    }

    const configToSave = configJson.trim() ? configJson : null;
    updateConfigMutation.mutate({
      pluginId: configPlugin.name,
      configJson: configToSave,
    });
  };

  const getStatusBadge = (status: PluginInfo['status']) => {
    switch (status) {
      case 'enabled':
        return (
          <Badge variant="default" className="gap-1">
            <CheckCircle className="h-3 w-3" />
            Enabled
          </Badge>
        );
      case 'disabled':
        return (
          <Badge variant="secondary" className="gap-1">
            <XCircle className="h-3 w-3" />
            Disabled
          </Badge>
        );
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  if (isLoading) {
    return (
      <DensityProvider pageKey="plugins">
        <FeatureLayout
          title="Plugin Management"
          description="Enable, disable, and configure system plugins"
        >
          <LoadingState message="Loading plugins..." />
        </FeatureLayout>
      </DensityProvider>
    );
  }

  if (error) {
    const message = error instanceof Error ? error.message : String(error);
    return <ErrorView message={message} onRetry={refetch} />;
  }

  return (
    <DensityProvider pageKey="plugins">
      <FeatureLayout
        title="Plugin Management"
        description="Enable, disable, and configure system plugins"
        maxWidth="xl"
        contentPadding="default"
        secondaryActions={[
          {
            label: 'Refresh',
            onClick: () => refetch(),
            variant: 'outline',
            icon: RefreshCw,
          },
        ]}
      >
        <StatsGrid
          total={pluginsResponse?.total || 0}
          enabled={pluginsResponse?.enabled_count || 0}
          disabled={pluginsResponse?.disabled_count || 0}
        />

        <PluginsTableCard
          plugins={filteredPlugins}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          onConfigure={handleOpenConfig}
          onToggle={handleTogglePlugin}
          isMutating={enableMutation.isPending || disableMutation.isPending}
        />

        <ActionDialog
          open={actionDialogOpen}
          onOpenChange={setActionDialogOpen}
          actionType={actionType}
          plugin={selectedPlugin}
          onConfirm={handleConfirmAction}
          onCancel={() => {
            setActionDialogOpen(false);
            setSelectedPlugin(null);
          }}
          isPending={enableMutation.isPending || disableMutation.isPending}
        />

        <ConfigDialog
          open={configDialogOpen}
          onOpenChange={setConfigDialogOpen}
          plugin={configPlugin}
          configJson={configJson}
          onConfigChange={(value) => {
            setConfigJson(value);
            setConfigJsonError(null);
          }}
          error={configJsonError}
          isLoading={loadingConfig || updateConfigMutation.isPending}
          onSave={handleSaveConfig}
        />
      </FeatureLayout>
    </DensityProvider>
  );
}

export default PluginsPage;
