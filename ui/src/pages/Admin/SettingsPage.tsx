//! Admin Settings Page
//!
//! System-wide configuration and settings management.
//! Provides controls for system behavior, security, and integrations.
//!
//! Citation: Settings patterns from ui/src/pages/AdminPage.tsx
//! - FeatureLayout with sections
//! - Card-based settings groups

import React, { useState, useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageHeader } from '@/components/ui/page-header';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Separator } from '@/components/ui/separator';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { LoadingState } from '@/components/ui/loading-state';
import { useRBAC } from '@/hooks/useRBAC';
import { useTheme } from '@/layout/LayoutProvider';
import { useSettings, useUpdateSettings } from '@/hooks/useSettings';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import * as securityApi from '@/api/security';
import type { UpdateSettingsRequest, GeneralSettings, ServerSettings, SecuritySettings, PerformanceSettings } from '@/api/document-types';
import {
  Settings,
  RefreshCw,
  AlertCircle,
  Save,
  Shield,
  Database,
  Bell,
  Palette,
  Clock,
  Zap,
  Server,
  Lock,
  Key,
  Eye,
  EyeOff,
  CheckCircle,
  Info,
  Loader2,
  Download,
  Upload,
} from 'lucide-react';

// Settings sections mapped to backend structure
interface SettingsSection {
  id: string;
  title: string;
  description: string;
  icon: React.ReactNode;
}

const settingsSections: SettingsSection[] = [
  {
    id: 'general',
    title: 'General',
    description: 'Basic system configuration',
    icon: <Settings className="h-5 w-5" />,
  },
  {
    id: 'server',
    title: 'Server',
    description: 'Server configuration and networking',
    icon: <Server className="h-5 w-5" />,
  },
  {
    id: 'security',
    title: 'Security',
    description: 'Authentication and authorization settings',
    icon: <Shield className="h-5 w-5" />,
  },
  {
    id: 'performance',
    title: 'Performance',
    description: 'Performance tuning and resource limits',
    icon: <Zap className="h-5 w-5" />,
  },
];

