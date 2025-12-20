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
import { PermissionDenied } from '@/components/ui/permission-denied';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useTheme } from '@/providers/CoreProviders';
import { useSettings, useUpdateSettings } from '@/hooks/config/useSettings';
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
  Zap,
  Server,
  Lock,
  Key,
  Info,
  Loader2,
  Download,
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

type SecurityInfo = Awaited<ReturnType<typeof securityApi.getSecurityInfo>>;

type FieldBlockProps = {
  id?: string;
  label: string;
  description?: string;
  control: React.ReactNode;
};

type ToggleRowProps = {
  title: string;
  description: string;
  icon?: React.ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
};

type GeneralSettingsCardProps = {
  formData: UpdateSettingsRequest;
  updateGeneralSetting: <K extends keyof GeneralSettings>(key: K, value: GeneralSettings[K]) => void;
  theme: 'light' | 'dark' | 'system';
  setTheme: (value: 'light' | 'dark' | 'system') => void;
};

type ServerSettingsCardProps = {
  formData: UpdateSettingsRequest;
  updateServerSetting: <K extends keyof ServerSettings>(key: K, value: ServerSettings[K]) => void;
};

type JwtConfigurationCardProps = {
  securityLoading: boolean;
  securityError: unknown;
  securityInfo?: SecurityInfo;
  jwtConfig: securityApi.JwtConfig;
  setJwtConfig: React.Dispatch<React.SetStateAction<securityApi.JwtConfig>>;
  onUpdateJwtConfig: () => void;
  onRotateKeys: () => void;
  isUpdatingJwtConfig: boolean;
  isRotatingKeys: boolean;
  hasChanges: boolean;
  setHasChanges: (value: boolean) => void;
};

type AccessControlCardProps = {
  formData: UpdateSettingsRequest;
  updateSecuritySetting: <K extends keyof SecuritySettings>(key: K, value: SecuritySettings[K]) => void;
};

type PerformanceSettingsCardProps = {
  formData: UpdateSettingsRequest;
  updatePerformanceSetting: <K extends keyof PerformanceSettings>(key: K, value: PerformanceSettings[K]) => void;
};

type SettingsTabsProps = {
  activeSection: string;
  onSectionChange: (value: string) => void;
  formData: UpdateSettingsRequest;
  theme: 'light' | 'dark' | 'system';
  setTheme: (value: 'light' | 'dark' | 'system') => void;
  updateGeneralSetting: <K extends keyof GeneralSettings>(key: K, value: GeneralSettings[K]) => void;
  updateServerSetting: <K extends keyof ServerSettings>(key: K, value: ServerSettings[K]) => void;
  updateSecuritySetting: <K extends keyof SecuritySettings>(key: K, value: SecuritySettings[K]) => void;
  updatePerformanceSetting: <K extends keyof PerformanceSettings>(key: K, value: PerformanceSettings[K]) => void;
  securityLoading: boolean;
  securityError: unknown;
  securityInfo?: SecurityInfo;
  jwtConfig: securityApi.JwtConfig;
  setJwtConfig: React.Dispatch<React.SetStateAction<securityApi.JwtConfig>>;
  onUpdateJwtConfig: () => void;
  onRotateKeys: () => void;
  isUpdatingJwtConfig: boolean;
  isRotatingKeys: boolean;
  hasChanges: boolean;
  setHasChanges: (value: boolean) => void;
};

type SettingsContentProps = SettingsTabsProps & {
  restartRequired: boolean;
  hasChanges: boolean;
};

function FieldBlock({ id, label, description, control }: FieldBlockProps) {
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      {control}
      {description && <p className="text-xs text-muted-foreground">{description}</p>}
    </div>
  );
}

