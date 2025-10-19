
  import { createRoot } from "react-dom/client";
  import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
  import RootLayout from "./layout/RootLayout";
  import FeatureLayout from "./layout/FeatureLayout";
  import { LayoutProvider, useAuth, useTenant } from "./layout/LayoutProvider";
  import { Dashboard } from "./components/Dashboard";
  import { Telemetry } from "./components/Telemetry";
  import { AlertsPage } from "./components/AlertsPage";
  import { ReplayPanel } from "./components/ReplayPanel";
  import { Policies } from "./components/Policies";
  import { RoutingInspector } from "./components/RoutingInspector";
  import { LoginForm } from "./components/LoginForm";
  import { GoldenRuns } from "./components/GoldenRuns";
  import { Journeys } from "./components/Journeys";
  import "./index.css";

  function DashboardRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Dashboard" description="System overview, health monitoring, and alerts">
        <Dashboard user={user} selectedTenant={selectedTenant} onNavigate={() => {}} />
      </FeatureLayout>
    );
  }

  function TelemetryRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Telemetry" description="Stream viewer and bundle verification">
        <Telemetry user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function AlertsRoute() {
    const { selectedTenant } = useTenant();
    return (
      <FeatureLayout title="Alerts" description="Monitoring and alert rules">
        <AlertsPage selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function ReplayRoute() {
    const { selectedTenant } = useTenant();
    return (
      <FeatureLayout title="Replay" description="Deterministic verification">
        <ReplayPanel tenantId={selectedTenant} onSessionSelect={() => {}} />
      </FeatureLayout>
    );
  }

  function RoutingRoute() {
    return (
      <FeatureLayout title="Routing" description="History and debug tools">
        <div className="space-y-4">
          <RoutingInspector />
        </div>
      </FeatureLayout>
    );
  }

  function PoliciesRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Policies" description="Policy and audit views">
        <Policies user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function GoldenRoute() {
    const { user } = useAuth();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Golden" description="Baselines and summaries">
        <GoldenRuns />
      </FeatureLayout>
    );
  }

  function JourneysRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Journeys" description="User workflow journeys and visualizations">
        <Journeys user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function LoginRoute() {
    const { user, login } = useAuth();
    const navigate = useNavigate();
    if (user) return <Navigate to="/dashboard" replace />;
    return (
      <div className="min-h-screen bg-background flex items-center justify-center p-6">
        <div className="w-full max-w-md">
          <LoginForm
            onLogin={async (creds) => {
              await login(creds);
              navigate("/dashboard", { replace: true });
            }}
            error={null}
          />
        </div>
      </div>
    );
  }

  createRoot(document.getElementById("root")!).render(
    <BrowserRouter>
      <LayoutProvider>
        <Routes>
          <Route element={<RootLayout />}> 
            <Route index element={<Navigate to="/dashboard" replace />} />
            <Route path="/login" element={<LoginRoute />} />
            <Route path="/dashboard" element={<DashboardRoute />} />
            <Route path="/telemetry" element={<TelemetryRoute />} />
            <Route path="/alerts" element={<AlertsRoute />} />
            <Route path="/replay" element={<ReplayRoute />} />
            <Route path="/routing" element={<RoutingRoute />} />
            <Route path="/policies" element={<PoliciesRoute />} />
            <Route path="/golden" element={<GoldenRoute />} />
            <Route path="/journeys" element={<JourneysRoute />} />
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Route>
        </Routes>
      </LayoutProvider>
    </BrowserRouter>
  );
  
