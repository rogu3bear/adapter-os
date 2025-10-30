
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
  import { Promotion } from "./components/Promotion";
  import { InferencePlayground } from "./components/InferencePlayground";
  import { WorkflowWizard } from "./components/WorkflowWizard";
  import { TrainingPage } from "./components/TrainingPage";
  import { TestingPage } from "./components/TestingPage";
  import { AdaptersPage } from "./components/AdaptersPage";
  import { MonitoringPage } from "./components/MonitoringPage";
  import { AuditDashboard } from "./components/AuditDashboard";
  import { ITAdminDashboard } from "./components/ITAdminDashboard";
  import { UserReportsPage } from "./components/UserReportsPage";
  import { SingleFileAdapterTrainer } from "./components/SingleFileAdapterTrainer";
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

  function WorkflowWizardRoute() {
    const { user } = useAuth();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Getting Started" description="Onboarding and workflow wizard">
        <WorkflowWizard />
      </FeatureLayout>
    );
  }

  function TrainingRoute() {
    const { user } = useAuth();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Training" description="Manage and launch training jobs">
        <TrainingPage />
      </FeatureLayout>
    );
  }

  function TestingRoute() {
    const { user } = useAuth();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Testing" description="Compare against golden baselines">
        <TestingPage />
      </FeatureLayout>
    );
  }

  function PromotionRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Promotion" description="Promotion gates and approvals">
        <Promotion user={user} selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function AdaptersRoute() {
    return (
      <FeatureLayout title="Adapters" description="Deploy and manage adapters">
        <AdaptersPage />
      </FeatureLayout>
    );
  }

  function InferenceRoute() {
    const { selectedTenant } = useTenant();
    return (
      <FeatureLayout title="Inference" description="Playground for inference">
        <InferencePlayground selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function MonitoringRoute() {
    return (
      <FeatureLayout title="Monitoring" description="System health and metrics">
        <MonitoringPage />
      </FeatureLayout>
    );
  }

  function AuditRoute() {
    const { selectedTenant } = useTenant();
    return (
      <FeatureLayout title="Audit" description="Audit trails and compliance">
        <AuditDashboard selectedTenant={selectedTenant} />
      </FeatureLayout>
    );
  }

  function ITAdminRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    if (user.role !== 'Admin') return <Navigate to="/dashboard" replace />;
    return (
      <FeatureLayout title="IT Admin" description="System administration and management">
        <ITAdminDashboard tenantId={selectedTenant} />
      </FeatureLayout>
    );
  }

  function UserReportsRoute() {
    const { user } = useAuth();
    const { selectedTenant } = useTenant();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Reports" description="Activity reports and metrics">
        <UserReportsPage tenantId={selectedTenant} />
      </FeatureLayout>
    );
  }

  function SingleFileTrainerRoute() {
    const { user } = useAuth();
    if (!user) return <Navigate to="/login" replace />;
    return (
      <FeatureLayout title="Single-File Trainer" description="Train adapters from a single file">
        <SingleFileAdapterTrainer />
      </FeatureLayout>
    );
  }

  createRoot(document.getElementById("root")!).render(
    <BrowserRouter>
      <LayoutProvider>
        <Routes>
          <Route element={<RootLayout />}> 
            <Route index element={<Navigate to="/workflow" replace />} />
            <Route path="/login" element={<LoginRoute />} />
            
            {/* Workflow & Dashboard */}
            <Route path="/workflow" element={<WorkflowWizardRoute />} />
            <Route path="/dashboard" element={<DashboardRoute />} />
            
            {/* ML Lifecycle */}
            <Route path="/training" element={<TrainingRoute />} />
            <Route path="/testing" element={<TestingRoute />} />
            <Route path="/golden" element={<GoldenRoute />} />
            <Route path="/promotion" element={<PromotionRoute />} />
            <Route path="/adapters" element={<AdaptersRoute />} />
            
            {/* Operations */}
            <Route path="/routing" element={<RoutingRoute />} />
            <Route path="/inference" element={<InferenceRoute />} />
            <Route path="/monitoring" element={<MonitoringRoute />} />
            
            {/* Security & Compliance */}
            <Route path="/policies" element={<PoliciesRoute />} />
            <Route path="/telemetry" element={<TelemetryRoute />} />
            <Route path="/replay" element={<ReplayRoute />} />
            <Route path="/audit" element={<AuditRoute />} />
            
            {/* Admin & Reporting */}
            <Route path="/admin" element={<ITAdminRoute />} />
            <Route path="/reports" element={<UserReportsRoute />} />
            <Route path="/trainer" element={<SingleFileTrainerRoute />} />
            
            {/* Legacy redirects */}
            <Route path="/alerts" element={<Navigate to="/monitoring" replace />} />
            <Route path="/journeys" element={<Navigate to="/audit" replace />} />
            
            <Route path="*" element={<Navigate to="/workflow" replace />} />
          </Route>
        </Routes>
      </LayoutProvider>
    </BrowserRouter>
  );
  
