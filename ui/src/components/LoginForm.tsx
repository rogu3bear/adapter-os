import { useState, useEffect, useCallback, useMemo } from 'react';
import { useForm, type Resolver } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import {
  Lock,
  Shield,
  AlertTriangle,
  XCircle,
  Zap,
  Server,
  CheckCircle2,
  Loader2,
  RefreshCw,
  Cpu,
  Boxes,
  ShieldCheck,
  FileCheck,
  Fingerprint,
  Trash2,
  ChevronDown,
} from 'lucide-react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
import { apiClient } from '@/api/client';
import { LoginFormSchema, type LoginFormData } from '@/schemas/common.schema';
import { cn } from '@/components/ui/utils';
import { logger } from '@/utils/logger';
import type { HealthResponse, SystemHealthResponse, MetaResponse, BaseModelStatus } from '@/api/api-types';
import type { AuthConfigResponse } from '@/api/auth-types';
import { formatDurationSeconds } from '@/utils/format';

// Constants for health check configuration
const HEALTH_POLL_INTERVAL_MS = 2000;
const MAX_RETRY_ATTEMPTS = 3;
const DEBOUNCE_MS = 300;

// Dev mode constants
const DEV_ROLES = ['admin', 'operator', 'sre', 'viewer'] as const;
const LAST_ROLE_KEY = 'aos-last-dev-role';
const LOCALSTORAGE_KEYS_TO_CLEAR = [
  'theme',
  'selectedTenant',
  'aos_sidebar_collapsed_groups',
  'aos-first-login-completed',
];

// Wrapped resolver that catches validation errors during initial render
// This prevents console errors when form mounts with empty values
const safeZodResolver: Resolver<LoginFormData> = async (values, context, options) => {
  try {
    return await zodResolver(LoginFormSchema)(values, context, options);
  } catch (err) {
    // Log validation errors for debugging but don't block render
    logger.warn('Form validation error during initial render', {
      component: 'LoginForm',
      operation: 'validation',
    });
    return { values: {} as LoginFormData, errors: {} };
  }
};

