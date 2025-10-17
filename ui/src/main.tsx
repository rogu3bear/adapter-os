
  import { createRoot } from "react-dom/client";
  import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
  import RootLayout from "./layout/RootLayout";
  import FeatureLayout from "./layout/FeatureLayout";
  import { LayoutProvider, useAuth, useTenant } from "./layout/LayoutProvider";
  import { Dashboard } from "./components/Dashboard";
  import { Telemetry } from "./components/Telemetry";
  import { AlertsPage } from "./components/AlertsPage";
  import { ReplayPanel } from "./components/ReplayPanel";
  import { Policies } from "./components/Policies";
  import "./index.css";

  function DashboardRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/dashboard" replace />;
    return (
      <FeatureLayout title="Dashboard" description="System overview, health monitoring, and alerts">
        <Dashboard user={user} selectedTenant={selectedTenant} onNavigate={() => {}} />
      </FeatureLayout>
    );
  }

  function TelemetryRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/dashboard" replace />;
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

  function PoliciesRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/dashboard" replace />;
    return (
      <FeatureLayout title="Policies" description="Policy and audit views">
        <Policies user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  createRoot(document.getElementById("root")!).render(
    <BrowserRouter>
      <LayoutProvider>
        <Routes>
          <Route element={<RootLayout />}> 
            <Route index element={<Navigate to="/dashboard" replace />} />
            <Route path="/dashboard" element={<DashboardRoute />} />
            <Route path="/telemetry" element={<TelemetryRoute />} />
            <Route path="/alerts" element={<AlertsRoute />} />
            <Route path="/replay" element={<ReplayRoute />} />
            <Route path="/policies" element={<PoliciesRoute />} />
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Route>
        </Routes>
      </LayoutProvider>
    </BrowserRouter>
  );
  