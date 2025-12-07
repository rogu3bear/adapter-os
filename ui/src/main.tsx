import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { useState, useEffect, useRef } from "react";
import RootLayout from "@/layout/RootLayout";
import { SESSION_EXPIRED_FLAG_KEY, useAuth } from "@/providers/CoreProviders";
import type { AuthConfigResponse } from "@/api/auth-types";
import { AppProviders } from "@/providers/AppProviders";
import { LoginForm } from "@/components/LoginForm";
import { ErrorBoundary } from "@/components/shared/Feedback";
import { routes } from "@/config/routes";
import { RouteGuard } from "@/components/route-guard";
import { logger, toError } from "@/utils/logger";
import { captureException } from "@/stores/errorStore";
import { toast } from "sonner";
import { useToastQueue } from "@/components/toast/ToastProvider";
import NotFoundPage from "@/pages/NotFoundPage";

import "./index.css";

// Global error handlers - always enabled with different behavior for dev vs prod
window.addEventListener('error', (event) => {
  captureException(event.error || new Error(event.message), {
    component: 'global',
    operation: 'uncaught error',
    extra: {
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
    },
  });
});

window.addEventListener('unhandledrejection', (event) => {
  logger.error('Unhandled promise rejection', { component: 'global' }, event.reason);

  captureException(event.reason, {
    component: 'global',
    operation: 'unhandled promise rejection',
  });

  // Show user-friendly error in production
  if (import.meta.env.PROD) {
    toast.error('An unexpected error occurred');
  }
});

const FIRST_RUN_KEY = 'aos-first-login-completed';
const POST_LOGIN_REDIRECT_KEY = 'postLoginRedirect';

/** Extended error type for API errors with additional metadata */
interface ApiError extends Error {
  code?: string;
  status?: number;
  details?: Record<string, unknown>;
}

/** Type guard to check if error has API error properties */
function isApiError(err: unknown): err is ApiError {
  return err instanceof Error && ('code' in err || 'status' in err);
}

function friendlyLoginMessage(apiErr?: ApiError, fallback: string = 'Login failed'): string {
  if (!apiErr?.code) return fallback;
  switch (apiErr.code) {
    case 'ACCOUNT_LOCKED':
      return 'Your account is locked. Try again later or contact an administrator.';
    case 'ACCOUNT_DISABLED':
      return 'Your account is disabled. Contact an administrator.';
    case 'INVALID_CREDENTIALS':
      return 'Invalid email or password.';
    case 'TENANT_ACCESS_DENIED':
    case 'TENANT_ISOLATION_ERROR':
      return 'You have no role in this tenant. Request access from an admin.';
    case 'NO_TENANT_ACCESS':
      return 'You’re signed in but have no tenant access. Ask an admin to grant access.';
    case 'SESSION_EXPIRED':
      return 'Session expired—sign in again.';
    default:
      return fallback;
  }
}