function ToggleRow({ title, description, icon, checked, onChange }: ToggleRowProps) {
  return (
    <div className="flex items-center justify-between">
      <div className="space-y-0.5">
        <Label className="flex items-center gap-2">
          {icon}
          {title}
        </Label>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function PermissionDeniedView() {
  return (
    <DensityProvider pageKey="settings">
      <FeatureLayout title="System Settings" description="Configure system-wide settings">
        <PermissionDenied
          requiredPermission="tenant:manage"
          requiredRoles={['admin', 'developer']}
        />
      </FeatureLayout>
    </DensityProvider>
  );
}

function LoadingSettingsView() {
  return (
    <DensityProvider pageKey="settings">
      <FeatureLayout title="System Settings" description="Configure system-wide settings">
        <LoadingState message="Loading settings..." />
      </FeatureLayout>
    </DensityProvider>
  );
}

function ErrorSettingsView({ message }: { message: string }) {
  return (
    <DensityProvider pageKey="settings">
      <FeatureLayout title="System Settings" description="Configure system-wide settings">
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Error Loading Settings</AlertTitle>
          <AlertDescription>{message}</AlertDescription>
        </Alert>
      </FeatureLayout>
    </DensityProvider>
  );
}

const RestartAlert = () => (
  <Alert className="mb-6">
    <AlertCircle className="h-4 w-4" />
    <AlertTitle>Restart Required</AlertTitle>
    <AlertDescription>
      Some changes require a server restart to take effect. Please restart the server when convenient.
    </AlertDescription>
  </Alert>
);

const UnsavedChangesAlert = () => (
  <Alert className="mb-6">
    <Info className="h-4 w-4" />
    <AlertTitle>Unsaved Changes</AlertTitle>
    <AlertDescription>You have unsaved changes. Click "Save Changes" to apply them.</AlertDescription>
  </Alert>
);

function GeneralSettingsCard({
  formData,
  updateGeneralSetting,
  theme,
  setTheme,
}: GeneralSettingsCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Settings className="h-5 w-5" />
          General Settings
        </CardTitle>
        <CardDescription>Basic system configuration options</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="grid grid-cols-2 gap-6">
          <FieldBlock
            id="systemName"
            label="System Name"
            description="Display name for this installation"
            control={
              <Input
                id="systemName"
                value={formData.general?.system_name || ''}
                onChange={(e) => updateGeneralSetting('system_name', e.target.value)}
              />
            }
          />
          <FieldBlock
            id="environment"
            label="Environment"
            description="Current deployment environment"
            control={
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
            }
          />
          <FieldBlock
            id="apiBaseUrl"
            label="API Base URL"
            description="Base URL for API endpoints"
            control={
              <Input
                id="apiBaseUrl"
                value={formData.general?.api_base_url || ''}
                onChange={(e) => updateGeneralSetting('api_base_url', e.target.value)}
                placeholder="http://localhost:8080"
              />
            }
          />
          <FieldBlock
            label="Theme"
            description="UI color theme preference (local only)"
            control={
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
            }
          />
        </div>
      </CardContent>
    </Card>
  );
}

function ServerSettingsCard({ formData, updateServerSetting }: ServerSettingsCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Server className="h-5 w-5" />
          Server Configuration
        </CardTitle>
        <CardDescription>Server ports and networking settings</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="grid grid-cols-2 gap-6">
          <FieldBlock
            id="httpPort"
            label="HTTP Port"
            description="Port for HTTP server"
            control={
              <Input
                id="httpPort"
                type="number"
                min={1024}
                max={65535}
                value={formData.server?.http_port || 8080}
                onChange={(e) => updateServerSetting('http_port', parseInt(e.target.value))}
              />
            }
          />
          <FieldBlock
            id="httpsPort"
            label="HTTPS Port"
            description="Port for HTTPS server (optional)"
            control={
              <Input
                id="httpsPort"
                type="number"
                min={1024}
                max={65535}
                value={formData.server?.https_port || ''}
                onChange={(e) =>
                  updateServerSetting('https_port', e.target.value ? parseInt(e.target.value) : null)
                }
                placeholder="Optional"
              />
            }
          />
          <FieldBlock
            id="udsSocket"
            label="UDS Socket Path"
            description="Unix domain socket path (required in production mode)"
            control={
              <Input
                id="udsSocket"
                value={formData.server?.uds_socket_path || ''}
                onChange={(e) => updateServerSetting('uds_socket_path', e.target.value || null)}
                placeholder="/var/run/adapteros.sock"
              />
            }
          />
        </div>
        <Separator />
        <ToggleRow
          title="Production Mode"
          description="Enable production security policies"
          icon={<Lock className="h-4 w-4" />}
          checked={formData.server?.production_mode || false}
          onChange={(checked) => updateServerSetting('production_mode', checked)}
        />
      </CardContent>
    </Card>
  );
}

function FingerprintRow({ securityInfo }: { securityInfo?: SecurityInfo }) {
  const fingerprint = securityInfo?.keyFingerprint || 'Loading...';

  return (
    <div className="space-y-2">
      <Label>Current Key Fingerprint</Label>
      <div className="flex items-center gap-2">
        <Input value={fingerprint} readOnly className="font-mono text-sm" />
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
      <p className="text-xs text-muted-foreground">Ed25519 public key fingerprint for JWT signing</p>
    </div>
  );
}