interface LoginFormProps {
  onLogin: (credentials: { email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}

type BackendState = 'checking' | 'loading' | 'ready' | 'error';


export function LoginForm({ onLogin, onDevBypass, error }: LoginFormProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isDevBypassLoading, setIsDevBypassLoading] = useState(false);
  const [backendState, setBackendState] = useState<BackendState>('checking');
  const [backendHealth, setBackendHealth] = useState<HealthResponse | null>(null);
  const [systemHealth, setSystemHealth] = useState<SystemHealthResponse | null>(null);
  const [metaInfo, setMetaInfo] = useState<MetaResponse | null>(null);
  const [modelStatus, setModelStatus] = useState<BaseModelStatus | null>(null);
  const [checkAttempts, setCheckAttempts] = useState(0);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const [devBypassAllowed, setDevBypassAllowed] = useState(false);
  const [loadingRole, setLoadingRole] = useState<string | null>(null);
  const [lastRole, setLastRole] = useState<string | null>(() => {
    try {
      return localStorage.getItem(LAST_ROLE_KEY);
    } catch (e) {
      if (import.meta.env.DEV) {
        logger.warn('[LoginForm] localStorage read failed', {
          component: 'LoginForm',
          operation: 'localStorageRead',
          error: e instanceof Error ? e.message : String(e),
        });
      }
      return null;
    }
  });
  const [debugPanelOpen, setDebugPanelOpen] = useState(false);
  const [lastClickTime, setLastClickTime] = useState(0);
  const [clearStorageMessage, setClearStorageMessage] = useState<string | null>(null);

  // Comprehensive backend health check
  const checkBackendHealth = useCallback(async () => {
    try {
      // Primary health check
      const health = await apiClient.get<HealthResponse>('/healthz');
      setBackendHealth(health);

      if (health.status === 'healthy' || health.status === 'degraded') {
        setBackendState('ready');
        setLoadingProgress(100);

        // Fetch additional data in parallel once backend is ready
        const [systemHealthRes, metaRes, modelStatusRes] = await Promise.allSettled([
          apiClient.getHealthzAll(),
          apiClient.getMeta(),
          apiClient.getBaseModelStatus(),
        ]);

        if (systemHealthRes.status === 'fulfilled') {
          setSystemHealth(systemHealthRes.value);
        }
        if (metaRes.status === 'fulfilled') {
          setMetaInfo(metaRes.value);
        }
        // Handle model status gracefully - 401 is expected before login
        if (modelStatusRes.status === 'fulfilled') {
          setModelStatus(modelStatusRes.value);
        } else if (modelStatusRes.status === 'rejected') {
          // Don't log 401 errors - they're expected when not authenticated
          // Set modelStatus to null to indicate we need authentication
          setModelStatus(null);
        }
      } else {
        setBackendState('loading');
        setLoadingProgress(50);
      }
    } catch (err) {
      logger.warn('Backend health check failed', {
        component: 'LoginForm',
        operation: 'healthCheck',
      });
      setCheckAttempts(prev => {
        const attempts = prev + 1;
        if (attempts >= MAX_RETRY_ATTEMPTS) {
          setBackendState('error');
          setLoadingProgress(0);
        } else {
          setBackendState('loading');
          setLoadingProgress(Math.min(attempts * 25, 75));
        }
        return attempts;
      });
    }
  }, []);

  // Initial backend health check
  useEffect(() => {
    checkBackendHealth();
  }, [checkBackendHealth]);

  // Poll backend health while loading
  useEffect(() => {
    if (backendState === 'loading' || backendState === 'checking') {
      const interval = setInterval(checkBackendHealth, HEALTH_POLL_INTERVAL_MS);
      return () => clearInterval(interval);
    }
  }, [backendState, checkBackendHealth]);

  // Fetch auth config when backend is ready
  useEffect(() => {
    if (backendState === 'ready') {
      apiClient.getAuthConfig()
        .then((config: AuthConfigResponse) => {
          setDevBypassAllowed(config.dev_bypass_allowed ?? false);
        })
        .catch(() => setDevBypassAllowed(false));
    }
  }, [backendState]);

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    watch,
  } = useForm<LoginFormData>({
    resolver: safeZodResolver,
    mode: 'onBlur',
    reValidateMode: 'onChange',
    criteriaMode: 'firstError',
    defaultValues: { email: '', password: '' },
    shouldFocusError: false,
  });

  const watchedFields = watch();

  const onSubmit = async (data: LoginFormData) => {
    setIsLoading(true);
    try {
      await onLogin({ email: data.email.trim(), password: data.password.trim() });
    } finally {
      setIsLoading(false);
    }
  };

  const handleDevBypass = async () => {
    setIsDevBypassLoading(true);
    try {
      const response = await apiClient.devBypass();
      if (!response.token || !response.user_id || !response.tenant_id || !response.role) {
        throw new Error('Dev bypass response missing required authentication fields');
      }
      if (onDevBypass) await onDevBypass();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      logger.error('Dev bypass failed', {
        component: 'LoginForm',
        operation: 'devBypass',
      }, err instanceof Error ? err : new Error(errorMessage));
    } finally {
      setIsDevBypassLoading(false);
    }
  };

  const handleRetry = useCallback(() => {
    setCheckAttempts(0);
    setBackendState('checking');
    setLoadingProgress(0);
    checkBackendHealth();
  }, [checkBackendHealth]);

  // Keep system health/model status fresh while on the login screen
  const refreshSystemStatus = useCallback(async () => {
    const [systemHealthRes, modelStatusRes] = await Promise.allSettled([
      apiClient.getHealthzAll(),
      apiClient.getBaseModelStatus(),
    ]);

    if (systemHealthRes.status === 'fulfilled') {
      setSystemHealth(systemHealthRes.value);
    }

    if (modelStatusRes.status === 'fulfilled') {
      setModelStatus(modelStatusRes.value);
    } else if (modelStatusRes.status === 'rejected') {
      // 401 before login is expected; keep showing "Login for status"
      setModelStatus(null);
    }
  }, []);

