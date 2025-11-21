import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { useAuth } from '@/providers/CoreProviders';
import apiClient from '@/api/client';
import type { SessionInfo, TokenMetadata } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ConfirmationDialog } from '@/components/ui/confirmation-dialog';
import {
  AlertCircle,
  Copy,
  KeyRound,
  LogOut,
  RefreshCw,
  ShieldCheck,
  User as UserIcon,
  XCircle,
} from 'lucide-react';

interface SessionState {
  data: SessionInfo[];
  loading: boolean;
  error: string | null;
}

function formatDate(value?: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function formatRelative(value?: string | null): string {
  if (!value) return '';
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return '';
  const diffMs = Date.now() - timestamp;
  const absDiff = Math.abs(diffMs);
  const minutes = Math.round(absDiff / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${diffMs < 0 ? 'in ' : ''}${minutes}m${diffMs < 0 ? '' : ' ago'}`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${diffMs < 0 ? 'in ' : ''}${hours}h${diffMs < 0 ? '' : ' ago'}`;
  const days = Math.round(hours / 24);
  return `${diffMs < 0 ? 'in ' : ''}${days}d${diffMs < 0 ? '' : ' ago'}`;
}

function summarizeAgent(agent?: string): string {
  if (!agent) return 'Unknown device';
  if (agent.includes('iPhone')) return 'iPhone';
  if (agent.includes('Android')) return 'Android';
  if (agent.includes('Mac')) return 'MacOS';
  if (agent.includes('Windows')) return 'Windows';
  if (agent.includes('Linux')) return 'Linux';
  return agent.split(' ').slice(0, 3).join(' ');
}

export default function ProfilePage() {
  const {
    user,
    refreshSession,
    refreshUser,
    logoutAllSessions,
    updateProfile,
  } = useAuth();

  const { can, userRole } = useRBAC();

  const [sessionsState, setSessionsState] = useState<SessionState>({
    data: [],
    loading: false,
    error: null,
  });
  const [tokenMetadata, setTokenMetadata] = useState<TokenMetadata | null>(null);
  const [tokenError, setTokenError] = useState<string | null>(null);
  const [isRotatingToken, setIsRotatingToken] = useState(false);
  const [rotatedToken, setRotatedToken] = useState<string | null>(null);
  const [tokenDialogOpen, setTokenDialogOpen] = useState(false);
  const [displayNameDialogOpen, setDisplayNameDialogOpen] = useState(false);
  const [displayNameInput, setDisplayNameInput] = useState(user?.display_name ?? '');
  const [displayNameError, setDisplayNameError] = useState<string | null>(null);
  const [logoutAllDialogOpen, setLogoutAllDialogOpen] = useState(false);
  const [sessionToRevoke, setSessionToRevoke] = useState<SessionInfo | null>(null);
  const [revokeDialogOpen, setRevokeDialogOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState(false);

  useEffect(() => {
    setDisplayNameInput(user?.display_name ?? '');
  }, [user?.display_name]);

  const loadSessions = useCallback(async () => {
    setSessionsState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const sessions = await apiClient.listSessions();
      setSessionsState({ data: sessions, loading: false, error: null });
      logger.info('Loaded auth sessions', {
        component: 'ProfilePage',
        operation: 'listSessions',
        count: sessions.length,
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load sessions';
      setSessionsState(prev => ({ ...prev, loading: false, error: message }));
      logger.error('Failed to load auth sessions', {
        component: 'ProfilePage',
        operation: 'listSessions',
      }, toError(err));
    }
  }, []);

  const loadTokenMetadata = useCallback(async () => {
    setTokenError(null);
    try {
      const metadata = await apiClient.getTokenMetadata();
      setTokenMetadata(metadata);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load token metadata';
      setTokenError(message);
      logger.error('Failed to load token metadata', {
        component: 'ProfilePage',
        operation: 'tokenMetadata',
      }, toError(err));
    }
  }, []);

  useEffect(() => {
    if (!user) return;
    void loadSessions();
    void loadTokenMetadata();
  }, [user, loadSessions, loadTokenMetadata]);

  const handleRotateToken = useCallback(async () => {
    setIsRotatingToken(true);
    setRotatedToken(null);
    try {
      const response = await apiClient.rotateApiToken();
      setRotatedToken(response.token);
      setTokenMetadata({
        created_at: response.created_at,
        expires_at: response.expires_at,
        last_rotated_at: response.last_rotated_at ?? response.created_at,
      });
      setTokenDialogOpen(true);
      toast.success('Issued new API token');
      logger.info('API token rotated', {
        component: 'ProfilePage',
        operation: 'rotateToken',
      });
      await refreshUser().catch(() => {});
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to rotate token');
      logger.error('Failed to rotate API token', {
        component: 'ProfilePage',
        operation: 'rotateToken',
      }, toError(err));
    } finally {
      setIsRotatingToken(false);
    }
  }, [refreshUser]);

  const handleCopyToken = useCallback(async () => {
    if (!rotatedToken) return;
    try {
      await navigator.clipboard.writeText(rotatedToken);
      toast.success('Token copied to clipboard');
    } catch {
      toast.error('Failed to copy token');
    }
  }, [rotatedToken]);

  const handleLogoutAllSessions = useCallback(async () => {
    setPendingAction(true);
    try {
      await logoutAllSessions();
      toast.success('Ended all other sessions');
      logger.info('Ended all sessions', {
        component: 'ProfilePage',
        operation: 'logoutAll',
      });
      await loadSessions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to end sessions');
      logger.error('Failed to end all sessions', {
        component: 'ProfilePage',
        operation: 'logoutAll',
      }, toError(err));
    } finally {
      setPendingAction(false);
      setLogoutAllDialogOpen(false);
    }
  }, [logoutAllSessions, loadSessions]);

  const handleRevokeSession = useCallback(async () => {
    if (!sessionToRevoke) return;
    setPendingAction(true);
    try {
      await apiClient.revokeSession(sessionToRevoke.id);
      toast.success('Session terminated');
      logger.info('Terminated session', {
        component: 'ProfilePage',
        operation: 'revokeSession',
        sessionId: sessionToRevoke.id,
      });
      await loadSessions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to terminate session');
      logger.error('Failed to terminate session', {
        component: 'ProfilePage',
        operation: 'revokeSession',
        sessionId: sessionToRevoke.id,
      }, toError(err));
    } finally {
      setPendingAction(false);
      setRevokeDialogOpen(false);
      setSessionToRevoke(null);
    }
  }, [sessionToRevoke, loadSessions]);

  const handleRefreshSession = useCallback(async () => {
    setPendingAction(true);
    try {
      await refreshSession();
      toast.success('Session refreshed');
      logger.info('Session refreshed', {
        component: 'ProfilePage',
        operation: 'refreshSession',
      });
      await loadSessions();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to refresh session');
      logger.error('Failed to refresh session', {
        component: 'ProfilePage',
        operation: 'refreshSession',
      }, toError(err));
    } finally {
      setPendingAction(false);
    }
  }, [refreshSession, loadSessions]);

  const handleDisplayNameSave = useCallback(async () => {
    const trimmed = displayNameInput.trim();
    if (trimmed.length < 2) {
      setDisplayNameError('Display name must be at least 2 characters.');
      return;
    }
    setDisplayNameError(null);
    setPendingAction(true);
    try {
      await updateProfile({ display_name: trimmed });
      toast.success('Profile updated');
      logger.info('Profile updated', {
        component: 'ProfilePage',
        operation: 'updateProfile',
      });
      setDisplayNameDialogOpen(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update profile';
      setDisplayNameError(message);
      logger.error('Failed to update profile', {
        component: 'ProfilePage',
        operation: 'updateProfile',
      }, toError(err));
    } finally {
      setPendingAction(false);
    }
  }, [displayNameInput, updateProfile]);

  const currentSessionId = useMemo(() => {
    return sessionsState.data.find(session => session.is_current)?.id ?? null;
  }, [sessionsState.data]);

  if (!user) {
    return null;
    // RequireAuth in RouteGuard already prevents this state, but guard defensively.
  }

  return (
    <DensityProvider pageKey="profile">
      <FeatureLayout
        title="Profile"
        description="Manage your account and sessions"
      >
        <div className="space-y-6">
          <PageHeader
            title="Profile"
            description="Manage your account and sessions"
            helpContent="Manage personal details, active sessions, and access tokens"
          />
          <Card>
            <CardHeader className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <UserIcon className="h-5 w-5 text-muted-foreground" />
                  Account Overview
                </CardTitle>
                <CardDescription>
                  Manage personal details, active sessions, and access tokens without leaving the control plane.
                </CardDescription>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">{user.role}</Badge>
                <Badge variant={user.mfa_enabled ? 'default' : 'outline'}>
                  {user.mfa_enabled ? 'MFA enabled' : 'MFA disabled'}
                </Badge>
              </div>
            </CardHeader>
            <CardContent className="grid gap-6 md:grid-cols-2">
              <div className="space-y-3">
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Display name</h3>
                  <p className="text-base font-medium">{user.display_name}</p>
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Email</h3>
                  <p className="text-base font-medium">{user.email}</p>
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Last login</h3>
                  <p className="text-base font-medium">
                    {formatDate(user.last_login_at)}
                    {user.last_login_at && (
                      <span className="ml-2 text-xs text-muted-foreground">
                        ({formatRelative(user.last_login_at)})
                      </span>
                    )}
                  </p>
                </div>
              </div>
              <div className="flex flex-col gap-3">
                <Button variant="outline" onClick={() => setDisplayNameDialogOpen(true)}>
                  Edit profile
                </Button>
                <Button
                  variant="secondary"
                  onClick={handleRefreshSession}
                  disabled={pendingAction}
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Refresh session
                </Button>
                <Button
                  variant="outline"
                  onClick={() => setLogoutAllDialogOpen(true)}
                  disabled={pendingAction}
                >
                  <LogOut className="mr-2 h-4 w-4" />
                  End all other sessions
                </Button>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <ShieldCheck className="h-5 w-5 text-muted-foreground" />
                  Active Sessions
                </CardTitle>
                <CardDescription>
                  Monitor sessions signed into AdapterOS. End remote sessions to enforce security posture.
                </CardDescription>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void loadSessions()}
                disabled={sessionsState.loading}
              >
                <RefreshCw className={`mr-2 h-4 w-4 ${sessionsState.loading ? 'animate-spin' : ''}`} />
                Refresh
              </Button>
            </CardHeader>
            <CardContent>
              {sessionsState.error && (
                <Alert variant="destructive" className="mb-4">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>{sessionsState.error}</AlertDescription>
                </Alert>
              )}

              <div className="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Device</TableHead>
                      <TableHead>IP Address</TableHead>
                      <TableHead>Location</TableHead>
                      <TableHead>Last Seen</TableHead>
                      <TableHead>Created</TableHead>
                      <TableHead className="text-right">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {sessionsState.loading && (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-sm text-muted-foreground">
                          Loading sessions...
                        </TableCell>
                      </TableRow>
                    )}

                    {!sessionsState.loading && sessionsState.data.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-sm text-muted-foreground">
                          No active sessions found.
                        </TableCell>
                      </TableRow>
                    )}

                    {sessionsState.data.map(session => {
                      const isCurrent = session.is_current || session.id === currentSessionId;
                      return (
                        <TableRow key={session.id}>
                          <TableCell className="font-medium">
                            {summarizeAgent(session.device || session.user_agent)}
                            <div className="text-xs text-muted-foreground">
                              {session.user_agent?.split(')')[0] ?? '—'}
                            </div>
                          </TableCell>
                          <TableCell className="text-sm">{session.ip_address ?? '—'}</TableCell>
                          <TableCell className="text-sm">{session.location ?? '—'}</TableCell>
                          <TableCell className="text-sm">
                            {formatDate(session.last_seen_at)}
                            <div className="text-xs text-muted-foreground">
                              {formatRelative(session.last_seen_at)}
                            </div>
                          </TableCell>
                          <TableCell className="text-sm">{formatDate(session.created_at)}</TableCell>
                          <TableCell className="text-right">
                            {isCurrent ? (
                              <Badge variant="outline">Current session</Badge>
                            ) : (
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => {
                                  setSessionToRevoke(session);
                                  setRevokeDialogOpen(true);
                                }}
                                disabled={pendingAction}
                              >
                                <XCircle className="mr-2 h-4 w-4 text-destructive" />
                                End session
                              </Button>
                            )}
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <KeyRound className="h-5 w-5 text-muted-foreground" />
                  API Token
                </CardTitle>
                <CardDescription>
                  Regenerate and distribute API tokens securely. Tokens are shown once and rotate existing sessions.
                </CardDescription>
              </div>
              <Button onClick={handleRotateToken} disabled={isRotatingToken}>
                <RefreshCw className={`mr-2 h-4 w-4 ${isRotatingToken ? 'animate-spin' : ''}`} />
                Rotate token
              </Button>
            </CardHeader>
            <CardContent className="space-y-4">
              {tokenError && (
                <Alert variant="destructive">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>{tokenError}</AlertDescription>
                </Alert>
              )}

              <div className="grid gap-4 md:grid-cols-3">
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Issued</h3>
                  <p className="text-base font-medium">{formatDate(tokenMetadata?.created_at)}</p>
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Last rotated</h3>
                  <p className="text-base font-medium">
                    {formatDate(tokenMetadata?.last_rotated_at)}
                    {tokenMetadata?.last_rotated_at && (
                      <span className="ml-2 text-xs text-muted-foreground">
                        ({formatRelative(tokenMetadata.last_rotated_at)})
                      </span>
                    )}
                  </p>
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground">Expires</h3>
                  <p className="text-base font-medium">{formatDate(tokenMetadata?.expires_at)}</p>
                </div>
              </div>
            </CardContent>
            <CardFooter className="text-xs text-muted-foreground">
              Rotating the token invalidates previously issued API credentials. Copy and distribute the new token immediately.
            </CardFooter>
          </Card>

          <Dialog open={displayNameDialogOpen} onOpenChange={setDisplayNameDialogOpen}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Update profile</DialogTitle>
                <DialogDescription>
                  Change how your name appears across AdapterOS. This is visible to operators in audit logs.
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="display-name">Display name</Label>
                  <Input
                    id="display-name"
                    value={displayNameInput}
                    onChange={(event) => setDisplayNameInput(event.target.value)}
                    placeholder="Jane Doe"
                  />
                  {displayNameError && (
                    <p className="text-sm text-destructive">{displayNameError}</p>
                  )}
                </div>
              </div>
              <DialogFooter>
                <Button variant="ghost" onClick={() => setDisplayNameDialogOpen(false)}>Cancel</Button>
                <Button onClick={handleDisplayNameSave} disabled={pendingAction}>
                  Save changes
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>

          <Dialog open={tokenDialogOpen} onOpenChange={setTokenDialogOpen}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>New API token</DialogTitle>
                <DialogDescription>
                  Copy the token now. It will not be shown again once you close this dialog.
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-4">
                <div className="rounded-md border bg-muted/50 p-4">
                  <code className="block break-all text-sm">
                    {rotatedToken ?? '—'}
                  </code>
                </div>
                <Button variant="outline" onClick={handleCopyToken} disabled={!rotatedToken}>
                  <Copy className="mr-2 h-4 w-4" />
                  Copy token
                </Button>
              </div>
              <DialogFooter>
                <Button onClick={() => setTokenDialogOpen(false)}>Done</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>

          <ConfirmationDialog
            open={logoutAllDialogOpen}
            onOpenChange={setLogoutAllDialogOpen}
            onConfirm={handleLogoutAllSessions}
            options={{
              title: 'End all other sessions',
              description: 'Sign out of every session except this one. Active operators will need to sign in again.',
              confirmText: 'End sessions',
              variant: 'destructive',
            }}
          />

          <ConfirmationDialog
            open={revokeDialogOpen}
            onOpenChange={(open) => {
              setRevokeDialogOpen(open);
              if (!open) {
                setSessionToRevoke(null);
              }
            }}
            onConfirm={handleRevokeSession}
            options={{
              title: 'Terminate session',
              description: sessionToRevoke
                ? `Terminate the session from ${summarizeAgent(sessionToRevoke.device || sessionToRevoke.user_agent)}?`
                : 'Terminate selected session?',
              confirmText: 'Terminate',
              variant: 'destructive',
            }}
          />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
