//! Admin Settings Page
//!
//! System-wide configuration and settings management.
//! Provides controls for system behavior, security, and integrations.
//!
//! Citation: Settings patterns from ui/src/pages/AdminPage.tsx
//! - FeatureLayout with sections
//! - Card-based settings groups

import { useState } from 'react';
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
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
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

// Settings sections
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
    id: 'security',
    title: 'Security',
    description: 'Authentication and authorization settings',
    icon: <Shield className="h-5 w-5" />,
  },
  {
    id: 'inference',
    title: 'Inference',
    description: 'Model inference configuration',
    icon: <Zap className="h-5 w-5" />,
  },
  {
    id: 'notifications',
    title: 'Notifications',
    description: 'Alert and notification preferences',
    icon: <Bell className="h-5 w-5" />,
  },
];

export function SettingsPage() {
  const queryClient = useQueryClient();
  const { can, userRole } = useRBAC();
  const { theme, setTheme } = useTheme();
  const [activeSection, setActiveSection] = useState('general');
  const [saving, setSaving] = useState(false);
  const [hasChanges, setHasChanges] = useState(false);

  // Local state for settings (would normally be fetched from API)
  const [settings, setSettings] = useState({
    // General
    systemName: 'AdapterOS',
    defaultTenant: 'default',
    maxConcurrentJobs: 4,
    enableTelemetry: true,
    debugMode: false,

    // Security
    sessionTimeout: 480, // minutes
    mfaRequired: false,
    allowPasswordReset: true,
    maxLoginAttempts: 5,
    tokenRotationInterval: 24, // hours
    requireHttps: true,

    // Inference
    defaultModel: 'llama3.2-3b',
    maxTokens: 4096,
    temperature: 0.7,
    enableStreaming: true,
    kSparseLimit: 8,
    memoryHeadroom: 15,

    // Notifications
    enableEmailNotifications: true,
    enableSlackIntegration: false,
    alertThreshold: 'high',
    digestFrequency: 'daily',
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

  const updateSetting = <K extends keyof typeof settings>(key: K, value: typeof settings[K]) => {
    setSettings(prev => ({ ...prev, [key]: value }));
    setHasChanges(true);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      // Simulate API call
      await new Promise(resolve => setTimeout(resolve, 1000));

      toast.success('Settings saved successfully');
      logger.info('Settings saved', {
        component: 'SettingsPage',
        operation: 'saveSettings',
        section: activeSection,
      });
      setHasChanges(false);
    } catch (error) {
      toast.error('Failed to save settings');
      logger.error('Failed to save settings', {
        component: 'SettingsPage',
        operation: 'saveSettings',
      }, error instanceof Error ? error : new Error(String(error)));
    } finally {
      setSaving(false);
    }
  };

  const handleExportSettings = () => {
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
              disabled={!hasChanges || saving}
            >
              {saving ? (
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
                      value={settings.systemName}
                      onChange={(e) => updateSetting('systemName', e.target.value)}
                    />
                    <p className="text-xs text-muted-foreground">
                      Display name for this installation
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="defaultTenant">Default Tenant</Label>
                    <Input
                      id="defaultTenant"
                      value={settings.defaultTenant}
                      onChange={(e) => updateSetting('defaultTenant', e.target.value)}
                    />
                    <p className="text-xs text-muted-foreground">
                      Default tenant for new users
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="maxConcurrentJobs">Max Concurrent Jobs</Label>
                    <Input
                      id="maxConcurrentJobs"
                      type="number"
                      min={1}
                      max={16}
                      value={settings.maxConcurrentJobs}
                      onChange={(e) => updateSetting('maxConcurrentJobs', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Maximum simultaneous training jobs
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
                      UI color theme preference
                    </p>
                  </div>
                </div>

                <Separator />

                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label>Enable Telemetry</Label>
                      <p className="text-sm text-muted-foreground">
                        Collect anonymous usage statistics
                      </p>
                    </div>
                    <Switch
                      checked={settings.enableTelemetry}
                      onCheckedChange={(checked) => updateSetting('enableTelemetry', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        Debug Mode
                        <Badge variant="outline" className="text-xs">Development</Badge>
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Enable verbose logging and debugging features
                      </p>
                    </div>
                    <Switch
                      checked={settings.debugMode}
                      onCheckedChange={(checked) => updateSetting('debugMode', checked)}
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Security Settings */}
          <TabsContent value="security" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Shield className="h-5 w-5" />
                  Security Settings
                </CardTitle>
                <CardDescription>
                  Authentication and authorization configuration
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="sessionTimeout">Session Timeout (minutes)</Label>
                    <Input
                      id="sessionTimeout"
                      type="number"
                      min={15}
                      max={1440}
                      value={settings.sessionTimeout}
                      onChange={(e) => updateSetting('sessionTimeout', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Idle session timeout (15-1440 minutes)
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="maxLoginAttempts">Max Login Attempts</Label>
                    <Input
                      id="maxLoginAttempts"
                      type="number"
                      min={3}
                      max={10}
                      value={settings.maxLoginAttempts}
                      onChange={(e) => updateSetting('maxLoginAttempts', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Account lockout threshold
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="tokenRotationInterval">Token Rotation (hours)</Label>
                    <Input
                      id="tokenRotationInterval"
                      type="number"
                      min={1}
                      max={168}
                      value={settings.tokenRotationInterval}
                      onChange={(e) => updateSetting('tokenRotationInterval', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      JWT token rotation interval
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
                      checked={settings.mfaRequired}
                      onCheckedChange={(checked) => updateSetting('mfaRequired', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        <Key className="h-4 w-4" />
                        Allow Password Reset
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Enable self-service password reset
                      </p>
                    </div>
                    <Switch
                      checked={settings.allowPasswordReset}
                      onCheckedChange={(checked) => updateSetting('allowPasswordReset', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label>Require HTTPS</Label>
                      <p className="text-sm text-muted-foreground">
                        Enforce HTTPS for all connections
                      </p>
                    </div>
                    <Switch
                      checked={settings.requireHttps}
                      onCheckedChange={(checked) => updateSetting('requireHttps', checked)}
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Inference Settings */}
          <TabsContent value="inference" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Zap className="h-5 w-5" />
                  Inference Settings
                </CardTitle>
                <CardDescription>
                  Model inference and generation configuration
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label htmlFor="defaultModel">Default Model</Label>
                    <Select
                      value={settings.defaultModel}
                      onValueChange={(value) => updateSetting('defaultModel', value)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="llama3.2-3b">Llama 3.2 3B</SelectItem>
                        <SelectItem value="llama3.2-1b">Llama 3.2 1B</SelectItem>
                        <SelectItem value="mistral-7b">Mistral 7B</SelectItem>
                        <SelectItem value="phi-3-mini">Phi-3 Mini</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Default base model for inference
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="maxTokens">Max Tokens</Label>
                    <Input
                      id="maxTokens"
                      type="number"
                      min={256}
                      max={32768}
                      value={settings.maxTokens}
                      onChange={(e) => updateSetting('maxTokens', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Maximum tokens per generation
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="temperature">Default Temperature</Label>
                    <Input
                      id="temperature"
                      type="number"
                      min={0}
                      max={2}
                      step={0.1}
                      value={settings.temperature}
                      onChange={(e) => updateSetting('temperature', parseFloat(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Sampling temperature (0.0 - 2.0)
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="kSparseLimit">K-Sparse Limit</Label>
                    <Input
                      id="kSparseLimit"
                      type="number"
                      min={1}
                      max={8}
                      value={settings.kSparseLimit}
                      onChange={(e) => updateSetting('kSparseLimit', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Maximum active adapters (MAX_K)
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="memoryHeadroom">Memory Headroom (%)</Label>
                    <Input
                      id="memoryHeadroom"
                      type="number"
                      min={5}
                      max={30}
                      value={settings.memoryHeadroom}
                      onChange={(e) => updateSetting('memoryHeadroom', parseInt(e.target.value))}
                    />
                    <p className="text-xs text-muted-foreground">
                      Reserved memory percentage
                    </p>
                  </div>
                </div>

                <Separator />

                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label>Enable Streaming</Label>
                      <p className="text-sm text-muted-foreground">
                        Enable token-by-token streaming responses
                      </p>
                    </div>
                    <Switch
                      checked={settings.enableStreaming}
                      onCheckedChange={(checked) => updateSetting('enableStreaming', checked)}
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Notification Settings */}
          <TabsContent value="notifications" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Bell className="h-5 w-5" />
                  Notification Settings
                </CardTitle>
                <CardDescription>
                  Alert and notification preferences
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="grid grid-cols-2 gap-6">
                  <div className="space-y-2">
                    <Label>Alert Threshold</Label>
                    <Select
                      value={settings.alertThreshold}
                      onValueChange={(value) => updateSetting('alertThreshold', value)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="low">Low (All alerts)</SelectItem>
                        <SelectItem value="medium">Medium</SelectItem>
                        <SelectItem value="high">High (Critical only)</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Minimum severity for notifications
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label>Digest Frequency</Label>
                    <Select
                      value={settings.digestFrequency}
                      onValueChange={(value) => updateSetting('digestFrequency', value)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="realtime">Real-time</SelectItem>
                        <SelectItem value="hourly">Hourly</SelectItem>
                        <SelectItem value="daily">Daily</SelectItem>
                        <SelectItem value="weekly">Weekly</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Summary email frequency
                    </p>
                  </div>
                </div>

                <Separator />

                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label>Email Notifications</Label>
                      <p className="text-sm text-muted-foreground">
                        Send notifications via email
                      </p>
                    </div>
                    <Switch
                      checked={settings.enableEmailNotifications}
                      onCheckedChange={(checked) => updateSetting('enableEmailNotifications', checked)}
                    />
                  </div>

                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <Label className="flex items-center gap-2">
                        Slack Integration
                        <Badge variant="outline" className="text-xs">Coming Soon</Badge>
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        Send notifications to Slack channels
                      </p>
                    </div>
                    <Switch
                      checked={settings.enableSlackIntegration}
                      onCheckedChange={(checked) => updateSetting('enableSlackIntegration', checked)}
                      disabled
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default SettingsPage;