  // Poll for system health/model status once backend is ready
  useEffect(() => {
    if (backendState !== 'ready') return;

    refreshSystemStatus();
    const interval = setInterval(refreshSystemStatus, HEALTH_POLL_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [backendState, refreshSystemStatus]);

  // Handle role badge click with debounce
  const handleRoleBadgeClick = useCallback(async (role: string) => {
    const now = Date.now();
    if (now - lastClickTime < DEBOUNCE_MS) return;
    if (isLoading || isDevBypassLoading || loadingRole) return;

    setLastClickTime(now);
    setLoadingRole(role);

    try {
      const response = await apiClient.devBypass();
      if (!response.token || !response.user_id || !response.tenant_id || !response.role) {
        throw new Error('Dev bypass response missing required authentication fields');
      }
      // Save last used role
      try {
        localStorage.setItem(LAST_ROLE_KEY, role);
        setLastRole(role);
      } catch (e) {
        if (import.meta.env.DEV) {
          logger.warn('[LoginForm] localStorage write failed', {
            component: 'LoginForm',
            operation: 'localStorageWrite',
            role,
            error: e instanceof Error ? e.message : String(e),
          });
        }
      }
      if (onDevBypass) await onDevBypass();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      logger.error('Dev bypass via role badge failed', {
        component: 'LoginForm',
        operation: 'roleBadgeClick',
        role,
      }, err instanceof Error ? err : new Error(errorMessage));
    } finally {
      setLoadingRole(null);
    }
  }, [lastClickTime, isLoading, isDevBypassLoading, loadingRole, onDevBypass]);

  // Handle keyboard events for role badges
  const handleRoleBadgeKeyDown = useCallback((e: React.KeyboardEvent, role: string) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleRoleBadgeClick(role);
    }
  }, [handleRoleBadgeClick]);

  // Clear localStorage
  const handleClearLocalStorage = useCallback(() => {
    try {
      const keysToRemove = Object.keys(localStorage).filter(k =>
        LOCALSTORAGE_KEYS_TO_CLEAR.includes(k) ||
        k.startsWith('aos-wizard-') ||
        k.startsWith('aos-history-')
      );
      keysToRemove.forEach(k => localStorage.removeItem(k));
      // Also clear last role
      localStorage.removeItem(LAST_ROLE_KEY);
      setLastRole(null);
      setClearStorageMessage(`Cleared ${keysToRemove.length + 1} items`);
      setTimeout(() => setClearStorageMessage(null), 2000);
    } catch (err) {
      setClearStorageMessage('Failed to clear storage');
      setTimeout(() => setClearStorageMessage(null), 2000);
    }
  }, []);

  // Calculate system health summary
  const healthSummary = useMemo(() => {
    if (!systemHealth?.components) return { healthy: 0, degraded: 0, unhealthy: 0, total: 0 };
    const components = Object.values(systemHealth.components);
    return {
      healthy: components.filter(c => c.status === 'healthy').length,
      degraded: components.filter(c => c.status === 'degraded').length,
      unhealthy: components.filter(c => c.status === 'unhealthy').length,
      total: components.length,
    };
  }, [systemHealth]);

  const workerAvailable = useMemo(() => {
    if (!systemHealth?.components) return false;
    const components = Object.values(systemHealth.components);
    const kernelComponent = components.find((c) => c.component === 'kernel');
    return kernelComponent?.details && typeof kernelComponent.details === 'object' && 'worker_available' in kernelComponent.details
      ? (kernelComponent.details as Record<string, unknown>).worker_available === true
      : false;
  }, [systemHealth]);

