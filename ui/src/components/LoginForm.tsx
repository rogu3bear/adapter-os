import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { useForm, type Resolver } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Progress } from './ui/progress';
import { Badge } from './ui/badge';
import { Lock, Shield, AlertTriangle, XCircle, Zap, Clock, Server, CheckCircle2, XCircle as XCircleIcon, Loader2, RefreshCw } from 'lucide-react';
import { Alert, AlertDescription } from './ui/alert';
import { apiClient } from '../api/client';
import { LoginFormSchema, type LoginFormData } from '../schemas/common.schema';
import { useServiceStatus } from '../hooks/useServiceStatus';
import { cn } from './ui/utils';
import type { HealthResponse } from '../api/api-types';
import type { AuthConfigResponse } from '../api/auth-types';

// Wrapped resolver that catches validation errors silently during initial render
// This prevents unhandled promise rejections from zodResolver
const safeZodResolver: Resolver<LoginFormData> = async (values, context, options) => {
  try {
    return await zodResolver(LoginFormSchema)(values, context, options);
  } catch {
    // Return empty errors on exception - validation will retry on user interaction
    return { values: {} as LoginFormData, errors: {} };
  }
};

interface LoginFormProps {
  onLogin: (credentials: { email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}

interface LoginHeaderProps {
  currentTime: Date;
}

const LoginHeader = ({ currentTime }: LoginHeaderProps) => (
  <div className="text-center space-y-3">
    <div className="flex justify-center">
      <div className="flex items-center justify-center bg-primary text-primary-foreground p-3 rounded-lg">
        <Lock className="h-6 w-6" />
        <span className="font-medium ml-2">AdapterOS</span>
      </div>
    </div>
    <h1 className="font-medium text-xl">System Login</h1>
    <div className="flex items-center justify-center space-x-2 text-muted-foreground text-xs">
      <Clock className="h-3 w-3" />
      <span className="font-mono">
        {currentTime.toLocaleTimeString('en-US', {
          hour12: false,
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        })}
      </span>
    </div>
  </div>
);

interface ServiceStatus {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'error';
}

const getServiceStatusIcon = (status: ServiceStatus['status']) => {
  switch (status) {
    case 'running':
      return <CheckCircle2 className="h-4 w-4 text-green-500" />;
    case 'starting':
      return <Loader2 className="h-4 w-4 text-blue-500 animate-spin" />;
    case 'error':
      return <XCircleIcon className="h-4 w-4 text-red-500" />;
    default:
      return <XCircleIcon className="h-4 w-4 text-gray-400" />;
  }
};

interface ServiceStatusOverviewProps {
  statuses: ServiceStatus[];
  running: number;
  total: number;
  onStartService?: (serviceId: string) => void;
}

const getSystemHealthLabel = (running: number, total: number): { label: string; severity: 'healthy' | 'degraded' | 'critical' } => {
  if (total === 0) return { label: 'Unknown', severity: 'degraded' };
  const ratio = running / total;
  if (ratio === 1) return { label: 'All systems operational', severity: 'healthy' };
  if (ratio >= 0.5) return { label: 'Degraded: some services offline', severity: 'degraded' };
  return { label: 'Critical: most services offline', severity: 'critical' };
};

const ServiceStatusOverview = ({
  statuses,
  running,
  total,
  onStartService,
}: ServiceStatusOverviewProps) => {
  const { label: healthLabel, severity } = getSystemHealthLabel(running, total);

  // Sort services: failed/stopped first (critical), then starting, then running
  const sortedStatuses = [...statuses].sort((a, b) => {
    const priority: Record<ServiceStatus['status'], number> = { error: 0, stopped: 1, starting: 2, running: 3 };
    return priority[a.status] - priority[b.status];
  });

  const severityStyles = {
    healthy: 'text-green-700 dark:text-green-400',
    degraded: 'text-amber-700 dark:text-amber-400',
    critical: 'text-red-700 dark:text-red-400',
  };

  return (
    <div className="space-y-3">
      {/* Health summary banner */}
      <div className={cn(
        "flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium",
        severity === 'healthy' && "bg-green-50 border border-green-200 dark:bg-green-950/30 dark:border-green-800",
        severity === 'degraded' && "bg-amber-50 border border-amber-200 dark:bg-amber-950/30 dark:border-amber-800",
        severity === 'critical' && "bg-red-50 border border-red-200 dark:bg-red-950/30 dark:border-red-800"
      )}>
        {severity === 'healthy' && <CheckCircle2 className="h-4 w-4 text-green-600 dark:text-green-400" />}
        {severity === 'degraded' && <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />}
        {severity === 'critical' && <XCircle className="h-4 w-4 text-red-600 dark:text-red-400" />}
        <span className={severityStyles[severity]}>{healthLabel}</span>
      </div>

      {/* Service list with inline actions */}
      <div className="space-y-2">
        {sortedStatuses.map((service) => {
          const isOffline = service.status === 'stopped' || service.status === 'error';
          return (
            <div
              key={service.id}
              className={cn(
                "flex items-center justify-between p-3 rounded-lg border transition-colors",
                isOffline
                  ? "bg-amber-50/50 border-amber-300 dark:bg-amber-950/20 dark:border-amber-700"
                  : "bg-background/50 border-border"
              )}
            >
              <div className="flex items-center gap-3">
                {getServiceStatusIcon(service.status)}
                <span className="text-sm font-medium">{service.name}</span>
              </div>
              <div className="flex items-center gap-2">
                <StatusBadge status={service.status} />
                {isOffline && onStartService && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => onStartService(service.id)}
                    className="h-7 px-2 text-xs border-amber-400 text-amber-700 hover:bg-amber-100 dark:border-amber-600 dark:text-amber-300 dark:hover:bg-amber-900/30"
                  >
                    Start
                  </Button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const StatusBadge = ({ status }: { status: ServiceStatus['status'] }) => {
  const baseStyles = 'text-xs px-2.5 py-1 rounded-full font-medium uppercase tracking-wide';
  const variants: Record<ServiceStatus['status'], string> = {
    running: 'bg-green-100 text-green-800 border border-green-300 dark:bg-green-900/40 dark:text-green-300 dark:border-green-700',
    starting: 'bg-blue-100 text-blue-800 border border-blue-300 dark:bg-blue-900/40 dark:text-blue-300 dark:border-blue-700',
    error: 'bg-red-100 text-red-800 border-2 border-red-400 dark:bg-red-900/50 dark:text-red-200 dark:border-red-600',
    stopped: 'bg-amber-100 text-amber-800 border-2 border-amber-400 dark:bg-amber-900/50 dark:text-amber-200 dark:border-amber-600',
  };
  const labels: Record<ServiceStatus['status'], string> = {
    running: 'Online',
    starting: 'Starting',
    error: 'Failed',
    stopped: 'Offline',
  };
  return <span className={`${baseStyles} ${variants[status]}`}>{labels[status]}</span>;
};

interface ServiceStatus {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'error';
}

type BackendState = 'checking' | 'loading' | 'ready' | 'error';

export function LoginForm({ onLogin, onDevBypass, error }: LoginFormProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isDevBypassLoading, setIsDevBypassLoading] = useState(false);
  const [currentTime, setCurrentTime] = useState(new Date());
  const [backendState, setBackendState] = useState<BackendState>('checking');
  const [backendHealth, setBackendHealth] = useState<HealthResponse | null>(null);
  const [serviceStatuses, setServiceStatuses] = useState<ServiceStatus[]>([]);
  const [checkAttempts, setCheckAttempts] = useState(0);
  const [loadingProgress, setLoadingProgress] = useState(0);
  // Backend-driven dev bypass state (replaces import.meta.env.DEV)
  const [devBypassAllowed, setDevBypassAllowed] = useState(false);

  const { status: serviceStatus } = useServiceStatus();

  // Update time ticker every second
  useEffect(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  // Check backend health and determine state
  const checkBackendHealth = useCallback(async () => {
    try {
      const health = await apiClient.get<HealthResponse>('/healthz');
      setBackendHealth(health);

      if (health.status === 'healthy') {
        setBackendState('ready');
        setLoadingProgress(100);
      } else if (health.status === 'degraded') {
        // Degraded but functional - allow login
        setBackendState('ready');
        setLoadingProgress(90);
      } else {
        setBackendState('loading');
        setLoadingProgress(50);
      }
    } catch (err) {
      setCheckAttempts(prev => {
        const attempts = prev + 1;
        // If we've tried multiple times, show error
        if (attempts >= 3) {
          setBackendState('error');
          setLoadingProgress(0);
        } else {
          // Still loading - backend might be starting
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Poll backend health while loading
  useEffect(() => {
    if (backendState === 'loading' || backendState === 'checking') {
      const interval = setInterval(() => {
        checkBackendHealth();
      }, 2000); // Check every 2 seconds
      return () => clearInterval(interval);
    }
  }, [backendState, checkBackendHealth]);

  // Fetch auth config when backend is ready to get dev bypass status
  useEffect(() => {
    if (backendState === 'ready') {
      apiClient.getAuthConfig()
        .then((config: AuthConfigResponse) => {
          setDevBypassAllowed(config.dev_bypass_allowed ?? false);
        })
        .catch(() => {
          // If auth config endpoint fails, default to no dev bypass
          setDevBypassAllowed(false);
        });
    }
  }, [backendState]);

  // Update service statuses from API (only when backend is ready)
  useEffect(() => {
    if (backendState !== 'ready') {
      // Set minimal service info while loading
      setServiceStatuses([
        { id: 'backend', name: 'Backend Server', status: backendState === 'error' ? 'error' : backendState === 'loading' ? 'starting' : 'stopped' },
        { id: 'ui', name: 'UI Frontend', status: 'running' },
      ]);
      return;
    }

    // Backend is ready - get full service status
    if (serviceStatus?.services && serviceStatus.services.length > 0) {
      const services: ServiceStatus[] = serviceStatus.services.map((s: any) => ({
        id: s.id || s.name?.toLowerCase().replace(/\s+/g, '-') || 'unknown',
        name: s.name || s.id || 'Unknown Service',
        status: s.state === 'running' ? 'running' : s.state === 'failed' ? 'error' : s.state === 'starting' ? 'starting' : 'stopped',
      }));
      setServiceStatuses(services);
    } else if (backendHealth) {
      // Use health check data to infer service status
      const services: ServiceStatus[] = [
        { id: 'backend', name: 'Backend Server', status: backendHealth.status === 'healthy' ? 'running' : backendHealth.status === 'degraded' ? 'running' : 'error' },
        { id: 'ui', name: 'UI Frontend', status: 'running' },
      ];
      
      // Add component health as services
      if (backendHealth.components) {
        Object.entries(backendHealth.components).forEach(([name, component]) => {
          services.push({
            id: name.toLowerCase().replace(/\s+/g, '-'),
            name: name,
            status: component.status === 'healthy' ? 'running' : component.status === 'degraded' ? 'running' : 'error',
          });
        });
      }
      
      setServiceStatuses(services);
    }
  }, [backendState, serviceStatus, backendHealth]);

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    watch,
  } = useForm<LoginFormData>({
    resolver: safeZodResolver,
    mode: 'onBlur', // Validate on blur to prevent validation errors on initial render
    reValidateMode: 'onChange', // Re-validate on change after first blur
    criteriaMode: 'firstError', // Stop at first error to reduce noise
    defaultValues: {
      email: '',
      password: '',
    },
    shouldFocusError: false, // Prevent auto-focus on error which can trigger re-renders
  });

  const watchedFields = watch();

  const onSubmit = async (data: LoginFormData) => {
    setIsLoading(true);
    try {
      await onLogin({
        email: data.email.trim(),
        password: data.password.trim(),
      });
    } catch (err) {
      // Error is handled by parent component
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
      if (onDevBypass) {
        await onDevBypass();
      }
    } catch (err) {
      console.error('Dev bypass failed:', err instanceof Error ? err.message : String(err));
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

  const getServiceStatusIcon = (status: ServiceStatus['status']) => {
    switch (status) {
      case 'running':
        return <CheckCircle2 className="h-4 w-4 text-green-500" />;
      case 'starting':
        return <Loader2 className="h-4 w-4 text-blue-500 animate-spin" />;
      case 'error':
        return <XCircleIcon className="h-4 w-4 text-red-500" />;
      default:
        return <XCircleIcon className="h-4 w-4 text-gray-400" />;
    }
  };

  const totalServices = serviceStatuses.length || 1;
  const runningServices = serviceStatuses.filter((s) => s.status === 'running').length;
  const serviceProgress = useMemo(
    () => (totalServices > 0 ? (runningServices / totalServices) * 100 : 0),
    [runningServices, totalServices],
  );

  // Handler to start a specific service
  const handleStartService = useCallback((serviceId: string) => {
    // For now, just retry the connection - in production this would call an API
    handleRetry();
  }, [handleRetry]);

  // Show loading screen until backend is ready
  if (backendState !== 'ready') {
    const hasOfflineServices = serviceStatuses.some(s => s.status === 'stopped' || s.status === 'error');

    return (
      <div className="min-h-screen flex items-center justify-center p-6 bg-background">
        <div className="w-full max-w-md space-y-4">
          <LoginHeader currentTime={currentTime} />

          {/* Cluster Status Card */}
          <Card className="border-2">
            <CardHeader className="pb-3">
              <CardTitle className="text-base flex items-center gap-2">
                <Server className="h-4 w-4" />
                Cluster Status
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {backendState === 'error' ? (
                <>
                  {/* Critical error state */}
                  <div className="flex items-center gap-3 p-4 rounded-lg bg-red-50 border-2 border-red-300 dark:bg-red-950/30 dark:border-red-700">
                    <XCircle className="h-6 w-6 text-red-600 dark:text-red-400 flex-shrink-0" />
                    <div className="space-y-1">
                      <p className="text-sm font-medium text-red-800 dark:text-white">
                        Backend server not responding
                      </p>
                      <p className="text-xs text-red-600 dark:text-red-400">
                        Tried {checkAttempts} time{checkAttempts !== 1 ? 's' : ''}. Check that the server is running.
                      </p>
                    </div>
                  </div>
                  <ServiceStatusOverview
                    statuses={serviceStatuses}
                    running={runningServices}
                    total={totalServices}
                    onStartService={handleStartService}
                  />
                  <Button onClick={handleRetry} className="w-full" size="lg">
                    <RefreshCw className="h-4 w-4 mr-2" />
                    Retry Connection
                  </Button>
                </>
              ) : (
                <>
                  {/* Loading state with prominent spinner */}
                  <div className="flex flex-col items-center gap-4 py-4">
                    <div className="relative">
                      <Loader2 className="h-10 w-10 animate-spin text-primary" />
                      <div className="absolute inset-0 h-10 w-10 animate-ping opacity-20 rounded-full bg-primary" />
                    </div>
                    <div className="text-center space-y-1">
                      <p className="text-sm font-medium">
                        {backendState === 'checking' ? 'Connecting to backend' : 'Waiting for services'}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Attempt {checkAttempts + 1} of 3 · Retrying every 2s
                      </p>
                    </div>
                    <Progress value={loadingProgress} className="h-2 w-full" />
                  </div>

                  {/* Service Status with inline actions */}
                  <ServiceStatusOverview
                    statuses={serviceStatuses}
                    running={runningServices}
                    total={totalServices}
                    onStartService={handleStartService}
                  />

                  {/* Single action button when services are offline */}
                  {hasOfflineServices && (
                    <Button onClick={handleRetry} variant="outline" className="w-full">
                      <RefreshCw className="h-4 w-4 mr-2" />
                      Refresh Status
                    </Button>
                  )}
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  // Backend is ready - show full login form
  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-background">
      <div className="w-full max-w-2xl space-y-6">
        <LoginHeader currentTime={currentTime} />

        {/* Security Indicators */}
        <div className="flex items-center justify-center space-x-3 flex-wrap gap-2">
          <div className="flex items-center space-x-2 px-3 py-1 bg-green-100 text-green-800 rounded-full text-xs dark:bg-green-900/30 dark:text-green-400">
            <Shield className="h-3 w-3" />
            Zero Egress
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-xs dark:bg-blue-900/30 dark:text-blue-400">
            <Lock className="h-3 w-3" />
            CSP Enforced
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-yellow-100 text-yellow-800 rounded-full text-xs dark:bg-yellow-900/30 dark:text-yellow-400">
            <AlertTriangle className="h-3 w-3" />
            ITAR Compliance Active
          </div>
        </div>

        {/* Login Form */}
        <Card>
          <CardHeader>
            <CardTitle>Authentication Required</CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
              {/* Error Display */}
              {error && (
                <Alert variant="destructive" className="border-red-500 bg-red-50 dark:bg-red-900/20">
                  <XCircle className="h-4 w-4" />
                  <AlertDescription className="text-sm font-medium">{error}</AlertDescription>
                </Alert>
              )}

              {/* Email Field */}
              <div className="space-y-2">
                <Label htmlFor="email" className="text-sm font-medium">
                  Email
                </Label>
                <Input
                  id="email"
                  type="email"
                  placeholder="Enter your email"
                  {...register('email')}
                  className={cn(
                    "border-2 transition-colors",
                    errors.email 
                      ? "border-red-500 focus-visible:border-red-500 focus-visible:ring-red-500/20" 
                      : "border-border focus-visible:border-primary"
                  )}
                  disabled={isLoading || isDevBypassLoading}
                />
                {errors.email && (
                  <p className="text-sm text-red-500 flex items-center gap-1">
                    <XCircle className="h-3 w-3" />
                    {errors.email.message}
                  </p>
                )}
              </div>

              {/* Password Field */}
              <div className="space-y-2">
                <Label htmlFor="password" className="text-sm font-medium">
                  Password
                </Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter your password"
                  {...register('password')}
                  className={cn(
                    "border-2 transition-colors",
                    errors.password 
                      ? "border-red-500 focus-visible:border-red-500 focus-visible:ring-red-500/20" 
                      : "border-border focus-visible:border-primary"
                  )}
                  disabled={isLoading || isDevBypassLoading}
                />
                {errors.password && (
                  <p className="text-sm text-red-500 flex items-center gap-1">
                    <XCircle className="h-3 w-3" />
                    {errors.password.message}
                  </p>
                )}
              </div>

              {/* Loading Progress */}
              {(isLoading || isDevBypassLoading) && (
                <div className="space-y-2">
                  <Progress value={undefined} className="h-1" />
                  <p className="text-xs text-muted-foreground text-center">
                    {isLoading ? 'Authenticating...' : 'Activating dev bypass...'}
                  </p>
                </div>
              )}

              {/* Action Buttons */}
              <div className="space-y-3 pt-2">
                <Button
                  type="submit"
                  className="w-full"
                  disabled={isLoading || isDevBypassLoading || !isValid || !watchedFields.email?.trim() || !watchedFields.password?.trim()}
                >
                  {isLoading ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Authenticating...
                    </>
                  ) : (
                    'Secure Login'
                  )}
                </Button>

                {devBypassAllowed && (
                  <>
                    {/* Warning badge for dev bypass enabled */}
                    <div className="flex items-center justify-center gap-2 py-2">
                      <Badge variant="destructive" className="animate-pulse">
                        <AlertTriangle className="h-3 w-3 mr-1" />
                        Dev Bypass Enabled
                      </Badge>
                    </div>
                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t border-border" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="bg-card px-2 text-muted-foreground">Development</span>
                      </div>
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      className="w-full"
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
                          Dev Bypass (No Auth Required)
                        </>
                      )}
                    </Button>
                    <p className="text-xs text-muted-foreground text-center">
                      Dev bypass enabled in config - bypasses authentication
                    </p>
                  </>
                )}
              </div>
            </form>
          </CardContent>
        </Card>

        {/* Dev Mode Credentials - Only shown when dev bypass is allowed */}
        {devBypassAllowed && (
          <Card className="border-2 border-amber-400 bg-amber-50 dark:bg-amber-950/20 dark:border-amber-600">
            <CardHeader className="pb-2">
              <CardTitle className="text-sm flex items-center gap-2 text-amber-800 dark:text-amber-300">
                <AlertTriangle className="h-4 w-4" />
                Development Mode - Test Credentials
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-0">
              <div className="grid grid-cols-2 gap-3 text-xs">
                <div className="p-2 rounded bg-white/80 dark:bg-black/30 border border-amber-300 dark:border-amber-700">
                  <p className="font-semibold text-amber-900 dark:text-amber-200">Admin</p>
                  <p className="font-mono text-amber-800 dark:text-amber-300">admin@aos.local</p>
                  <p className="font-mono text-amber-700 dark:text-amber-400">password</p>
                </div>
                <div className="p-2 rounded bg-white/80 dark:bg-black/30 border border-amber-300 dark:border-amber-700">
                  <p className="font-semibold text-amber-900 dark:text-amber-200">Operator</p>
                  <p className="font-mono text-amber-800 dark:text-amber-300">operator@aos.local</p>
                  <p className="font-mono text-amber-700 dark:text-amber-400">password</p>
                </div>
                <div className="p-2 rounded bg-white/80 dark:bg-black/30 border border-amber-300 dark:border-amber-700">
                  <p className="font-semibold text-amber-900 dark:text-amber-200">SRE</p>
                  <p className="font-mono text-amber-800 dark:text-amber-300">sre@aos.local</p>
                  <p className="font-mono text-amber-700 dark:text-amber-400">password</p>
                </div>
                <div className="p-2 rounded bg-white/80 dark:bg-black/30 border border-amber-300 dark:border-amber-700">
                  <p className="font-semibold text-amber-900 dark:text-amber-200">Viewer</p>
                  <p className="font-mono text-amber-800 dark:text-amber-300">viewer@aos.local</p>
                  <p className="font-mono text-amber-700 dark:text-amber-400">password</p>
                </div>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
