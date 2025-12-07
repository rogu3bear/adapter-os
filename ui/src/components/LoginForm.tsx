import { useState, useEffect, useCallback, useRef } from 'react';
import { useForm, type Resolver } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';
import { apiClient } from '@/api/client';
import { LoginFormSchema, type LoginFormData } from '@/schemas/common.schema';
import { logger } from '@/utils/logger';
import type { AuthConfigResponse } from '@/api/auth-types';
import type { HealthResponse, SystemHealthResponse, ComponentHealth } from '@/api/api-types';

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
  onLogin: (credentials: { email: string; password: string; totp?: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
  lockoutMessage?: string | null;
  onConfigLoaded?: (config: AuthConfigResponse) => void;
  /** When true, shows TOTP field. Set by parent when MFA_REQUIRED error code is received */
  mfaRequired?: boolean;
}

const LockLogo = ({ className = 'h-10 w-10 text-foreground' }: { className?: string }) => (
  <svg
    viewBox="0 0 64 64"
    fill="none"
    role="img"
    aria-hidden="true"
    className={className}
  >
    <rect x="14" y="26" width="36" height="28" rx="6" className="fill-card stroke-foreground" strokeWidth="2" />
    <path
      d="M22 26v-6c0-6.627 5.373-12 12-12s12 5.373 12 12v6"
      className="stroke-foreground"
      strokeWidth="2.5"
      strokeLinecap="round"
    />
    <circle cx="32" cy="40" r="4" className="fill-foreground/20 stroke-foreground" strokeWidth="2" />
    <path d="M32 44v5" className="stroke-foreground" strokeWidth="2" strokeLinecap="round" />
  </svg>
);

const statusTone = (status: string) => {
  switch (status) {
    case 'healthy':
      return 'bg-emerald-500/10 text-emerald-700 border-emerald-300';
    case 'degraded':
      return 'bg-amber-500/10 text-amber-700 border-amber-300';
    case 'unhealthy':
    case 'issue':
      return 'bg-red-500/10 text-red-700 border-red-300';
    default:
      return 'bg-muted text-muted-foreground border-border';
  }
};

export function LoginForm({ onLogin, onDevBypass, error, lockoutMessage, onConfigLoaded, mfaRequired: mfaRequiredProp }: LoginFormProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isDevBypassLoading, setIsDevBypassLoading] = useState(false);
  const [devBypassAllowed, setDevBypassAllowed] = useState(false);
  const [showTotpField, setShowTotpField] = useState(false);
  const [configStatus, setConfigStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle');
  const [configError, setConfigError] = useState<string | null>(null);
  const [backendStatus, setBackendStatus] = useState<'checking' | 'ready' | 'issue'>('checking');
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [systemHealth, setSystemHealth] = useState<SystemHealthResponse | null>(null);
  const [showDetails, setShowDetails] = useState(false);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [devBypassError, setDevBypassError] = useState<string | null>(null);

  const fetchHealth = useCallback(async () => {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 10000); // 10s timeout

    try {
      const healthRes = await apiClient.request<HealthResponse>(
        '/healthz',
        { method: 'GET' },
        false, // skipRetry
        controller.signal
      );
      clearTimeout(timeoutId);
      setHealth(healthRes);
      setHealthError(null);

      try {
        const systemRes = await apiClient.getHealthzAll();
        setSystemHealth(systemRes);
      } catch {
        // System details may not be available yet; keep previous value if any.
      }

      const status = healthRes.status === 'healthy' ? 'ready' : 'issue';
      setBackendStatus(status);
    } catch (err) {
      clearTimeout(timeoutId);
      setBackendStatus('issue');
      if (err instanceof Error && err.name === 'AbortError') {
        setHealthError('Health check timed out.');
      } else {
        setHealthError('Unable to reach system health.');
      }
    }
  }, []);

  const loadAuthConfig = useCallback(async () => {
    setConfigStatus('loading');
    setConfigError(null);
    try {
      const config = await apiClient.getAuthConfig();
      setDevBypassAllowed(config.dev_bypass_allowed ?? false);
      setShowTotpField(config.mfa_required ?? false);
      logger.info('Auth config TTLs', {
        component: 'LoginForm',
        access_token_ttl_minutes: config.access_token_ttl_minutes,
        session_timeout_minutes: config.session_timeout_minutes,
      });
      onConfigLoaded?.(config);
      setConfigStatus('ready');
    } catch (err) {
      setConfigStatus('error');
      setConfigError('Unable to load sign-in settings. You can still try to sign in.');
      logger.warn('Auth config load failed', {
        component: 'LoginForm',
        operation: 'authConfig',
      });
    }
  }, [onConfigLoaded]);

  // Initial fetch on mount, then poll with adaptive timing
  const hasFetchedRef = useRef(false);
  useEffect(() => {
    if (!hasFetchedRef.current) {
      hasFetchedRef.current = true;
      fetchHealth();
    }
    const interval = setInterval(fetchHealth, backendStatus === 'ready' ? 10000 : 2500);
    return () => clearInterval(interval);
  }, [fetchHealth, backendStatus]);

  useEffect(() => {
    if (backendStatus === 'ready') {
      loadAuthConfig();
    }
  }, [backendStatus, loadAuthConfig]);

  const {
    register,
    handleSubmit,
    formState: { errors },
    watch,
  } = useForm<LoginFormData>({
    resolver: safeZodResolver,
    mode: 'onBlur',
    reValidateMode: 'onChange',
    criteriaMode: 'firstError',
    defaultValues: { email: '', password: '', totp: '' },
    shouldFocusError: false,
  });

  const watchedFields = watch();

  const onSubmit = async (data: LoginFormData) => {
    if (lockoutMessage) {
      return;
    }
    setIsLoading(true);
    try {
      await onLogin({
        email: data.email.trim(),
        password: data.password.trim(),
        totp: data.totp?.trim() || undefined,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleDevBypass = async () => {
    setIsDevBypassLoading(true);
    setDevBypassError(null);
    try {
      const response = await apiClient.devBypass();
      if (!response.token || !response.user_id || !response.tenant_id || !response.role) {
        throw new Error('Dev bypass response missing required authentication fields');
      }
      if (onDevBypass) await onDevBypass();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setDevBypassError(errorMessage);
      logger.error('Dev bypass failed', {
        component: 'LoginForm',
        operation: 'devBypass',
      }, err instanceof Error ? err : new Error(errorMessage));
    } finally {
      setIsDevBypassLoading(false);
    }
  };

  useEffect(() => {
    if (mfaRequiredProp) {
      setShowTotpField(true);
    }
  }, [mfaRequiredProp]);

  const componentsToShow: Record<string, ComponentHealth> =
    systemHealth?.components ?? health?.components ?? {};
  const issueEntries = Object.entries(componentsToShow)
    .map(([name, comp]) => ({
      name,
      status: comp?.status ?? 'unknown',
      message: comp?.message,
    }))
    .filter((item) => item.status !== 'healthy');
  const systemStatus = health?.status || systemHealth?.status || 'unknown';
  const isReady = backendStatus === 'ready' && systemStatus === 'healthy';
  const lastUpdated = systemHealth?.timestamp
    ? new Date(systemHealth.timestamp).toLocaleTimeString()
    : null;
  const backendUpdates = [
    {
      title: 'Overall health',
      status: systemStatus,
      message: isReady ? 'All critical services are healthy.' : 'Waiting for services to report healthy.',
    },
    ...Object.entries(componentsToShow)
      .slice(0, 4)
      .map(([name, comp]) => ({
        title: name,
        status: comp?.status ?? 'unknown',
        message: comp?.message || 'No detail reported yet.',
      })),
  ];

  return (
    <main className="relative min-h-screen bg-background px-6 py-12 flex items-center justify-center overflow-hidden">
      <div className="pointer-events-none absolute inset-0 opacity-80">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_20%_20%,rgba(255,255,255,0.05),transparent_35%),radial-gradient(circle_at_80%_0%,rgba(0,0,0,0.05),transparent_30%),radial-gradient(circle_at_40%_80%,rgba(255,255,255,0.03),transparent_30%)]" />
        <div className="absolute inset-0 bg-[linear-gradient(120deg,rgba(255,255,255,0.04) 0%,rgba(255,255,255,0.02) 40%,transparent 60%)] bg-[length:140%_140%]" />
      </div>

      <div className="relative w-full max-w-6xl">
        <div className="grid gap-8 lg:grid-cols-2">
          <div className="space-y-8">
            <section className="rounded-lg border bg-card/95 backdrop-blur-sm p-5 space-y-4 text-center shadow-sm transition-all duration-300 hover:shadow-md">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:text-left">
                <div className="space-y-1">
                  <h2 className="text-xl font-semibold">System status</h2>
                  <p className="text-sm text-muted-foreground">
                    {isReady
                      ? 'System is up. Sign in is available.'
                      : 'Waiting for critical services to become healthy.'}
                  </p>
                </div>
                <div className="flex items-center justify-center gap-2 sm:justify-end">
                  <span
                    className={`rounded-full border px-3 py-1 text-sm font-medium capitalize ${statusTone(systemStatus)} ${backendStatus !== 'ready' ? 'ring-2 ring-border/60' : ''}`}
                  >
                    {systemStatus}
                  </span>
                </div>
              </div>

              {healthError && (
                <p className="text-sm text-destructive">{healthError}</p>
              )}

              {backendStatus !== 'ready' && (
                <div className="space-y-2">
                  <h3 className="text-sm font-semibold">What&apos;s not working yet</h3>
                  {issueEntries.length ? (
                    <ul className="list-disc space-y-1 pl-5 text-sm text-muted-foreground">
                      {issueEntries.slice(0, 3).map((item) => (
                        <li key={item.name} className="flex flex-col">
                          <span className="font-medium text-foreground">{item.name}</span>
                          <span className="text-xs capitalize">{item.status}</span>
                          {item.message && (
                            <span className="text-xs text-muted-foreground">{item.message}</span>
                          )}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-sm text-muted-foreground">Waiting for component checks...</p>
                  )}
                </div>
              )}

              <div className="flex flex-wrap gap-3">
                <Button
                  type="button"
                  variant="link"
                  size="sm"
                  className="px-0"
                  onClick={() => setShowDetails((prev) => !prev)}
                >
                  {showDetails ? 'Hide details' : 'View details'}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={fetchHealth}
                  disabled={backendStatus === 'checking'}
                >
                  {backendStatus === 'checking' ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Checking...
                    </>
                  ) : (
                    'Refresh status'
                  )}
                </Button>
              </div>

              {showDetails && (
                <div className="rounded-md border bg-muted/30 p-3 space-y-3 text-left">
                  {Object.keys(componentsToShow).length ? (
                    Object.entries(componentsToShow).map(([name, comp]) => {
                      const compStatus = comp?.status ?? 'unknown';
                      return (
                        <div
                          key={name}
                          className="space-y-1 border-b border-border/60 pb-2 last:border-b-0 last:pb-0"
                        >
                          <div className="flex items-center justify-between text-sm font-medium">
                            <span>{name}</span>
                            <span className="capitalize text-muted-foreground">{compStatus}</span>
                          </div>
                          {comp?.message && (
                            <p className="text-xs text-muted-foreground">{comp.message}</p>
                          )}
                        </div>
                      );
                    })
                  ) : (
                    <div className="space-y-2">
                      <p className="text-sm text-muted-foreground">
                        No component details available yet. Authentication/health checks may still be starting.
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Use Refresh status or check authentication logs/worker stderr to see startup issues as they appear.
                      </p>
                    </div>
                  )}
                </div>
              )}

              <div className="rounded-md border bg-muted/40 p-3 space-y-3 text-left transition-all duration-300 hover:shadow-sm">
                <div className="flex items-center justify-between text-sm font-semibold">
                  <span>Backend updates</span>
                  {lastUpdated && <span className="text-xs text-muted-foreground">Last update {lastUpdated}</span>}
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  {backendUpdates.map((item) => (
                    <div key={`${item.title}-${item.status}`} className="rounded border border-border/60 bg-card/70 p-3 space-y-1 transition-all duration-300 hover:shadow-sm">
                      <div className="flex items-center justify-between text-sm font-medium">
                        <span>{item.title}</span>
                        <span
                          className={`rounded-full border px-2 py-0.5 text-xs capitalize ${statusTone(item.status)}`}
                        >
                          {item.status}
                        </span>
                      </div>
                      <p className="text-xs text-muted-foreground leading-relaxed">{item.message}</p>
                    </div>
                  ))}
                </div>
              </div>
            </section>
          </div>

          <div className="space-y-8">
            <header className="flex justify-end">
              <div className="flex items-center gap-3 text-right">
                <LockLogo />
                <div>
                  <h1 className="text-4xl font-semibold tracking-tight">AdapterOS</h1>
                  <p className="text-base text-muted-foreground">Sign in to the control plane.</p>
                </div>
              </div>
            </header>

            {isReady ? (
              <>
                {(configStatus === 'loading' || configStatus === 'error') && (
                  <section className="rounded-lg border bg-card p-5 space-y-3" aria-live="polite">
                    <h2 className="text-xl font-semibold">Preparing sign-in</h2>
                    <p className="text-sm text-muted-foreground">
                      {configStatus === 'loading'
                        ? 'Loading sign-in settings...'
                        : configError || 'Unable to load sign-in settings. You can still continue.'}
                    </p>
                    <div className="flex gap-3">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={loadAuthConfig}
                        disabled={configStatus === 'loading'}
                      >
                        {configStatus === 'loading' ? (
                          <>
                            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                            Loading
                          </>
                        ) : (
                          'Retry'
                        )}
                      </Button>
                    </div>
                  </section>
                )}

                <section className="rounded-lg border bg-card p-8 space-y-6">
                  <h2 className="text-xl font-semibold">Sign in</h2>
                  <form onSubmit={handleSubmit(onSubmit)} className="space-y-5" aria-label="Login form">
                    {lockoutMessage && (
                      <Alert variant="destructive">
                        <AlertDescription>{lockoutMessage}</AlertDescription>
                      </Alert>
                    )}

                    {error && (
                      <Alert variant="destructive">
                        <AlertDescription>{error}</AlertDescription>
                      </Alert>
                    )}

                    <div className="grid gap-5 md:grid-cols-2">
                      <div className="space-y-2">
                        <Label htmlFor="email">Email</Label>
                        <Input
                          id="email"
                          type="email"
                          placeholder="you@example.com"
                          autoComplete="email"
                          aria-describedby={errors.email ? 'email-error' : undefined}
                          aria-invalid={errors.email ? 'true' : 'false'}
                          {...register('email')}
                          disabled={isLoading || isDevBypassLoading}
                        />
                        {errors.email && (
                          <p id="email-error" className="text-sm text-destructive" role="alert">
                            {errors.email.message}
                          </p>
                        )}
                      </div>

                      <div className="space-y-2">
                        <Label htmlFor="password">Password</Label>
                        <Input
                          id="password"
                          type="password"
                          placeholder="Enter your password"
                          autoComplete="current-password"
                          aria-describedby={errors.password ? 'password-error' : undefined}
                          aria-invalid={errors.password ? 'true' : 'false'}
                          {...register('password')}
                          disabled={isLoading || isDevBypassLoading}
                        />
                        {errors.password && (
                          <p id="password-error" className="text-sm text-destructive" role="alert">
                            {errors.password.message}
                          </p>
                        )}
                      </div>
                    </div>

                    {showTotpField ? (
                      <div className="space-y-2 max-w-sm">
                        <Label htmlFor="totp">TOTP code</Label>
                        <Input
                          id="totp"
                          type="text"
                          inputMode="numeric"
                          placeholder="6-digit code"
                          autoComplete="one-time-code"
                          aria-describedby={errors.totp ? 'totp-error' : undefined}
                          aria-invalid={errors.totp ? 'true' : 'false'}
                          {...register('totp')}
                          disabled={isLoading || isDevBypassLoading}
                        />
                        {errors.totp && (
                          <p id="totp-error" className="text-sm text-destructive" role="alert">
                            {errors.totp.message}
                          </p>
                        )}
                      </div>
                    ) : (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => setShowTotpField(true)}
                        className="px-0 w-fit"
                      >
                        Add TOTP code (if prompted)
                      </Button>
                    )}

                    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                      <Button
                        type="submit"
                        className="w-full sm:w-auto"
                        disabled={
                          isLoading ||
                          isDevBypassLoading ||
                          !!lockoutMessage ||
                          !watchedFields.email?.trim() ||
                          !watchedFields.password?.trim()
                        }
                      >
                        {isLoading ? (
                          <>
                            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                            Signing in...
                          </>
                        ) : (
                          'Sign in'
                        )}
                      </Button>
                    </div>
                  </form>
                </section>

                {devBypassAllowed && (
                  <section className="rounded-lg border bg-card p-5 space-y-3">
                    <h2 className="text-base font-semibold">Development</h2>
                    <p className="text-sm text-muted-foreground">
                      Available in local or staging environments.
                    </p>
                    {devBypassError && (
                      <Alert variant="destructive">
                        <AlertDescription>{devBypassError}</AlertDescription>
                      </Alert>
                    )}
                    <Button
                      type="button"
                      variant="secondary"
                      onClick={handleDevBypass}
                      disabled={isDevBypassLoading || isLoading}
                      className="w-full sm:w-auto"
                    >
                      {isDevBypassLoading ? (
                        <>
                          <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                          Activating dev bypass...
                        </>
                      ) : (
                        'Use dev bypass'
                      )}
                    </Button>
                  </section>
                )}
              </>
            ) : (
              <section className="rounded-lg border bg-muted/40 p-5 text-sm text-muted-foreground">
                System is still starting. Sign in will appear once all critical components are healthy.
              </section>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}
