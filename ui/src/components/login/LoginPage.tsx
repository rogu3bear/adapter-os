/**
 * Login Page
 *
 * Main login page layout composing health panel, login form, and dev bypass.
 * Uses useAuthFlow hook for state management.
 */

import { useState, useCallback, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, RefreshCw } from 'lucide-react';
import { FetchErrorPanel } from '@/components/ui/fetch-error-panel';
import { SystemHealthPanel } from './SystemHealthPanel';
import { LoginCredentialsForm } from './LoginCredentialsForm';
import { DevBypassSection } from './DevBypassSection';
import type { UseAuthFlowReturn, LoginCredentials } from '@/hooks/auth/useAuthFlow';

interface LoginPageProps {
  authFlow: UseAuthFlowReturn;
}

/** Lock logo SVG component */
function LockLogo({ className = 'h-10 w-10 text-foreground' }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 64 64"
      fill="none"
      role="img"
      aria-hidden="true"
      className={className}
    >
      <rect
        x="14"
        y="26"
        width="36"
        height="28"
        rx="6"
        className="fill-primary/10 stroke-primary"
        strokeWidth="2"
      />
      <path
        d="M22 26v-6c0-6.627 5.373-12 12-12s12 5.373 12 12v6"
        className="stroke-primary"
        strokeWidth="2.5"
        strokeLinecap="round"
      />
      <circle
        cx="32"
        cy="40"
        r="4"
        className="fill-primary/20 stroke-primary"
        strokeWidth="2"
      />
      <path
        d="M32 44v5"
        className="stroke-primary"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function LoginPage({ authFlow }: LoginPageProps) {
  const {
    state,
    health,
    login,
    devBypass,
    retryConfig,
    clearError,
    showMfaField,
    canSubmit,
    devBypassAllowed,
    failedAttempts,
    maxAttempts,
  } = authFlow;

  const [localShowTotp, setLocalShowTotp] = useState(false);
  const [devBypassError, setDevBypassError] = useState<string | null>(null);

  // Determine if system is ready for login (includes config_error and loading_config since UI should show)
  const isSystemReady =
    state.status === 'ready' ||
    state.status === 'authenticating' ||
    state.status === 'mfa_required' ||
    state.status === 'locked_out' ||
    state.status === 'error' ||
    state.status === 'config_error' ||
    state.status === 'loading_config';

  const isConfigLoading = state.status === 'loading_config';
  const isConfigError = state.status === 'config_error';
  const isHealthError = state.status === 'health_error';
  const isAuthenticating = state.status === 'authenticating';

  // Get error message from state
  const errorMessage =
    state.status === 'error'
      ? state.error.message
      : state.status === 'config_error'
        ? state.error
        : null;

  // Get lockout message
  const lockoutMessage =
    state.status === 'locked_out'
      ? state.message
      : failedAttempts >= maxAttempts
        ? 'Too many failed attempts. Account temporarily locked—please try again later or contact an administrator.'
        : null;

  // Handle login with error clearing
  const handleLogin = useCallback(
    async (credentials: LoginCredentials) => {
      clearError();
      await login(credentials);
    },
    [login, clearError]
  );

  // Handle dev bypass with local error state
  const handleDevBypass = useCallback(async () => {
    setDevBypassError(null);
    try {
      await devBypass();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Dev bypass failed';
      setDevBypassError(message);
    }
  }, [devBypass]);

  // Show TOTP field when triggered locally or from MFA required state
  const shouldShowTotp = localShowTotp || showMfaField || state.status === 'mfa_required';

  const systemStatus = health.health?.status || health.systemHealth?.status || 'unknown';
  const isSystemHealthy = systemStatus === 'healthy';
  const statusTone =
    systemStatus === 'unhealthy' || systemStatus === 'issue'
      ? 'bg-destructive/10 text-destructive border-destructive/30'
      : isSystemHealthy
        ? 'bg-emerald-500/10 text-emerald-700 border-emerald-200'
        : 'bg-amber-500/15 text-amber-700 border-amber-200';
  const statusDot =
    systemStatus === 'unhealthy' || systemStatus === 'issue'
      ? 'bg-destructive'
      : isSystemHealthy
        ? 'bg-emerald-500'
        : 'bg-amber-500';

  const statusPanel = (
    <aside className="rounded-2xl border bg-card/85 backdrop-blur-md p-5 sm:p-6 shadow-lg space-y-4 h-full">
      <div className="flex items-center justify-between gap-3">
        <div className="space-y-1">
          <p className="text-xs uppercase tracking-[0.14em] text-muted-foreground">
            Control plane
          </p>
          <p className="text-lg font-semibold capitalize">{systemStatus}</p>
          <p className="text-xs text-muted-foreground">
            Live status while you sign in
          </p>
        </div>
        <span
          className={`inline-flex items-center gap-2 rounded-full border px-3 py-1.5 text-xs font-semibold capitalize ${statusTone}`}
        >
          <span className={`h-2 w-2 rounded-full ${statusDot} animate-pulse`} />
          {systemStatus}
        </span>
      </div>
      <SystemHealthPanel health={health} />
    </aside>
  );

  let primaryPanel: ReactNode;

  if (isHealthError) {
    primaryPanel = (
      <FetchErrorPanel
        title="Control plane unavailable"
        description="The UI can't reach the AdapterOS API. Start the backend and retry."
        error={health.healthError || 'Connection failed'}
        onRetry={health.refresh}
        className="h-full w-full shadow-xl border border-destructive/30 bg-card/95"
      />
    );
  } else if (!isSystemReady) {
    primaryPanel = (
      <section className="rounded-2xl border bg-card/95 p-6 sm:p-8 shadow-xl space-y-5 h-full">
        <div className="flex items-start gap-3">
          <div className="rounded-full bg-primary/10 p-2">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
          </div>
          <div className="space-y-1">
            <h2 className="text-2xl font-semibold">System starting</h2>
            <p className="text-sm text-muted-foreground">
              Sign in will appear once critical services report healthy.
            </p>
          </div>
        </div>
        <div className="rounded-lg border border-dashed border-border/70 bg-muted/40 px-4 py-3 text-sm text-muted-foreground">
          Health checks refresh automatically. You can also retry below if needed.
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={health.refresh}
            disabled={health.backendStatus === 'checking'}
            className="gap-2"
          >
            {health.backendStatus === 'checking' ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Checking
              </>
            ) : (
              <>
                <RefreshCw className="h-4 w-4" />
                Refresh status
              </>
            )}
          </Button>
        </div>
      </section>
    );
  } else if (isConfigLoading) {
    primaryPanel = (
      <section
        className="rounded-2xl border bg-card/95 p-6 sm:p-8 space-y-4 shadow-xl h-full"
        aria-live="polite"
      >
        <div className="flex items-center gap-3">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          <div>
            <h2 className="text-lg font-semibold">Preparing sign-in</h2>
            <p className="text-sm text-muted-foreground">Loading sign-in settings...</p>
          </div>
        </div>
        <div className="flex gap-3">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={retryConfig}
            disabled={isConfigLoading}
            className="w-full sm:w-auto"
          >
            Retry
          </Button>
          <Button type="button" variant="ghost" size="sm" disabled className="w-full sm:w-auto">
            Please wait
          </Button>
        </div>
      </section>
    );
  } else {
    primaryPanel = (
      <section className="rounded-2xl border bg-card/95 p-6 sm:p-8 space-y-6 shadow-xl h-full">
        <div>
          <h2 className="text-2xl font-semibold mb-1">Welcome back</h2>
          <p className="text-sm text-muted-foreground">Enter your credentials to continue</p>
        </div>

        {/* Config error alert with retry */}
        {state.status === 'config_error' && (
          <Alert variant="destructive">
            <AlertDescription className="flex items-center justify-between gap-3">
              <span>
                {state.error || 'Unable to load sign-in settings. You can still try to sign in.'}
              </span>
              <Button type="button" variant="outline" size="sm" onClick={retryConfig}>
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        )}

        <LoginCredentialsForm
          onSubmit={handleLogin}
          isSubmitting={isAuthenticating}
          disabled={!canSubmit}
          error={errorMessage}
          lockoutMessage={lockoutMessage}
          showTotpField={shouldShowTotp}
          onShowTotpField={() => setLocalShowTotp(true)}
        />

        {devBypassAllowed && (
          <div className="pt-1 border-t border-dashed border-border/60">
            <DevBypassSection
              onDevBypass={handleDevBypass}
              isLoading={isAuthenticating}
              disabled={isAuthenticating}
              error={devBypassError}
            />
          </div>
        )}
      </section>
    );
  }

  return (
    <main className="relative min-h-screen bg-gradient-to-br from-background via-background to-muted/20 flex items-center justify-center p-4 sm:p-6 lg:p-8">
      {/* Subtle grid pattern background */}
      <div className="pointer-events-none absolute inset-0 opacity-[0.02] dark:opacity-[0.05]">
        <div
          className="h-full w-full"
          style={{
            backgroundImage: `linear-gradient(to right, currentColor 1px, transparent 1px),
                             linear-gradient(to bottom, currentColor 1px, transparent 1px)`,
            backgroundSize: '32px 32px',
          }}
        />
      </div>

      <div className="relative w-full max-w-5xl">
        {/* Header - centered and prominent */}
        <header className="text-center mb-10">
          <div className="inline-flex items-center justify-center gap-3 mb-4 rounded-full border bg-card/70 px-4 py-2 text-xs font-medium text-muted-foreground shadow-sm">
            <span className="h-2 w-2 rounded-full bg-primary animate-pulse" aria-hidden="true" />
            Secure sign-in
          </div>
          <div className="flex items-center justify-center gap-3 mb-3">
            <LockLogo className="h-12 w-12 text-primary" />
            <h1 className="text-3xl sm:text-4xl font-bold tracking-tight bg-gradient-to-r from-foreground to-foreground/70 bg-clip-text text-transparent">
              AdapterOS
            </h1>
          </div>
          <p className="text-muted-foreground text-sm sm:text-base">
            Sign in to access the control plane
          </p>
        </header>

        <div className="grid gap-6 lg:grid-cols-[1.05fr_0.95fr] items-start">
          {primaryPanel}
          {statusPanel}
        </div>
      </div>
    </main>
  );
}
