import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { useState } from "react";
import RootLayout from "./layout/RootLayout";
import { useAuth } from "./providers/CoreProviders";
import { AppProviders } from "./providers/AppProviders";
import { LoginForm } from "./components/LoginForm";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { routes } from "./config/routes";
import { RouteGuard } from "./components/route-guard";
import { logger, toError } from "./utils/logger";


import "./index.css";

function LoginRoute() {
  const { user, login, refreshUser } = useAuth();
  const navigate = useNavigate();
  const [loginError, setLoginError] = useState<string | null>(null);

  if (user) return <Navigate to="/dashboard" replace />;

  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-6">
      <div className="w-full max-w-md">
        <LoginForm
          onLogin={async (creds) => {
            try {
              setLoginError(null);
              // Convert to format expected by login (may need username for backend compatibility)
              await login({ username: creds.email.split('@')[0], email: creds.email, password: creds.password });
              navigate("/dashboard", { replace: true });
            } catch (err) {
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
              // Cookie is set synchronously by browser, no delay needed
              await refreshUser();
              navigate("/dashboard", { replace: true });
            } catch (err) {
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
            <Route path="/journeys" element={<Navigate to="/audit" replace />} />

            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Route>
        </Routes>
      </AppProviders>
    </BrowserRouter>
  </ErrorBoundary>
);
