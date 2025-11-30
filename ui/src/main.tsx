import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { useState, useEffect, useRef } from "react";
import RootLayout from "./layout/RootLayout";
import { useAuth } from "./providers/CoreProviders";
import { AppProviders } from "./providers/AppProviders";
import { LoginForm } from "./components/LoginForm";
import { ErrorBoundary } from "./components/shared/Feedback";
import { routes } from "./config/routes";
import { RouteGuard } from "./components/route-guard";
import { logger, toError } from "./utils/logger";


import "./index.css";

const FIRST_RUN_KEY = 'aos-first-login-completed';

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

  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-6">
      <div className="w-full max-w-md">
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
                if ((err as any).code) {
                  errorMessage = `${errorMessage} (${(err as any).code})`;
                }
              }
              setLoginError(errorMessage);
              logger.error('Login failed', {
                component: 'LoginRoute',
                operation: 'login',
                errorCode: (err as any)?.code,
                status: (err as any)?.status,
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
                if ((err as any).code) {
                  errorMessage = `${errorMessage} (${(err as any).code})`;
                }
              }
              setLoginError(errorMessage);
              logger.error('Dev bypass failed', {
                component: 'LoginRoute',
                operation: 'devBypass',
                errorCode: (err as any)?.code,
                status: (err as any)?.status,
              }, toError(err));
            }
          }}
          error={loginError}
        />
      </div>
    </div>
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
