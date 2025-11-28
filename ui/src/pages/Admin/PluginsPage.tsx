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
    return (
      <DensityProvider pageKey="plugins">
        <FeatureLayout
          title="Plugin Management"
          description="Enable, disable, and configure system plugins"
        >
          <ErrorRecovery
            error={error instanceof Error ? error.message : String(error)}
            onRetry={refetch}
          />
        </FeatureLayout>
      </DensityProvider>
    );
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

        {/* Stats Cards */}
        <div className="grid grid-cols-3 gap-4 mb-6">
          <Card>
            <CardContent className="pt-6">
              <div className="flex items-center gap-4">
                <div className="p-3 bg-primary/10 rounded-lg">
                  <Package className="h-6 w-6 text-primary" />
                </div>
                <div>
                  <p className="text-sm text-muted-foreground">Total Plugins</p>
                  <p className="text-2xl font-bold">{pluginsResponse?.total || 0}</p>
                </div>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="pt-6">
              <div className="flex items-center gap-4">
                <div className="p-3 bg-green-500/10 rounded-lg">
                  <CheckCircle className="h-6 w-6 text-green-500" />
                </div>
                <div>
                  <p className="text-sm text-muted-foreground">Enabled</p>
                  <p className="text-2xl font-bold">{pluginsResponse?.enabled_count || 0}</p>
                </div>
              </div>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="pt-6">
              <div className="flex items-center gap-4">
                <div className="p-3 bg-muted rounded-lg">
                  <XCircle className="h-6 w-6 text-muted-foreground" />
                </div>
                <div>
                  <p className="text-sm text-muted-foreground">Disabled</p>
                  <p className="text-2xl font-bold">{pluginsResponse?.disabled_count || 0}</p>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Plugins Table */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <Plug className="h-5 w-5" />
                  Installed Plugins
                </CardTitle>
                <CardDescription>
                  Manage plugins installed on this system
                </CardDescription>
              </div>
              <div className="relative w-64">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Search plugins..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-9"
                />
              </div>
            </div>
          </CardHeader>
          <CardContent>
            {filteredPlugins.length === 0 ? (
              <div className="text-center py-12">
                <Plug className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">
                  {plugins.length === 0 ? 'No Plugins Installed' : 'No Matching Plugins'}
                </h3>
                <p className="text-muted-foreground">
                  {plugins.length === 0
                    ? 'There are no plugins installed on this system.'
                    : 'No plugins match your search criteria.'}
                </p>
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
                  {filteredPlugins.map((plugin) => (
                    <TableRow key={plugin.name}>
                      <TableCell>
                        <div className="flex flex-col gap-1">
                          <span className="font-medium">{plugin.display_name}</span>
                          <span className="text-sm text-muted-foreground">
                            {plugin.description}
                          </span>
                          <code className="text-xs text-muted-foreground">
                            {plugin.name}
                          </code>
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">{plugin.version}</Badge>
                      </TableCell>
                      <TableCell>{getStatusBadge(plugin.status)}</TableCell>
                      <TableCell>
                        {plugin.author || <span className="text-muted-foreground">-</span>}
                      </TableCell>
                      <TableCell>
                        {plugin.enabled_tenants && plugin.enabled_tenants.length > 0 ? (
                          <div className="flex gap-1 flex-wrap">
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
                          <span className="text-muted-foreground text-sm">Global</span>
                        )}
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex items-center justify-end gap-2">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleOpenConfig(plugin)}
                            title="Configure plugin"
                          >
                            <Settings2 className="h-4 w-4" />
                          </Button>
                          <Switch
                            checked={plugin.status === 'enabled'}
                            onCheckedChange={() => handleTogglePlugin(plugin)}
                            disabled={enableMutation.isPending || disableMutation.isPending}
                          />
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>

        {/* Confirmation Dialog */}
        <Dialog open={actionDialogOpen} onOpenChange={setActionDialogOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>
                {actionType === 'enable' ? 'Enable Plugin' : 'Disable Plugin'}
              </DialogTitle>
              <DialogDescription>
                {actionType === 'enable'
                  ? `Are you sure you want to enable "${selectedPlugin?.display_name}"? This will activate the plugin for all organizations.`
                  : `Are you sure you want to disable "${selectedPlugin?.display_name}"? This may affect functionality for organizations using this plugin.`}
              </DialogDescription>
            </DialogHeader>

            {selectedPlugin && (
              <div className="space-y-4 py-4">
                <div className="flex items-center gap-4 p-4 bg-muted rounded-lg">
                  <Plug className="h-8 w-8 text-muted-foreground" />
                  <div>
                    <p className="font-medium">{selectedPlugin.display_name}</p>
                    <p className="text-sm text-muted-foreground">{selectedPlugin.description}</p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Version: {selectedPlugin.version}
                    </p>
                  </div>
                </div>

                {actionType === 'disable' && selectedPlugin.enabled_tenants && selectedPlugin.enabled_tenants.length > 0 && (
                  <Alert>
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Warning</AlertTitle>
                    <AlertDescription>
                      This plugin is currently enabled for {selectedPlugin.enabled_tenants.length} organization(s).
                      Disabling it may affect their workflows.
                    </AlertDescription>
                  </Alert>
                )}
              </div>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => {
                  setActionDialogOpen(false);
                  setSelectedPlugin(null);
                }}
              >
                Cancel
              </Button>
              <Button
                variant={actionType === 'disable' ? 'destructive' : 'default'}
                onClick={handleConfirmAction}
                disabled={enableMutation.isPending || disableMutation.isPending}
              >
                {(enableMutation.isPending || disableMutation.isPending) ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    {actionType === 'enable' ? 'Enabling...' : 'Disabling...'}
                  </>
                ) : (
                  <>
                    {actionType === 'enable' ? (
                      <CheckCircle className="h-4 w-4 mr-2" />
                    ) : (
                      <XCircle className="h-4 w-4 mr-2" />
                    )}
                    {actionType === 'enable' ? 'Enable' : 'Disable'}
                  </>
                )}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* Configuration Dialog */}
        <Dialog open={configDialogOpen} onOpenChange={setConfigDialogOpen}>
          <DialogContent className="max-w-2xl">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <Settings2 className="h-5 w-5" />
                Plugin Configuration
              </DialogTitle>
              <DialogDescription>
                Configure settings for {configPlugin?.display_name}
              </DialogDescription>
            </DialogHeader>

            {configPlugin && (
              <div className="space-y-4 py-4">
                <div className="flex items-center gap-4 p-4 bg-muted rounded-lg">
                  <Plug className="h-8 w-8 text-muted-foreground" />
                  <div className="flex-1">
                    <p className="font-medium">{configPlugin.display_name}</p>
                    <p className="text-sm text-muted-foreground">{configPlugin.description}</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline" className="text-xs">{configPlugin.version}</Badge>
                      {getStatusBadge(configPlugin.status)}
                    </div>
                  </div>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="config-json">Configuration JSON</Label>
                  <Textarea
                    id="config-json"
                    value={configJson}
                    onChange={(e) => {
                      setConfigJson(e.target.value);
                      setConfigJsonError(null);
                    }}
                    placeholder='{"key": "value"}'
                    className="font-mono text-sm min-h-[300px]"
                    disabled={loadingConfig || updateConfigMutation.isPending}
                  />
                  {configJsonError && (
                    <p className="text-sm text-destructive">{configJsonError}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Enter plugin configuration as JSON. Leave empty or enter {} for default settings.
                  </p>
                </div>

                {loadingConfig && (
                  <div className="flex items-center justify-center py-8">
                    <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                  </div>
                )}
              </div>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => {
                  setConfigDialogOpen(false);
                  setConfigPlugin(null);
                  setConfigJson('');
                  setConfigJsonError(null);
                }}
                disabled={updateConfigMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                onClick={handleSaveConfig}
                disabled={loadingConfig || updateConfigMutation.isPending || !!configJsonError}
              >
                {updateConfigMutation.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Saving...
                  </>
                ) : (
                  <>
                    <Save className="h-4 w-4 mr-2" />
                    Save Configuration
                  </>
                )}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default PluginsPage;