function JwtModeSelect({
  jwtConfig,
  setJwtConfig,
  setHasChanges,
  securityInfo,
}: Pick<JwtConfigurationCardProps, 'jwtConfig' | 'setJwtConfig' | 'setHasChanges' | 'securityInfo'>) {
  return (
    <FieldBlock
      id="jwtMode"
      label="JWT Signing Mode"
      description={securityInfo?.jwtMode ? `Current: ${securityInfo.jwtMode.toUpperCase()}` : undefined}
      control={
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
      }
    />
  );
}

function TokenTtlInput({
  jwtConfig,
  setJwtConfig,
  setHasChanges,
  securityInfo,
}: Pick<JwtConfigurationCardProps, 'jwtConfig' | 'setJwtConfig' | 'setHasChanges' | 'securityInfo'>) {
  return (
    <FieldBlock
      id="tokenTtl"
      label="Session Duration (minutes)"
      description={securityInfo ? `Current: ${securityInfo.tokenTtlMinutes} minutes` : undefined}
      control={
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
      }
    />
  );
}

function KeyMetadata({ securityInfo }: { securityInfo?: SecurityInfo }) {
  if (!securityInfo) return null;
  return (
    <div className="space-y-2 rounded-lg bg-slate-50 p-4 dark:bg-slate-900">
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">Created:</span>
        <span className="font-mono">
          {securityInfo.createdAt ? new Date(securityInfo.createdAt).toLocaleString() : 'N/A'}
        </span>
      </div>
      {securityInfo.lastRotated && (
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Last Rotated:</span>
          <span className="font-mono">{new Date(securityInfo.lastRotated).toLocaleString()}</span>
        </div>
      )}
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">Production Mode:</span>
        <Badge variant={securityInfo.productionMode ? 'default' : 'secondary'}>
          {securityInfo.productionMode ? 'Enabled' : 'Disabled'}
        </Badge>
      </div>
    </div>
  );
}

function SecurityActions({
  onUpdateJwtConfig,
  onRotateKeys,
  isUpdatingJwtConfig,
  isRotatingKeys,
  hasChanges,
}: Pick<
  JwtConfigurationCardProps,
  'onUpdateJwtConfig' | 'onRotateKeys' | 'isUpdatingJwtConfig' | 'isRotatingKeys' | 'hasChanges'
>) {
  return (
    <div className="flex items-center gap-4">
      <Button onClick={onUpdateJwtConfig} disabled={!hasChanges || isUpdatingJwtConfig}>
        {isUpdatingJwtConfig ? (
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
            onRotateKeys();
          }
        }}
        disabled={isRotatingKeys}
      >
        {isRotatingKeys ? (
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
  );
}

const RotationWarning = () => (
  <Alert>
    <Info className="h-4 w-4" />
    <AlertTitle>Key Rotation Warning</AlertTitle>
    <AlertDescription>
      Rotating keys will invalidate all existing JWT tokens. All users will need to re-authenticate.
    </AlertDescription>
  </Alert>
);

function JwtConfigurationCard({
  securityLoading,
  securityError,
  securityInfo,
  jwtConfig,
  setJwtConfig,
  onUpdateJwtConfig,
  onRotateKeys,
  isUpdatingJwtConfig,
  isRotatingKeys,
  hasChanges,
  setHasChanges,
}: JwtConfigurationCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Key className="h-5 w-5" />
          JWT Configuration
        </CardTitle>
        <CardDescription>JSON Web Token settings and key management</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {securityLoading && <LoadingState message="Loading security configuration..." />}
        {!securityLoading && !!securityError && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Error Loading Security Info</AlertTitle>
            <AlertDescription>
              {securityError instanceof Error ? securityError.message : String(securityError)}
            </AlertDescription>
          </Alert>
        )}
        {!securityLoading && !securityError && (
          <>
            <FingerprintRow securityInfo={securityInfo} />
            <div className="grid grid-cols-2 gap-6">
              <JwtModeSelect
                jwtConfig={jwtConfig}
                setJwtConfig={setJwtConfig}
                setHasChanges={setHasChanges}
                securityInfo={securityInfo}
              />
              <TokenTtlInput
                jwtConfig={jwtConfig}
                setJwtConfig={setJwtConfig}
                setHasChanges={setHasChanges}
                securityInfo={securityInfo}
              />
            </div>
            <KeyMetadata securityInfo={securityInfo} />
            <Separator />
            <SecurityActions
              onUpdateJwtConfig={onUpdateJwtConfig}
              onRotateKeys={onRotateKeys}
              isUpdatingJwtConfig={isUpdatingJwtConfig}
              isRotatingKeys={isRotatingKeys}
              hasChanges={hasChanges}
            />
            <RotationWarning />
          </>
        )}
      </CardContent>
    </Card>
  );
}

