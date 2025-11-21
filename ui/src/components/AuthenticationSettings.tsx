import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Switch } from './ui/switch';
import { Label } from './ui/label';
import { Separator } from './ui/separator';
import {
  Shield,
  Settings,
  Key,
  RefreshCw,
  LogOut,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Save,
  RotateCcw,
  Clock,
  Users,
  Lock,
  Unlock
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { logger } from '../utils/logger';

interface AuthConfig {
  production_mode: boolean;
  dev_token_enabled: boolean;
  jwt_mode: string;
  token_expiry_hours: number;
}

interface SessionInfo {
  id: string;
  device?: string;
  ip_address?: string;
  user_agent?: string;
  location?: string;
  created_at: string;
  last_seen_at: string;
  is_current: boolean;
}

export function AuthenticationSettings() {
  const [config, setConfig] = useState<AuthConfig | null>(null);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);

  // Load authentication configuration and sessions
  const loadData = async () => {
    setIsRefreshing(true);
    try {
      const [configResponse, sessionsResponse] = await Promise.all([
        apiClient.getAuthConfig(),
        apiClient.listSessions()
      ]);
      setConfig({
        production_mode: configResponse.production_mode,
        dev_token_enabled: configResponse.dev_token_enabled,
        jwt_mode: configResponse.jwt_mode,
        token_expiry_hours: configResponse.token_expiry_hours,
      });
      setSessions(sessionsResponse);
    } catch (error) {
      logger.error('Failed to load auth data', { component: 'AuthenticationSettings' }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to load authentication settings');
    } finally {
      setIsRefreshing(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // Update authentication configuration
  const updateConfig = async (updates: Partial<AuthConfig>) => {
    if (!config) return;

    setIsLoading(true);
    try {
      const newConfig = { ...config, ...updates };
      await apiClient.updateAuthConfig(newConfig);
      setConfig(newConfig);
      toast.success('Authentication settings updated');
    } catch (error) {
      logger.error('Failed to update auth config', { component: 'AuthenticationSettings' }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to update authentication settings');
    } finally {
      setIsLoading(false);
    }
  };

  // Rotate token
  const rotateToken = async () => {
    setIsLoading(true);
    try {
      await apiClient.rotateApiToken();
      toast.success('API token rotated successfully');
      await loadData(); // Refresh data
    } catch (error) {
      logger.error('Failed to rotate API token', { component: 'AuthenticationSettings' }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to rotate token');
    } finally {
      setIsLoading(false);
    }
  };

  // Logout all sessions
  const logoutAllSessions = async () => {
    if (!confirm('Are you sure you want to logout from all sessions? This will require re-authentication on all devices.')) {
      return;
    }

    setIsLoading(true);
    try {
      await apiClient.logoutAllSessions();
      toast.success('Logged out from all sessions');
      // Note: This will likely cause the current session to be invalidated
      // The UI should handle this by redirecting to login
    } catch (error) {
      logger.error('Failed to logout all sessions', { component: 'AuthenticationSettings' }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to logout all sessions');
    } finally {
      setIsLoading(false);
    }
  };

  // Revoke specific session
  const revokeSession = async (sessionId: string) => {
    if (sessionId === sessions.find(s => s.is_current)?.id) {
      if (!confirm('Are you sure you want to revoke your current session? This will log you out.')) {
        return;
      }
    }

    setIsLoading(true);
    try {
      await apiClient.revokeSession(sessionId);
      toast.success('Session revoked');
      await loadData(); // Refresh data
    } catch (error) {
      logger.error('Failed to revoke session', { component: 'AuthenticationSettings', sessionId }, error instanceof Error ? error : new Error(String(error)));
      toast.error('Failed to revoke session');
    } finally {
      setIsLoading(false);
    }
  };

  if (!config) {
    return (
      <div className="flex items-center justify-center p-8">
        <RefreshCw className="w-6 h-6 animate-spin" />
        <span className="ml-2">Loading authentication settings...</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-2">
        <Shield className="w-6 h-6 text-blue-500" />
        <h2 className="text-2xl font-semibold">Authentication Settings</h2>
        <Button
          onClick={loadData}
          variant="outline"
          size="sm"
          disabled={isRefreshing}
        >
          <RefreshCw className={`w-4 h-4 mr-2 ${isRefreshing ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      {/* Production Mode Settings */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Settings className="w-5 h-5" />
            Security Configuration
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* Production Mode Toggle */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Label htmlFor="production-mode" className="text-sm font-medium">
                  Production Mode
                </Label>
                {config.production_mode ? (
                  <Lock className="w-4 h-4 text-green-500" />
                ) : (
                  <Unlock className="w-4 h-4 text-yellow-500" />
                )}
              </div>
              <p className="text-sm text-gray-600">
                Enables strict security policies, disables dev features
              </p>
            </div>
            <Switch
              id="production-mode"
              checked={config.production_mode}
              onCheckedChange={(checked) => updateConfig({ production_mode: checked })}
              disabled={isLoading}
            />
          </div>

          <Separator />

          {/* Dev Token Toggle */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Label htmlFor="dev-token" className="text-sm font-medium">
                  Development Token
                </Label>
                {config.dev_token_enabled ? (
                  <CheckCircle className="w-4 h-4 text-green-500" />
                ) : (
                  <XCircle className="w-4 h-4 text-gray-500" />
                )}
              </div>
              <p className="text-sm text-gray-600">
                Allows bypass authentication in development
                {!config.production_mode ? ' (available)' : ' (disabled in production)'}
              </p>
            </div>
            <Switch
              id="dev-token"
              checked={config.dev_token_enabled}
              onCheckedChange={(checked) => updateConfig({ dev_token_enabled: checked })}
              disabled={isLoading || config.production_mode}
            />
          </div>

          <Separator />

          {/* JWT Mode Display */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <Label className="text-sm font-medium">JWT Mode</Label>
              <p className="text-sm text-gray-600">
                Token signing algorithm (requires restart to change)
              </p>
            </div>
            <Badge variant="secondary">{config.jwt_mode.toUpperCase()}</Badge>
          </div>

          {/* Token Expiry Display */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <Label className="text-sm font-medium">Token Expiry</Label>
              <p className="text-sm text-gray-600">
                Hours until tokens expire
              </p>
            </div>
            <Badge variant="secondary">{config.token_expiry_hours}h</Badge>
          </div>
        </CardContent>
      </Card>

      {/* Session Management */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Users className="w-5 h-5" />
            Session Management
            <Badge variant="secondary">{sessions.length} session{sessions.length !== 1 ? 's' : ''}</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Token Actions */}
          <div className="flex gap-2">
            <Button
              onClick={rotateToken}
              variant="outline"
              size="sm"
              disabled={isLoading}
            >
              <RotateCcw className="w-4 h-4 mr-2" />
              Rotate Token
            </Button>
            <Button
              onClick={logoutAllSessions}
              variant="destructive"
              size="sm"
              disabled={isLoading}
            >
              <LogOut className="w-4 h-4 mr-2" />
              Logout All Sessions
            </Button>
          </div>

          <Separator />

          {/* Sessions List */}
          <div className="space-y-3">
            <Label className="text-sm font-medium">Active Sessions</Label>
            {sessions.map((session) => (
              <div
                key={session.id}
                className="flex items-center justify-between p-3 border rounded-lg"
              >
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium">
                      {session.is_current ? 'Current Session' : 'Other Session'}
                    </span>
                    {session.is_current && (
                      <Badge variant="default" className="text-xs">Active</Badge>
                    )}
                  </div>
                  <div className="text-xs text-gray-600 space-y-1">
                    <div>Created: {new Date(session.created_at).toLocaleString()}</div>
                    <div>Last seen: {new Date(session.last_seen_at).toLocaleString()}</div>
                    {session.device && <div>Device: {session.device}</div>}
                    {session.ip_address && <div>IP: {session.ip_address}</div>}
                  </div>
                </div>
                {!session.is_current && (
                  <Button
                    onClick={() => revokeSession(session.id)}
                    variant="outline"
                    size="sm"
                    disabled={isLoading}
                  >
                    <XCircle className="w-4 h-4" />
                  </Button>
                )}
              </div>
            ))}
            {sessions.length === 0 && (
              <div className="text-center py-8 text-gray-500">
                <Users className="w-12 h-12 mx-auto mb-4 opacity-50" />
                <p>No active sessions found</p>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Security Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Shield className="w-5 h-5" />
            Security Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="text-center">
              <div className={`inline-flex items-center justify-center w-12 h-12 rounded-full mb-2 ${
                config.production_mode ? 'bg-green-100' : 'bg-yellow-100'
              }`}>
                {config.production_mode ? (
                  <Lock className="w-6 h-6 text-green-600" />
                ) : (
                  <Unlock className="w-6 h-6 text-yellow-600" />
                )}
              </div>
              <p className="text-sm font-medium">Production Mode</p>
              <p className="text-xs text-gray-600">
                {config.production_mode ? 'Enabled' : 'Disabled'}
              </p>
            </div>

            <div className="text-center">
              <div className={`inline-flex items-center justify-center w-12 h-12 rounded-full mb-2 ${
                config.dev_token_enabled ? 'bg-yellow-100' : 'bg-gray-100'
              }`}>
                <Key className={`w-6 h-6 ${
                  config.dev_token_enabled ? 'text-yellow-600' : 'text-gray-600'
                }`} />
              </div>
              <p className="text-sm font-medium">Dev Token</p>
              <p className="text-xs text-gray-600">
                {config.dev_token_enabled ? 'Enabled' : 'Disabled'}
              </p>
            </div>

            <div className="text-center">
              <div className="inline-flex items-center justify-center w-12 h-12 rounded-full mb-2 bg-blue-100">
                <Clock className="w-6 h-6 text-blue-600" />
              </div>
              <p className="text-sm font-medium">Token Expiry</p>
              <p className="text-xs text-gray-600">{config.token_expiry_hours}h</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
