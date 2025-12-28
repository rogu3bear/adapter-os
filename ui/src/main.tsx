import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { useEffect, useRef } from "react";
import RootLayout from "@/layout/RootLayout";
import { useAuth } from "@/providers/CoreProviders";
import { AppProviders } from "@/providers/AppProviders";
import { LoginPage } from "@/components/login";
import { ErrorBoundary } from "@/components/shared/Feedback";
import { routes } from "@/config/routes";
import { RouteGuard } from "@/components/route-guard";
import { logger } from "@/utils/logger";
import { captureException } from "@/stores/errorStore";
import { toast } from "sonner";
import { useToastQueue } from "@/components/toast/ToastProvider";
import NotFoundPage from "@/pages/NotFoundPage";
import { isDevBypassEnabled } from "@/auth/authBootstrap";
import { applyE2EToastGuards, applyE2EVisualGuards, applyE2EModeStyles, patchToastTestIds } from "@/utils/e2e";
import { getDemoEntryPath, isDemoEnvEnabled, isDemoMvpMode } from "@/config/demo";
import { useAuthFlow } from "@/hooks/auth";

import "./index.css";

applyE2EVisualGuards();
applyE2EToastGuards();

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

// Enable E2E stability features early
applyE2EModeStyles();
patchToastTestIds();

/**
 * LoginRoute - Thin wrapper using useAuthFlow state machine
 *
 * All login logic is now in useAuthFlow hook.
 * This component handles navigation and auto dev-bypass.
 */
export function LoginRoute() {
  const { user, sessionMode } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { enqueue } = useToastQueue();
  const authFlow = useAuthFlow();
  const devBypassFlagEnabled = isDevBypassEnabled();
  const devBypassRequested = new URLSearchParams(location.search).get('dev') === 'true';
  const devAutoTriggeredRef = useRef(false);
  const demoMode = isDemoMvpMode(sessionMode);

  // Show toast when dev bypass is requested but disabled
  useEffect(() => {
    if (devBypassRequested && !devBypassFlagEnabled) {
      toast.info('Dev bypass disabled in this environment.');
    }
  }, [devBypassRequested, devBypassFlagEnabled]);

  // Auto-trigger dev bypass when ?dev=true and allowed
  useEffect(() => {
    if (!devBypassRequested || !devBypassFlagEnabled || devAutoTriggeredRef.current) {
      return;
    }
    if (authFlow.devBypassAllowed) {
      devAutoTriggeredRef.current = true;
      authFlow.devBypass().catch(() => {
        // Error handled in useAuthFlow
      });
    }
  }, [authFlow.devBypassAllowed, devBypassFlagEnabled, devBypassRequested, authFlow]);

  // Handle navigation on successful auth
  useEffect(() => {
    if (authFlow.state.status === 'success') {
      const { redirectPath } = authFlow.state;
      logger.info('Login successful, navigating', {
        component: 'LoginRoute',
        redirectPath,
      });
      navigate(redirectPath, { replace: true });
    }
  }, [authFlow.state, navigate]);

  // Show toast for multi-tenant users (handled in useAuthFlow but we can add toast here)
  useEffect(() => {
    if (user && authFlow.state.status === 'success') {
      // Could add multi-tenant toast here if needed
    }
  }, [user, authFlow.state]);

  // Already authenticated - redirect immediately
  if (user && authFlow.state.status !== 'authenticating') {
    return <Navigate to={demoMode ? getDemoEntryPath() : "/dashboard"} replace />;
  }

  // Render LoginPage with auth flow state
  return <LoginPage authFlow={authFlow} />;
}

function IndexRedirect() {
  const demoMode = isDemoEnvEnabled();
  return <Navigate to={demoMode ? getDemoEntryPath() : "/dashboard"} replace />;
}

  createRoot(document.getElementById("root")!).render(
  <ErrorBoundary fullPage showDetails={import.meta.env.DEV} showDemoHints>
    <BrowserRouter>
      <AppProviders>
        <Routes>
          <Route element={<RootLayout />}>
            <Route index element={<IndexRedirect />} />
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