function AccessControlCard({ formData, updateSecuritySetting }: AccessControlCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Shield className="h-5 w-5" />
          Access Control
        </CardTitle>
        <CardDescription>Authentication and authorization policies</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="grid grid-cols-2 gap-6">
          <FieldBlock
            id="tokenTtl"
            label="Token TTL (seconds)"
            description="JWT token time-to-live (15 min - 24 hours)"
            control={
              <Input
                id="tokenTtl"
                type="number"
                min={900}
                max={86400}
                value={formData.security?.token_ttl_seconds || 28800}
                onChange={(e) => updateSecuritySetting('token_ttl_seconds', parseInt(e.target.value))}
              />
            }
          />
          <FieldBlock
            id="jwtMode"
            label="JWT Mode"
            description="JWT signing algorithm"
            control={
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
            }
          />
        </div>
        <Separator />
        <div className="space-y-4">
          <ToggleRow
            title="Require MFA"
            description="Enforce multi-factor authentication for all users"
            icon={<Lock className="h-4 w-4" />}
            checked={formData.security?.require_mfa || false}
            onChange={(checked) => updateSecuritySetting('require_mfa', checked)}
          />
          <ToggleRow
            title="Allow Outbound Connections"
            description="Allow outbound network connections"
            icon={<Server className="h-4 w-4" />}
            checked={formData.security?.egress_enabled || false}
            onChange={(checked) => updateSecuritySetting('egress_enabled', checked)}
          />
          <ToggleRow
            title="Require Firewall Rules"
            description="Require packet filter deny rules"
            checked={formData.security?.require_pf_deny || false}
            onChange={(checked) => updateSecuritySetting('require_pf_deny', checked)}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function PerformanceSettingsCard({ formData, updatePerformanceSetting }: PerformanceSettingsCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Zap className="h-5 w-5" />
          Performance Configuration
        </CardTitle>
        <CardDescription>Resource limits and performance tuning</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="grid grid-cols-2 gap-6">
          <FieldBlock
            id="maxAdapters"
            label="Max Adapters"
            description="Maximum number of concurrent adapters"
            control={
              <Input
                id="maxAdapters"
                type="number"
                min={1}
                max={100}
                value={formData.performance?.max_adapters || 8}
                onChange={(e) => updatePerformanceSetting('max_adapters', parseInt(e.target.value))}
              />
            }
          />
          <FieldBlock
            id="maxWorkers"
            label="Max Workers"
            description="Maximum number of worker threads"
            control={
              <Input
                id="maxWorkers"
                type="number"
                min={1}
                max={32}
                value={formData.performance?.max_workers || 4}
                onChange={(e) => updatePerformanceSetting('max_workers', parseInt(e.target.value))}
              />
            }
          />
          <FieldBlock
            id="memoryThreshold"
            label="Memory Threshold (%)"
            description="Memory usage threshold for eviction (50-95%)"
            control={
              <Input
                id="memoryThreshold"
                type="number"
                min={50}
                max={95}
                value={formData.performance?.memory_threshold_pct || 85}
                onChange={(e) =>
                  updatePerformanceSetting('memory_threshold_pct', parseInt(e.target.value))
                }
              />
            }
          />
          <FieldBlock
            id="cacheSize"
            label="Cache Size (MB)"
            description="Cache size for adapter weights (128-8192 MB)"
            control={
              <Input
                id="cacheSize"
                type="number"
                min={128}
                max={8192}
                value={formData.performance?.cache_size_mb || 1024}
                onChange={(e) => updatePerformanceSetting('cache_size_mb', parseInt(e.target.value))}
              />
            }
          />
        </div>
        <Alert>
          <Info className="h-4 w-4" />
          <AlertTitle>Performance Impact</AlertTitle>
          <AlertDescription>
            Changing these settings may affect system performance and memory usage. Adjust carefully based
            on available resources.
          </AlertDescription>
        </Alert>
      </CardContent>
    </Card>
  );
}