  // Loading/Error State - Full Screen
  if (backendState !== 'ready') {
    return (
      <div className="min-h-screen flex items-center justify-center p-6 bg-background">
        {/* Subtle grid pattern background */}
        <div className="absolute inset-0 bg-[linear-gradient(var(--gray-200)_1px,transparent_1px),linear-gradient(90deg,var(--gray-200)_1px,transparent_1px)] bg-[size:64px_64px] opacity-50" />

        <div className="relative w-full max-w-md space-y-8">
          {/* Logo and Title */}
          <div className="text-center space-y-3">
            <div className="inline-flex items-center gap-3 px-4 py-2">
              <div className="p-2.5 bg-foreground rounded-xl shadow-md">
                <Boxes className="h-7 w-7 text-background" />
              </div>
              <span className="text-2xl font-bold text-foreground tracking-tight">AdapterOS</span>
            </div>
            <p className="text-sm text-muted-foreground">Enterprise Adapter Management Platform</p>
          </div>

          {/* Connection Status Card */}
          <Card className="bg-card/60 backdrop-blur-2xl border-border/50 shadow-xl border-white/20">
            <CardHeader className="pb-3">
              <CardTitle className="text-base flex items-center gap-2 text-foreground">
                <Server className="h-4 w-4 text-muted-foreground" />
                System Initialization
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {backendState === 'error' ? (
                <>
                  <div className="flex items-center gap-3 p-4 rounded-xl bg-destructive/10 border border-destructive/20">
                    <XCircle className="h-6 w-6 text-destructive flex-shrink-0" />
                    <div className="space-y-1">
                      <p className="text-sm font-medium text-destructive">
                        Backend Unavailable
                      </p>
                      <p className="text-xs text-destructive/80">
                        Connection failed after {checkAttempts} attempts
                      </p>
                    </div>
                  </div>
                  <Button onClick={handleRetry} className="w-full" variant="secondary" size="lg">
                    <RefreshCw className="h-4 w-4 mr-2" />
                    Retry Connection
                  </Button>
                </>
              ) : (
                <div className="flex flex-col items-center gap-6 py-8">
                  <div className="relative">
                    <div className="absolute inset-0 h-16 w-16 animate-ping opacity-20 rounded-full bg-foreground" />
                    <div className="relative p-4 bg-muted rounded-full">
                      <Loader2 className="h-8 w-8 animate-spin text-foreground" />
                    </div>
                  </div>
                  <div className="text-center space-y-2">
                    <p className="text-sm font-medium text-foreground">
                      {backendState === 'checking' ? 'Connecting to AdapterOS' : 'Waiting for services'}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Attempt {checkAttempts + 1} of {MAX_RETRY_ATTEMPTS}
                    </p>
                  </div>
                  <div className="w-full space-y-2">
                    <Progress value={loadingProgress} className="h-1.5" />
                    <div className="flex justify-between text-[10px] text-muted-foreground">
                      <span>Connecting</span>
                      <span>{loadingProgress}%</span>
                    </div>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  // Main Login Page - Frosted Glass Design
  return (
    <div className="min-h-screen flex relative overflow-hidden">
      {/* Frosted Glass Background */}
      <div className="absolute inset-0 bg-gradient-to-br from-background/95 via-background/90 to-background/95 backdrop-blur-2xl">
        {/* Enhanced grid pattern with frosted effect */}
        <div className="absolute inset-0 bg-[linear-gradient(var(--gray-200)_1px,transparent_1px),linear-gradient(90deg,var(--gray-200)_1px,transparent_1px)] bg-[size:64px_64px] opacity-30" />

        {/* Subtle radial gradient overlay for depth */}
        <div className="absolute inset-0 bg-gradient-radial from-transparent via-background/20 to-background/40" />

        {/* Frosted glass texture */}
        <div className="absolute inset-0 bg-white/5 backdrop-blur-sm" />
      </div>

      {/* Environment Banner */}
      {(devBypassAllowed || metaInfo?.environment === 'staging') && (
        <div className={cn(
          "fixed top-0 inset-x-0 z-50 py-1.5 text-center text-xs font-medium backdrop-blur-xl bg-background/80 border-b border-border/50",
          devBypassAllowed
            ? "text-warning"
            : "text-info"
        )}>
          {devBypassAllowed ? "DEVELOPMENT MODE" : "STAGING ENVIRONMENT"}
        </div>
      )}

      {/* Left side - Branding & Status */}
      <div className="hidden lg:flex lg:w-1/2 relative items-center justify-center p-12">
        <div className="max-w-md space-y-8">
          {/* Logo */}
          <div className="space-y-4">
            <div className="inline-flex items-center gap-3">
              <div className="p-3 bg-foreground rounded-2xl shadow-lg">
                <Boxes className="h-8 w-8 text-background" />
              </div>
              <span className="text-3xl font-bold text-foreground tracking-tight">AdapterOS</span>
            </div>
            <p className="text-lg text-muted-foreground leading-relaxed">
              Deterministic LoRA adapter orchestration for enterprise AI deployments
            </p>
          </div>


          {/* System Status Compact */}
          <div className="p-4 bg-card/60 border border-border/50 rounded-xl backdrop-blur-2xl shadow-lg border-white/20">
            <div className="flex items-center justify-between mb-3">
              <span className="text-sm font-medium text-foreground">System Status</span>
              <Badge
                variant="outline"
                className={cn(
                  "text-xs",
                  backendHealth?.status === 'healthy'
                    ? 'bg-success/10 text-success border-success/30'
                    : 'bg-warning/10 text-warning border-warning/30'
                )}
              >
                {backendHealth?.status === 'healthy' ? 'Operational' : backendHealth?.status?.toUpperCase() || 'Unknown'}
              </Badge>
            </div>
            <div className="grid grid-cols-2 gap-3 text-xs">
              <div className="flex items-center gap-2 text-muted-foreground">
                <Server className="h-3.5 w-3.5" />
                <span>
                  {healthSummary.total > 0
                    ? `${healthSummary.healthy}/${healthSummary.total} Services`
                    : 'Services Ready'}
                </span>
              </div>
              <div className="flex items-center gap-2 text-muted-foreground">
                <Cpu className="h-3.5 w-3.5" />
                <span
                  title={modelStatus?.model_path || modelStatus?.model_name || undefined}
                >
                  {(() => {
                    // If we have model_name and it's not the placeholder, use it
                    if (modelStatus?.model_name && modelStatus.model_name !== 'No Model Loaded') {
                      return modelStatus.model_name.split('/').pop()?.substring(0, 15);
                    }
                    // If model_name is placeholder but we have model_path, use that
                    if (modelStatus?.model_path && modelStatus.model_path.trim()) {
                      return modelStatus.model_path.split('/').pop()?.substring(0, 15);
                    }
                    // Fallback to checking worker availability
                    return workerAvailable ? 'Worker Ready' : 'No Model';
                  })()}
                </span>
              </div>
              {systemHealth?.uptime_seconds && (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  <span>Uptime: {formatDurationSeconds(systemHealth.uptime_seconds)}</span>
                </div>
              )}
              {metaInfo?.version && (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <FileCheck className="h-3.5 w-3.5" />
                  <span>v{metaInfo.version}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Right side - Login Form */}
      <div className="flex-1 flex items-center justify-center p-6 lg:p-12">
        <div className="w-full max-w-md space-y-6">
          {/* Mobile Logo */}
          <div className="lg:hidden text-center space-y-3 mb-8">
            <div className="inline-flex items-center gap-3">
              <div className="p-2.5 bg-foreground rounded-xl shadow-md">
                <Boxes className="h-7 w-7 text-background" />
              </div>
              <span className="text-2xl font-bold text-foreground tracking-tight">AdapterOS</span>
            </div>
          </div>

          {/* Login Card */}
          <Card className="bg-card/60 backdrop-blur-2xl border-border/50 shadow-xl border-white/20">
            <CardHeader className="space-y-1 pb-4">
              <CardTitle className="text-xl text-foreground flex items-center gap-2">
                <Lock className="h-5 w-5 text-muted-foreground" />
                Sign In
              </CardTitle>
              <CardDescription className="text-muted-foreground">
                Enter your credentials to access the control plane
              </CardDescription>
            </CardHeader>
            <CardContent>
              <form onSubmit={handleSubmit(onSubmit)} className="space-y-4" aria-label="Login form">
                {/* Error Display */}
                {error && (
                  <Alert variant="destructive">
                    <XCircle className="h-4 w-4" />
                    <AlertDescription>{error}</AlertDescription>
                  </Alert>
                )}

                {/* Email Field */}
                <div className="space-y-2">
                  <Label htmlFor="email" className="text-foreground">Email</Label>
                  <Input
                    id="email"
                    type="email"
                    placeholder="your@email.com"
                    autoComplete="email"
                    aria-describedby={errors.email ? 'email-error' : undefined}
                    aria-invalid={errors.email ? 'true' : 'false'}
                    {...register('email')}
                    className={cn(
                      errors.email && 'border-destructive focus-visible:ring-destructive/20'
                    )}
                    disabled={isLoading || isDevBypassLoading}
                  />
                  {errors.email && (
                    <p id="email-error" className="text-sm text-destructive flex items-center gap-1" role="alert">
                      <XCircle className="h-3 w-3" aria-hidden="true" />
                      {errors.email.message}
                    </p>
                  )}
                </div>

                {/* Password Field */}
                <div className="space-y-2">
                  <Label htmlFor="password" className="text-foreground">Password</Label>
                  <Input
                    id="password"
                    type="password"
                    placeholder="Enter your password"
                    autoComplete="current-password"
                    aria-describedby={errors.password ? 'password-error' : undefined}
                    aria-invalid={errors.password ? 'true' : 'false'}
                    {...register('password')}
                    className={cn(
                      errors.password && 'border-destructive focus-visible:ring-destructive/20'
                    )}
                    disabled={isLoading || isDevBypassLoading}
                  />
                  {errors.password && (
                    <p id="password-error" className="text-sm text-destructive flex items-center gap-1" role="alert">
                      <XCircle className="h-3 w-3" aria-hidden="true" />
                      {errors.password.message}
                    </p>
                  )}
                </div>

                {/* Loading Progress */}
                {(isLoading || isDevBypassLoading) && (
                  <div className="space-y-2" role="status" aria-live="polite">
                    <Progress value={undefined} className="h-1" aria-label="Authentication in progress" />
                    <p className="text-xs text-muted-foreground text-center">
                      {isLoading ? 'Authenticating...' : 'Activating dev bypass...'}
                    </p>
                  </div>
                )}

                {/* Login Button */}
                <Button
                  type="submit"
                  className="w-full shadow-sm border border-border"
                  disabled={isLoading || isDevBypassLoading || !watchedFields.email?.trim() || !watchedFields.password?.trim()}
                >
                  {isLoading ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Authenticating...
                    </>
                  ) : (
                    'Sign In'
                  )}
                </Button>

                {/* Dev Bypass Section */}
                {devBypassAllowed && (
                  <>
                    <div className="relative py-2">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t border-border" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="bg-card px-3 text-muted-foreground">Development Mode</span>
                      </div>
                    </div>

                    <Button
                      type="button"
                      variant="outline"
                      className="w-full border-warning/30 bg-warning/5 text-warning hover:bg-warning/10 hover:border-warning/50"
                      onClick={handleDevBypass}
                      disabled={isDevBypassLoading || isLoading}
                    >
                      {isDevBypassLoading ? (
                        <>
                          <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                          Activating...
                        </>
                      ) : (
                        <>
                          <Zap className="h-4 w-4 mr-2" />
                          Quick Access (Dev Bypass)
                        </>
                      )}
                    </Button>
                  </>
                )}
              </form>
            </CardContent>
          </Card>

          {/* Dev Tools Panel - Only in Dev Mode */}
          {devBypassAllowed && (
            <div className="p-4 bg-card/50 border border-border/50 rounded-xl backdrop-blur-2xl space-y-4 shadow-lg border-white/20">
              {/* Quick Login Section */}
              <div>
                <p className="text-xs text-muted-foreground mb-3 flex items-center gap-1.5">
                  <Zap className="h-3.5 w-3.5 text-warning" />
                  Quick Login (click to sign in)
                </p>
                <div className="flex flex-wrap gap-2">
                  {DEV_ROLES.map((role) => (
                    <Badge
                      key={role}
                      variant="outline"
                      role="button"
                      tabIndex={0}
                      aria-label={`Sign in as ${role}`}
                      onClick={() => handleRoleBadgeClick(role)}
                      onKeyDown={(e) => handleRoleBadgeKeyDown(e, role)}
                      className={cn(
                        "text-xs py-1.5 px-3 transition-all duration-150",
                        "cursor-pointer select-none",
                        "focus:outline-none focus:ring-2 focus:ring-ring/50",
                        loadingRole === role
                          ? "bg-primary/10 border-primary/50 text-primary"
                          : lastRole === role
                            ? "bg-warning/10 border-warning/40 text-warning hover:bg-warning/20"
                            : "bg-muted border-border text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                        (isLoading || isDevBypassLoading || loadingRole) && loadingRole !== role
                          ? "opacity-50 cursor-not-allowed"
                          : ""
                      )}
                    >
                      {loadingRole === role ? (
                        <span className="flex items-center gap-1.5">
                          <Loader2 className="h-3 w-3 animate-spin" />
                          Signing in...
                        </span>
                      ) : (
                        <>
                          {role}@aos.local
                          {lastRole === role && (
                            <span className="ml-1 text-[10px] text-warning">(last)</span>
                          )}
                        </>
                      )}
                    </Badge>
                  ))}
                </div>
              </div>

              {/* Clear Storage Section */}
              <div className="flex items-center justify-between pt-2 border-t border-border">
                <span className="text-xs text-muted-foreground">Reset UI State</span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleClearLocalStorage}
                  className="h-7 px-2 text-xs text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                >
                  <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                  {clearStorageMessage || 'Clear Storage'}
                </Button>
              </div>

              {/* Debug Info Panel */}
              <Collapsible open={debugPanelOpen} onOpenChange={setDebugPanelOpen}>
                <CollapsibleTrigger className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors w-full pt-2 border-t border-border">
                  <ChevronDown className={cn(
                    "h-3.5 w-3.5 transition-transform",
                    debugPanelOpen ? "rotate-180" : ""
                  )} />
                  Debug Info
                </CollapsibleTrigger>
                <CollapsibleContent className="mt-2">
                  <div className="p-3 bg-muted/50 rounded-lg text-xs font-mono space-y-1.5 text-muted-foreground">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Environment:</span>
                      <span>{metaInfo?.environment || 'unknown'}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Backend:</span>
                      <span className={backendHealth?.status === 'healthy' ? 'text-success' : 'text-warning'}>
                        {backendHealth?.status || 'unknown'}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Version:</span>
                      <span>{metaInfo?.version || 'unknown'}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Git Commit:</span>
                      <span>{metaInfo?.git_commit?.substring(0, 7) || 'unknown'}</span>
                    </div>
                    {systemHealth?.uptime_seconds && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground/70">Uptime:</span>
                        <span>{formatDurationSeconds(systemHealth.uptime_seconds)}</span>
                      </div>
                    )}
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Services:</span>
                      <span>
                        {healthSummary.healthy}/{healthSummary.total} healthy
                        {healthSummary.degraded > 0 && `, ${healthSummary.degraded} degraded`}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Model:</span>
                      <span
                        className="truncate max-w-[180px]"
                        title={modelStatus?.model_path || modelStatus?.model_name || (workerAvailable ? 'Worker Ready' : 'No Model')}
                      >
                        {modelStatus?.model_name && modelStatus.model_name !== 'No Model Loaded'
                          ? modelStatus.model_name.split('/').pop()
                          : modelStatus?.model_path?.split('/').pop()
                            || (workerAvailable ? 'Worker Ready' : 'No Model')}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground/70">Dev Bypass:</span>
                      <span className="text-success">enabled</span>
                    </div>
                  </div>
                </CollapsibleContent>
              </Collapsible>
            </div>
          )}

          {/* Footer */}
          {metaInfo?.git_commit && (
            <p className="text-center text-xs text-muted-foreground/70">
              Build {metaInfo.git_commit.substring(0, 7)} · {metaInfo.environment || 'development'}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
