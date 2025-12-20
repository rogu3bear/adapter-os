/**
 * Login Page
 *
 * Main login page layout composing health panel, login form, and dev bypass.
 * Uses useAuthFlow hook for state management.
 */

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';
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
  const [showHealthPanel, setShowHealthPanel] = useState(false);

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

      <div className="relative w-full max-w-md">
        {/* Header - centered and prominent */}
        <header className="text-center mb-8">
          <div className="flex items-center justify-center gap-3 mb-4">
            <LockLogo className="h-12 w-12 text-primary" />
            <h1 className="text-3xl sm:text-4xl font-bold tracking-tight bg-gradient-to-r from-foreground to-foreground/70 bg-clip-text text-transparent">
              AdapterOS
            </h1>
          </div>
          <p className="text-muted-foreground text-sm sm:text-base">
            Sign in to access the control plane
          </p>
        </header>

        {/* Content based on state */}
        {isSystemReady ? (
          <>
            {/* Config loading state */}
            {isConfigLoading && (
              <section
                className="rounded-xl border bg-card p-6 space-y-4 shadow-sm"
                aria-live="polite"
              >
                <div className="flex items-center gap-3">
                  <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
                  <div>
                    <h2 className="text-lg font-semibold">Preparing sign-in</h2>
                    <p className="text-sm text-muted-foreground">
                      Loading sign-in settings...
                    </p>
                  </div>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={retryConfig}
                  disabled={isConfigLoading}
                  className="w-full"
                >
                  Retry
                </Button>
              </section>
            )}

            {/* Main sign-in form */}
            {!isConfigLoading && (
              <>
                <section className="rounded-xl border bg-card p-6 sm:p-8 space-y-6 shadow-lg">
                  <div>
                    <h2 className="text-2xl font-semibold mb-1">Welcome back</h2>
                    <p className="text-sm text-muted-foreground">
                      Enter your credentials to continue
                    </p>
                  </div>

                  {/* Config error alert with retry */}
                  {state.status === 'config_error' && (
                    <Alert variant="destructive">
                      <AlertDescription className="flex items-center justify-between gap-3">
                        <span>
                          {state.error ||
                            'Unable to load sign-in settings. You can still try to sign in.'}
                        </span>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onClick={retryConfig}
                        >
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
                </section>

                {/* Dev bypass section */}
                {devBypassAllowed && (
                  <DevBypassSection
                    onDevBypass={handleDevBypass}
                    isLoading={isAuthenticating}
                    disabled={isAuthenticating}
                    error={devBypassError}
                  />
                )}

                {/* Collapsible health status */}
                <div className="mt-6">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowHealthPanel(!showHealthPanel)}
                    className="w-full text-muted-foreground hover:text-foreground"
                  >
                    {showHealthPanel ? 'Hide' : 'Show'} system status
                    {!isSystemHealthy && (
                      <span className="ml-2 h-2 w-2 rounded-full bg-amber-500" />
                    )}
                  </Button>
                  {showHealthPanel && (
                    <div className="mt-4 animate-in fade-in slide-in-from-top-2">
                      <SystemHealthPanel health={health} />
                    </div>
                  )}
                </div>
              </>
            )}
          </>
        ) : isHealthError ? (
          <FetchErrorPanel
            title="Control plane unavailable"
            description="The UI can't reach the AdapterOS API. Start the backend and retry."
            error={health.healthError || 'Connection failed'}
            onRetry={health.refresh}
          />
        ) : (
          /* Loading fallback with consistent layout dimensions */
          <section className="rounded-xl border bg-card p-6 sm:p-8 space-y-6 shadow-lg min-h-[360px] flex flex-col">
            <div className="text-center flex-shrink-0">
              <div className="flex items-center justify-center gap-2 mb-2">
                <Loader2 className="h-5 w-5 animate-spin text-primary" />
                <h2 className="text-2xl font-semibold">System starting</h2>
              </div>
              <p className="text-sm text-muted-foreground">
                Sign in will appear once all critical components are healthy.
              </p>
            </div>
            <div className="flex-1 flex flex-col justify-start">
              <SystemHealthPanel health={health} />
            </div>
          </section>
        )}
      </div>
    </main>
  );
}