function SettingsTabs({
  activeSection,
  onSectionChange,
  formData,
  theme,
  setTheme,
  updateGeneralSetting,
  updateServerSetting,
  updateSecuritySetting,
  updatePerformanceSetting,
  securityLoading,
  securityError,
  securityInfo,
  jwtConfig,
  setJwtConfig,
  onUpdateJwtConfig,
  onRotateKeys,
  isUpdatingJwtConfig,
  isRotatingKeys,
  hasChanges,
  setHasChanges,
}: SettingsTabsProps) {
  return (
    <Tabs value={activeSection} onValueChange={onSectionChange} className="space-y-6">
      <TabsList className="grid grid-cols-4 w-full max-w-2xl">
        {settingsSections.map((section) => (
          <TabsTrigger key={section.id} value={section.id} className="gap-2">
            {section.icon}
            {section.title}
          </TabsTrigger>
        ))}
      </TabsList>

      <TabsContent value="general" className="space-y-6">
        <GeneralSettingsCard
          formData={formData}
          updateGeneralSetting={updateGeneralSetting}
          theme={theme}
          setTheme={setTheme}
        />
      </TabsContent>

      <TabsContent value="server" className="space-y-6">
        <ServerSettingsCard formData={formData} updateServerSetting={updateServerSetting} />
      </TabsContent>

      <TabsContent value="security" className="space-y-6">
        <JwtConfigurationCard
          securityLoading={securityLoading}
          securityError={securityError}
          securityInfo={securityInfo}
          jwtConfig={jwtConfig}
          setJwtConfig={setJwtConfig}
          onUpdateJwtConfig={onUpdateJwtConfig}
          onRotateKeys={onRotateKeys}
          isUpdatingJwtConfig={isUpdatingJwtConfig}
          isRotatingKeys={isRotatingKeys}
          hasChanges={hasChanges}
          setHasChanges={setHasChanges}
        />
        <AccessControlCard formData={formData} updateSecuritySetting={updateSecuritySetting} />
      </TabsContent>

      <TabsContent value="performance" className="space-y-6">
        <PerformanceSettingsCard
          formData={formData}
          updatePerformanceSetting={updatePerformanceSetting}
        />
      </TabsContent>
    </Tabs>
  );
}

function SettingsContent(props: SettingsContentProps) {
  const { restartRequired, hasChanges, ...tabsProps } = props;
  return (
    <>
      {restartRequired && <RestartAlert />}
      {hasChanges && <UnsavedChangesAlert />}
      <SettingsTabs hasChanges={hasChanges} {...tabsProps} />
    </>
  );
}

export function SettingsPage() {
  const queryClient = useQueryClient();
  const { userRole } = useRBAC();
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
    return <PermissionDeniedView />;
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
    return <LoadingSettingsView />;
  }

  // Show error state
  if (error) {
    const message = error instanceof Error ? error.message : 'Failed to load settings';
    return <ErrorSettingsView message={message} />;
  }

  return (
    <DensityProvider pageKey="settings">
      <FeatureLayout
        title="System Settings"
        description="Configure system-wide settings and preferences"
        maxWidth="xl"
        contentPadding="default"
        primaryAction={{
          label: hasChanges ? (updateSettings.isPending ? 'Saving...' : 'Save Changes') : 'Save Changes',
          onClick: handleSave,
          disabled: !hasChanges || updateSettings.isPending,
          icon: updateSettings.isPending ? Loader2 : Save,
        }}
        secondaryActions={[
          {
            label: 'Export',
            onClick: handleExportSettings,
            variant: 'outline',
            icon: Download,
          },
        ]}
      >
        <SettingsContent
          activeSection={activeSection}
          onSectionChange={setActiveSection}
          formData={formData}
          theme={theme}
          setTheme={setTheme}
          updateGeneralSetting={updateGeneralSetting}
          updateServerSetting={updateServerSetting}
          updateSecuritySetting={updateSecuritySetting}
          updatePerformanceSetting={updatePerformanceSetting}
          securityLoading={securityLoading}
          securityError={securityError}
          securityInfo={securityInfo}
          jwtConfig={jwtConfig}
          setJwtConfig={setJwtConfig}
          onUpdateJwtConfig={() => updateJwtConfigMutation.mutate(jwtConfig)}
          onRotateKeys={() => rotateKeysMutation.mutate()}
          isUpdatingJwtConfig={updateJwtConfigMutation.isPending}
          isRotatingKeys={rotateKeysMutation.isPending}
          hasChanges={hasChanges}
          setHasChanges={setHasChanges}
          restartRequired={restartRequired}
        />
      </FeatureLayout>
    </DensityProvider>
  );
}

export default SettingsPage;