function LoginRoute() {
  const { user, login, refreshUser } = useAuth();
  const navigate = useNavigate();
  const { enqueue } = useToastQueue();
  const [loginError, setLoginError] = useState<string | null>(() => {
    try {
      const expiredMessage = sessionStorage.getItem(SESSION_EXPIRED_FLAG_KEY);
      if (expiredMessage) {
        sessionStorage.removeItem(SESSION_EXPIRED_FLAG_KEY);
        return expiredMessage;
      }
    } catch {
      // ignore storage errors
    }
    return null;
  });
  const [failedAttempts, setFailedAttempts] = useState(0);
  const [authConfig, setAuthConfig] = useState<AuthConfigResponse | null>(null);
  const [mfaRequired, setMfaRequired] = useState(false);
  const isLoggingIn = useRef(false);
  const maxLoginAttempts = authConfig?.max_login_attempts ?? 5;
  const lockoutMessage = failedAttempts >= maxLoginAttempts
    ? 'Too many failed attempts. Account temporarily locked—please try again later or contact an administrator.'
    : null;

  useEffect(() => {
    if (user) {
      setFailedAttempts(0);
    }
  }, [user]);

  // Handle navigation after user is authenticated
  useEffect(() => {
    if (user && isLoggingIn.current) {
      isLoggingIn.current = false;

      // Restore deep link if stored
      let redirectPath: string | null = null;
      try {
        redirectPath = sessionStorage.getItem(POST_LOGIN_REDIRECT_KEY);
        if (redirectPath) {
          sessionStorage.removeItem(POST_LOGIN_REDIRECT_KEY);
        }
      } catch {
        // ignore storage errors
      }

      // Check if this is an admin user's first login
      if (user.role.toLowerCase() === 'admin') {
        try {
          const hasCompletedFirstRun = localStorage.getItem(FIRST_RUN_KEY);
          if (!hasCompletedFirstRun) {
            localStorage.setItem(FIRST_RUN_KEY, 'true');
            logger.info('First-run redirect for admin user', {
              component: 'LoginRoute',
              user_id: user.id,
            });
            navigate("/dashboard", { replace: true });
            return;
          }
        } catch (error) {
          logger.warn('Failed to check/set first-run flag', { component: 'LoginRoute' });
        }
      }

      // Default navigation for non-admin or returning admin users
      if (redirectPath) {
        navigate(redirectPath, { replace: true });
      } else {
        navigate("/dashboard", { replace: true });
      }
    }
  }, [user, navigate]);

  if (user && !isLoggingIn.current) {
    return <Navigate to="/dashboard" replace />;
  }

  // LoginForm handles its own full-screen layout
  return (
    <LoginForm
      lockoutMessage={lockoutMessage}
      onConfigLoaded={(config) => setAuthConfig(config)}
      onLogin={async (creds) => {
        try {
          setLoginError(null);
          isLoggingIn.current = true;
          // Convert to format expected by login (may need username for backend compatibility)
          const result = await login({
            username: creds.email.split('@')[0],
            email: creds.email,
            password: creds.password,
            totp_code: creds.totp,
          });
          if (result?.tenants && result.tenants.length > 1) {
            toast.info('Multiple tenants available. Use the tenant switcher after sign-in.');
          }
          // Navigation will happen in useEffect once user is set
        } catch (err) {
          isLoggingIn.current = false;
          const apiErr = isApiError(err) ? err : undefined;
          const mfaNeeded =
            apiErr?.code === 'MFA_REQUIRED' ||
            (apiErr?.details && typeof apiErr.details === 'object' && (apiErr.details as Record<string, unknown>).requires_mfa === true);
          setMfaRequired(mfaNeeded);
          let errorMessage = mfaNeeded
            ? 'Multi-factor authentication required. Enter your TOTP code.'
            : friendlyLoginMessage(apiErr);
          if (err instanceof Error) {
            errorMessage = friendlyLoginMessage(apiErr, err.message);
          }
          setLoginError(errorMessage);
          setFailedAttempts(prev => {
            const next = apiErr?.code === 'ACCOUNT_LOCKED' ? maxLoginAttempts : prev + 1;
            if (next >= maxLoginAttempts) {
              const lockout =
                lockoutMessage ??
                (apiErr?.code === 'ACCOUNT_LOCKED'
                  ? 'Your account is locked. Try again later or contact an administrator.'
                  : 'Too many failed attempts. Please wait and try again.');
              setLoginError(lockout);
              enqueue({ title: lockout, variant: 'error', persist: true });
            } else {
              enqueue({ title: errorMessage, variant: 'error', persist: true });
            }
            return next;
          });
          logger.error('Login failed', {
            component: 'LoginRoute',
            operation: 'login',
            errorCode: apiErr?.code,
            status: apiErr?.status,
          }, toError(err));
        }
      }}
          onDevBypass={async () => {
            try {
              setLoginError(null);
              isLoggingIn.current = true;
              // Cookie is set synchronously by browser, no delay needed
              await refreshUser();
              // Navigation will happen in useEffect once user is set
            } catch (err) {
              isLoggingIn.current = false;
              let errorMessage = 'Dev bypass failed';
              if (err instanceof Error) {
                errorMessage = err.message;
                // Include error code if available
                if (isApiError(err) && err.code) {
                  errorMessage = `${errorMessage} (${err.code})`;
                }
              }
              setLoginError(errorMessage);
              const apiErr = isApiError(err) ? err : undefined;
              logger.error('Dev bypass failed', {
                component: 'LoginRoute',
                operation: 'devBypass',
                errorCode: apiErr?.code,
                status: apiErr?.status,
              }, toError(err));
            }
          }}
      error={loginError}
      mfaRequired={mfaRequired}
    />
  );
}

  createRoot(document.getElementById("root")!).render(
  <ErrorBoundary>
    <BrowserRouter>
      <AppProviders>
        <Routes>
          <Route element={<RootLayout />}>
            <Route index element={<Navigate to="/dashboard" replace />} />
            <Route path="/login" element={<LoginRoute />} />

            {/* Generate routes from centralized config */}
            {routes.map((routeConfig) => (
              <Route
                key={routeConfig.path}
                path={routeConfig.path}
                element={<RouteGuard route={routeConfig} />}
              />
            ))}

            {/* Legacy redirects */}
            <Route path="/alerts" element={<Navigate to="/metrics" replace />} />
            <Route path="/journeys" element={<Navigate to="/security/audit" replace />} />

            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Routes>
      </AppProviders>
    </BrowserRouter>
  </ErrorBoundary>
);