export function SettingsPage() {
  const queryClient = useQueryClient();
  const { can, userRole } = useRBAC();
  const { theme, setTheme } = useTheme();
  const [activeSection, setActiveSection] = useState('general');
  const [hasChanges, setHasChanges] = useState(false);
  const [restartRequired, setRestartRequired] = useState(false);

  // Fetch settings from backend
  const { data: settings, isLoading, error } = useSettings();
  const updateSettings = useUpdateSettings();

  // Local form state for editing (initialized from backend)
  const [formData, setFormData] = useState<UpdateSettingsRequest>({});

  // Fetch security info from API (for JWT config and key rotation)
  const { data: securityInfo, isLoading: securityLoading, error: securityError } = useQuery({
    queryKey: ['security-info'],
    queryFn: securityApi.getSecurityInfo,
    refetchInterval: 60000, // Refresh every minute
  });

  // JWT configuration state (separate from other settings, managed by API)
  const [jwtConfig, setJwtConfig] = useState<securityApi.JwtConfig>({
    mode: securityInfo?.jwtMode || 'eddsa',
    ttlMinutes: securityInfo?.tokenTtlMinutes || 480,
    requireHttps: securityInfo?.requireHttps || false,
  });

  // Initialize form data when settings load
  useEffect(() => {
    if (settings) {
      setFormData({
        general: { ...settings.general },
        server: { ...settings.server },
        security: { ...settings.security },
        performance: { ...settings.performance },
      });
    }
  }, [settings]);

  // Update JWT config when security info loads
  useEffect(() => {
    if (securityInfo) {
      setJwtConfig({
        mode: securityInfo.jwtMode,
        ttlMinutes: securityInfo.tokenTtlMinutes,
        requireHttps: securityInfo.requireHttps,
      });
    }
  }, [securityInfo]);

  // Mutation for updating JWT config
  const updateJwtConfigMutation = useMutation({
    mutationFn: securityApi.updateJwtConfig,
    onSuccess: (data) => {
      toast.success('JWT configuration updated successfully');
      queryClient.invalidateQueries({ queryKey: ['security-info'] });
      logger.info('JWT config updated', {
        component: 'SettingsPage',
        operation: 'updateJwtConfig',
        mode: data.jwtMode,
      });
    },
    onError: (error) => {
      toast.error(error instanceof Error ? error.message : 'Failed to update JWT configuration');
      logger.error('Failed to update JWT config', {
        component: 'SettingsPage',
        operation: 'updateJwtConfig',
      }, error instanceof Error ? error : new Error(String(error)));
    },
  });

  // Mutation for key rotation
  const rotateKeysMutation = useMutation({
    mutationFn: securityApi.rotateKeys,
    onSuccess: (data) => {
      toast.success(`Keys rotated successfully. New fingerprint: ${data.newFingerprint}`);
      queryClient.invalidateQueries({ queryKey: ['security-info'] });
      logger.info('Keys rotated', {
        component: 'SettingsPage',
        operation: 'rotateKeys',
        newFingerprint: data.newFingerprint,
      });
    },
    onError: (error) => {
      toast.error(error instanceof Error ? error.message : 'Failed to rotate keys');
      logger.error('Failed to rotate keys', {
        component: 'SettingsPage',
        operation: 'rotateKeys',
      }, error instanceof Error ? error : new Error(String(error)));
    },
  });

  // Check admin permissions
  if (userRole !== 'admin') {
    return (
      <DensityProvider pageKey="settings">
        <FeatureLayout title="Settings">
          <PageHeader
            title="System Settings"
            description="Configure system-wide settings"
          />
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Permission Denied</AlertTitle>
            <AlertDescription>
              You do not have permission to access system settings. Admin role required.
            </AlertDescription>
          </Alert>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  // Update general settings
  const updateGeneralSetting = <K extends keyof GeneralSettings>(key: K, value: GeneralSettings[K]) => {
    setFormData(prev => ({
      ...prev,
      general: { ...prev.general, [key]: value },
    }));
    setHasChanges(true);
  };

  // Update server settings
  const updateServerSetting = <K extends keyof ServerSettings>(key: K, value: ServerSettings[K]) => {
    setFormData(prev => ({
      ...prev,
      server: { ...prev.server, [key]: value },
    }));
    setHasChanges(true);
  };

  // Update security settings
  const updateSecuritySetting = <K extends keyof SecuritySettings>(key: K, value: SecuritySettings[K]) => {
    setFormData(prev => ({
      ...prev,
      security: { ...prev.security, [key]: value },
    }));
    setHasChanges(true);
  };

  // Update performance settings
  const updatePerformanceSetting = <K extends keyof PerformanceSettings>(key: K, value: PerformanceSettings[K]) => {
    setFormData(prev => ({
      ...prev,
      performance: { ...prev.performance, [key]: value },
    }));
    setHasChanges(true);
  };

  const handleSave = async () => {
    try {
      const response = await updateSettings.mutateAsync(formData);

      logger.info('Settings saved', {
        component: 'SettingsPage',
        operation: 'saveSettings',
        section: activeSection,
      });

      setHasChanges(false);
      setRestartRequired(response.restart_required);
    } catch (error) {
      logger.error('Failed to save settings', {
        component: 'SettingsPage',
        operation: 'saveSettings',
      }, error instanceof Error ? error : new Error(String(error)));
    }
  };

  const handleExportSettings = () => {
    if (!settings) return;

    const exportData = JSON.stringify(settings, null, 2);
    const blob = new Blob([exportData], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'adapteros-settings.json';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    toast.success('Settings exported successfully');
  };

  // Show loading state
  if (isLoading) {
    return (
      <DensityProvider pageKey="settings">
        <FeatureLayout title="Settings">
          <LoadingState message="Loading settings..." />
        </FeatureLayout>
      </DensityProvider>
    );
  }

  // Show error state
  if (error) {
    return (
      <DensityProvider pageKey="settings">
        <FeatureLayout title="Settings">
          <PageHeader
            title="System Settings"
            description="Configure system-wide settings"
          />
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Error Loading Settings</AlertTitle>
            <AlertDescription>
              {error instanceof Error ? error.message : 'Failed to load settings'}
            </AlertDescription>
          </Alert>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="settings">
      <FeatureLayout
        title="Settings"
        maxWidth="xl"
        contentPadding="default"
      >
        <PageHeader
          title="System Settings"
          description="Configure system-wide settings and preferences"
        >
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleExportSettings}
            >
              <Download className="h-4 w-4 mr-2" />
              Export
            </Button>
            <Button
              size="sm"
              onClick={handleSave}
              disabled={!hasChanges || updateSettings.isPending}
            >
              {updateSettings.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Saving...
                </>
              ) : (
                <>
                  <Save className="h-4 w-4 mr-2" />
                  Save Changes
                </>
              )}
            </Button>
          </div>
        </PageHeader>

        {restartRequired && (
          <Alert className="mb-6">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Restart Required</AlertTitle>
            <AlertDescription>
              Some changes require a server restart to take effect. Please restart the server when convenient.
            </AlertDescription>
          </Alert>
        )}

        {hasChanges && (
          <Alert className="mb-6">
            <Info className="h-4 w-4" />
            <AlertTitle>Unsaved Changes</AlertTitle>
            <AlertDescription>
              You have unsaved changes. Click "Save Changes" to apply them.
            </AlertDescription>
          </Alert>
        )}

        <Tabs value={activeSection} onValueChange={setActiveSection} className="space-y-6">
          <TabsList className="grid grid-cols-4 w-full max-w-2xl">
            {settingsSections.map((section) => (
              <TabsTrigger key={section.id} value={section.id} className="gap-2">
                {section.icon}
                {section.title}
              </TabsTrigger>
            ))}
          </TabsList>

          {/* General Settings */}
          <TabsContent value="general" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Settings className="h-5 w-5" />
                  General Settings
                </CardTitle>
                <CardDescription>
                  Basic system configuration options
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="systemName">System Name</Label>
                    <Input
                      id="systemName"
                      value={formData.general?.system_name || ''}
                      onChange={(e) => updateGeneralSetting('system_name', e.target.value)}
                    />
                    <p className="text-xs text-muted-foreground">
                      Display name for this installation
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="environment">Environment</Label>
                    <Select
                      value={formData.general?.environment || 'development'}
                      onValueChange={(value) => updateGeneralSetting('environment', value)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="development">Development</SelectItem>
                        <SelectItem value="staging">Staging</SelectItem>
                        <SelectItem value="production">Production</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Current deployment environment
                    </p>
                  </div>

                  <div className="space-y-2 col-span-2">
                    <Label htmlFor="apiBaseUrl">API Base URL</Label>
                    <Input
                      id="apiBaseUrl"
                      value={formData.general?.api_base_url || ''}
                      onChange={(e) => updateGeneralSetting('api_base_url', e.target.value)}
                      placeholder="http://localhost:8080"
                    />
                    <p className="text-xs text-muted-foreground">
                      Base URL for API endpoints
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label>Theme</Label>
                    <Select value={theme} onValueChange={(value: 'light' | 'dark' | 'system') => setTheme(value)}>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="light">Light</SelectItem>
                        <SelectItem value="dark">Dark</SelectItem>
                        <SelectItem value="system">System</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      UI color theme preference (local only)
                    </p>
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Server Settings */}
          <TabsContent value="server" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Server className="h-5 w-5" />
                  Server Configuration
                </CardTitle>
                <CardDescription>
                  Server ports and networking settings
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="httpPort">HTTP Port</Label>
                    <Input
                      id="httpPort"
                      type="number"
                      min={1024}
                      max={65535}
                      value={formData.server?.http_port || 8080}
                      onChange={(e) => updateServerSetting('http_port', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Port for HTTP server
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="httpsPort">HTTPS Port</Label>
                    <Input
                      id="httpsPort"
                      type="number"
                      min={1024}
                      max={65535}
                      value={formData.server?.https_port || ''}
                      onChange={(e) => updateServerSetting('https_port', e.target.value ? parseInt(e.target.value) : null)}
                      placeholder="Optional"
                    />
                    <p className="text-xs text-muted-foreground">
                      Port for HTTPS server (optional)
                    </p>
                  </div>

                  <div className="space-y-2 col-span-2">
                    <Label htmlFor="udsSocket">UDS Socket Path</Label>
                    <Input
                      id="udsSocket"
                      value={formData.server?.uds_socket_path || ''}
                      onChange={(e) => updateServerSetting('uds_socket_path', e.target.value || null)}
                      placeholder="/var/run/adapteros.sock"
                    />
                    <p className="text-xs text-muted-foreground">
                      Unix domain socket path (required in production mode)
                    </p>
                  </div>
                </div>

                <Separator />

                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        <Lock className="h-4 w-4" />
                        Production Mode
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Enable production security policies
                      </p>
                    </div>
                    <Switch
                      checked={formData.server?.production_mode || false}
                      onCheckedChange={(checked) => updateServerSetting('production_mode', checked)}
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Security Settings */}
          <TabsContent value="security" className="space-y-6">
            {/* JWT & Key Management */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Key className="h-5 w-5" />
                  JWT Configuration
                </CardTitle>
                <CardDescription>
                  JSON Web Token settings and key management
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                {securityLoading ? (
                  <LoadingState message="Loading security configuration..." />
                ) : securityError ? (
                  <Alert variant="destructive">
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Error Loading Security Info</AlertTitle>
                    <AlertDescription>
                      {securityError instanceof Error ? securityError.message : 'Unknown error'}
                    </AlertDescription>
                  </Alert>
                ) : (
                  <>
                    {/* Key Fingerprint Display */}
                    <div className="space-y-2">
                      <Label>Current Key Fingerprint</Label>
                      <div className="flex items-center gap-2">
                        <Input
                          value={securityInfo?.keyFingerprint || 'Loading...'}
                          readOnly
                          className="font-mono text-sm"
                        />
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => {
                            if (securityInfo?.keyFingerprint) {
                              navigator.clipboard.writeText(securityInfo.keyFingerprint);
                              toast.success('Fingerprint copied to clipboard');
                            }
                          }}
                        >
                          Copy
                        </Button>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        Ed25519 public key fingerprint for JWT signing
                      </p>
                    </div>

                    <div className="grid grid-cols-2 gap-6">
                      {/* JWT Mode */}
                      <div className="space-y-2">
                        <Label htmlFor="jwtMode">JWT Signing Mode</Label>
                        <Select
                          value={jwtConfig.mode}
                          onValueChange={(value: 'eddsa' | 'hmac') => {
                            setJwtConfig({ ...jwtConfig, mode: value });
                            setHasChanges(true);
                          }}
                        >
                          <SelectTrigger id="jwtMode">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="eddsa">Ed25519 (EdDSA)</SelectItem>
                            <SelectItem value="hmac">HMAC-SHA256</SelectItem>
                          </SelectContent>
                        </Select>
                        <p className="text-xs text-muted-foreground">
                          Current: {securityInfo?.jwtMode.toUpperCase()}
                        </p>
                      </div>

                      {/* Token TTL */}
                      <div className="space-y-2">
                        <Label htmlFor="tokenTtl">Token TTL (minutes)</Label>
                        <Input
                          id="tokenTtl"
                          type="number"
                          min={15}
                          max={1440}
                          value={jwtConfig.ttlMinutes}
                          onChange={(e) => {
                            setJwtConfig({ ...jwtConfig, ttlMinutes: parseInt(e.target.value) || 480 });
                            setHasChanges(true);
                          }}
                        />
                        <p className="text-xs text-muted-foreground">
                          Current: {securityInfo?.tokenTtlMinutes} minutes
                        </p>
                      </div>
                    </div>

                    {/* Key Metadata */}
                    <div className="space-y-2 p-4 bg-slate-50 dark:bg-slate-900 rounded-lg">
                      <div className="flex items-center justify-between text-sm">
                        <span className="text-muted-foreground">Created:</span>
                        <span className="font-mono">
                          {securityInfo?.createdAt ? new Date(securityInfo.createdAt).toLocaleString() : 'N/A'}
                        </span>
                      </div>
                      {securityInfo?.lastRotated && (
                        <div className="flex items-center justify-between text-sm">
                          <span className="text-muted-foreground">Last Rotated:</span>
                          <span className="font-mono">
                            {new Date(securityInfo.lastRotated).toLocaleString()}
                          </span>
                        </div>
                      )}
                      <div className="flex items-center justify-between text-sm">
                        <span className="text-muted-foreground">Production Mode:</span>
                        <Badge variant={securityInfo?.productionMode ? 'default' : 'secondary'}>
                          {securityInfo?.productionMode ? 'Enabled' : 'Disabled'}
                        </Badge>
                      </div>
                    </div>

                    <Separator />

                    {/* Action Buttons */}
                    <div className="flex items-center gap-4">
                      <Button
                        onClick={() => updateJwtConfigMutation.mutate(jwtConfig)}
                        disabled={!hasChanges || updateJwtConfigMutation.isPending}
                      >
                        {updateJwtConfigMutation.isPending ? (
                          <>
                            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                            Updating...
                          </>
                        ) : (
                          <>
                            <Save className="h-4 w-4 mr-2" />
                            Update JWT Config
                          </>
                        )}
                      </Button>

                      <Button
                        variant="destructive"
                        onClick={() => {
                          if (confirm('Rotate signing keys? This will invalidate all existing tokens.')) {
                            rotateKeysMutation.mutate();
                          }
                        }}
                        disabled={rotateKeysMutation.isPending}
                      >
                        {rotateKeysMutation.isPending ? (
                          <>
                            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                            Rotating...
                          </>
                        ) : (
                          <>
                            <RefreshCw className="h-4 w-4 mr-2" />
                            Rotate Keys
                          </>
                        )}
                      </Button>
                    </div>

                    <Alert>
                      <Info className="h-4 w-4" />
                      <AlertTitle>Key Rotation Warning</AlertTitle>
                      <AlertDescription>
                        Rotating keys will invalidate all existing JWT tokens. All users will need to re-authenticate.
                      </AlertDescription>
                    </Alert>
                  </>
                )}
              </CardContent>
            </Card>

            {/* Other Security Settings */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Shield className="h-5 w-5" />
                  Access Control
                </CardTitle>
                <CardDescription>
                  Authentication and authorization policies
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="tokenTtl">Token TTL (seconds)</Label>
                    <Input
                      id="tokenTtl"
                      type="number"
                      min={900}
                      max={86400}
                      value={formData.security?.token_ttl_seconds || 28800}
                      onChange={(e) => updateSecuritySetting('token_ttl_seconds', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      JWT token time-to-live (15 min - 24 hours)
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="jwtMode">JWT Mode</Label>
                    <Select
                      value={formData.security?.jwt_mode || 'eddsa'}
                      onValueChange={(value: 'eddsa' | 'hmac') => updateSecuritySetting('jwt_mode', value)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="eddsa">EdDSA (Ed25519)</SelectItem>
                        <SelectItem value="hmac">HMAC-SHA256</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      JWT signing algorithm
                    </p>
                  </div>
                </div>

                <Separator />

                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        <Lock className="h-4 w-4" />
                        Require MFA
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Enforce multi-factor authentication for all users
                      </p>
                    </div>
                    <Switch
                      checked={formData.security?.require_mfa || false}
                      onCheckedChange={(checked) => updateSecuritySetting('require_mfa', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        <Server className="h-4 w-4" />
                        Enable Egress
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Allow outbound network connections
                      </p>
                    </div>
                    <Switch
                      checked={formData.security?.egress_enabled || false}
                      onCheckedChange={(checked) => updateSecuritySetting('egress_enabled', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label>Require PF Deny</Label>
                      <p className="text-sm text-muted-foreground">
                        Require packet filter deny rules
                      </p>
                    </div>
                    <Switch
                      checked={formData.security?.require_pf_deny || false}
                      onCheckedChange={(checked) => updateSecuritySetting('require_pf_deny', checked)}
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Performance Settings */}
          <TabsContent value="performance" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Zap className="h-5 w-5" />
                  Performance Configuration
                </CardTitle>
                <CardDescription>
                  Resource limits and performance tuning
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="maxAdapters">Max Adapters</Label>
                    <Input
                      id="maxAdapters"
                      type="number"
                      min={1}
                      max={100}
                      value={formData.performance?.max_adapters || 8}
                      onChange={(e) => updatePerformanceSetting('max_adapters', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Maximum number of concurrent adapters
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="maxWorkers">Max Workers</Label>
                    <Input
                      id="maxWorkers"
                      type="number"
                      min={1}
                      max={32}
                      value={formData.performance?.max_workers || 4}
                      onChange={(e) => updatePerformanceSetting('max_workers', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Maximum number of worker threads
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="memoryThreshold">Memory Threshold (%)</Label>
                    <Input
                      id="memoryThreshold"
                      type="number"
                      min={50}
                      max={95}
                      value={formData.performance?.memory_threshold_pct || 85}
                      onChange={(e) => updatePerformanceSetting('memory_threshold_pct', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Memory usage threshold for eviction (50-95%)
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="cacheSize">Cache Size (MB)</Label>
                    <Input
                      id="cacheSize"
                      type="number"
                      min={128}
                      max={8192}
                      value={formData.performance?.cache_size_mb || 1024}
                      onChange={(e) => updatePerformanceSetting('cache_size_mb', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Cache size for adapter weights (128-8192 MB)
                    </p>
                  </div>
                </div>

                <Alert>
                  <Info className="h-4 w-4" />
                  <AlertTitle>Performance Impact</AlertTitle>
                  <AlertDescription>
                    Changing these settings may affect system performance and memory usage.
                    Adjust carefully based on available resources.
                  </AlertDescription>
                </Alert>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default SettingsPage;
