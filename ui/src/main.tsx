import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { useState, useEffect, useRef } from "react";
import RootLayout from "@/layout/RootLayout";
import { useAuth } from "@/providers/CoreProviders";
import { AppProviders } from "@/providers/AppProviders";
import { LoginForm } from "@/components/LoginForm";
import { ErrorBoundary } from "@/components/shared/Feedback";
import { routes } from "@/config/routes";
import { RouteGuard } from "@/components/route-guard";
import { logger, toError } from "@/utils/logger";
import { captureException } from "@/stores/errorStore";
import { toast } from "sonner";

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

/** Extended error type for API errors with additional metadata */
interface ApiError extends Error {
  code?: string;
  status?: number;
}

/** Type guard to check if error has API error properties */
function isApiError(err: unknown): err is ApiError {
  return err instanceof Error && ('code' in err || 'status' in err);
}

function LoginRoute() {
  const { user, login, refreshUser } = useAuth();
  const navigate = useNavigate();
  const [loginError, setLoginError] = useState<string | null>(null);
  const isLoggingIn = useRef(false);

  // Handle navigation after user is authenticated
  useEffect(() => {
    if (user && isLoggingIn.current) {
      isLoggingIn.current = false;

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
            navigate("/owner", { replace: true });
            return;
          }
        } catch (error) {
          logger.warn('Failed to check/set first-run flag', { component: 'LoginRoute' });
        }
      }

      // Default navigation for non-admin or returning admin users
      navigate("/dashboard", { replace: true });
    }
  }, [user, navigate]);

  if (user && !isLoggingIn.current) {
    return <Navigate to="/dashboard" replace />;
  }

  // LoginForm handles its own full-screen layout
  return (
    <LoginForm
          onLogin={async (creds) => {
            try {
              setLoginError(null);
              isLoggingIn.current = true;
              // Convert to format expected by login (may need username for backend compatibility)
              await login({ username: creds.email.split('@')[0], email: creds.email, password: creds.password });
              // Navigation will happen in useEffect once user is set
            } catch (err) {
              isLoggingIn.current = false;
              let errorMessage = 'Login failed';
              if (err instanceof Error) {
                errorMessage = err.message;
                // Include error code if available for better diagnostics
                if (isApiError(err) && err.code) {
                  errorMessage = `${errorMessage} (${err.code})`;
                }
              }
              setLoginError(errorMessage);
              const apiErr = isApiError(err) ? err : undefined;
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

            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Route>
        </Routes>
      </AppProviders>
    </BrowserRouter>
  </ErrorBoundary>
);
