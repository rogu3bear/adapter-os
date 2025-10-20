import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import RootLayout from './src/layout/RootLayout';
import FeatureLayout from './src/layout/FeatureLayout';
import { LayoutProvider, useTenant } from './src/layout/LayoutProvider';
import { Dashboard } from './src/components/Dashboard';
import { Telemetry } from './src/components/Telemetry';
import { AlertsPage } from './src/components/AlertsPage';
import { ReplayPanel } from './src/components/ReplayPanel';
import { Policies } from './src/components/Policies';
import { RoutingInspector } from './src/components/RoutingInspector';
import { LoginForm } from './src/components/LoginForm';
import { GoldenRuns } from './src/components/GoldenRuns';
import { Journeys } from './src/components/Journeys';
import './src/index.css';

console.log('Test final script loaded');

// Mock user and tenant for bypassing auth
const mockUser = { id: "mock-admin", role: "admin", username: "admin" };
const mockTenant = "default";

function DashboardRoute() {
  const { selectedTenant } = useTenant();
  // Temporarily bypass auth
  return (
    <FeatureLayout title="Dashboard" description="System overview, health monitoring, and alerts">
      <Dashboard user={mockUser} selectedTenant={mockTenant} onNavigate={() => {}} />
    </FeatureLayout>
  );
}

function TelemetryRoute() {
  const { selectedTenant } = useTenant();
  // Temporarily bypass auth
  return (
    <FeatureLayout title="Telemetry" description="Stream viewer and bundle verification">
      <Telemetry user={mockUser} selectedTenant={mockTenant} />
    </FeatureLayout>
  );
}

function AlertsRoute() {
  const { selectedTenant } = useTenant();
  return (
    <FeatureLayout title="Alerts" description="Monitoring and alert rules">
      <AlertsPage selectedTenant={mockTenant} />
    </FeatureLayout>
  );
}

function ReplayRoute() {
  const { selectedTenant } = useTenant();
  return (
    <FeatureLayout title="Replay" description="Deterministic verification">
      <ReplayPanel tenantId={mockTenant} onSessionSelect={() => {}} />
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
  const { selectedTenant } = useTenant();
  // Temporarily bypass auth
  return (
    <FeatureLayout title="Policies" description="Policy and audit views">
      <Policies user={mockUser} selectedTenant={mockTenant} />
    </FeatureLayout>
  );
}

function GoldenRoute() {
  // Temporarily bypass auth
  return (
    <FeatureLayout title="Golden" description="Baselines and summaries">
      <GoldenRuns />
    </FeatureLayout>
  );
}

function JourneysRoute() {
  const { selectedTenant } = useTenant();
  // Temporarily bypass auth
  return (
    <FeatureLayout title="Journeys" description="User workflow journeys and visualizations">
      <Journeys user={mockUser} selectedTenant={mockTenant} />
    </FeatureLayout>
  );
}

function LoginRoute() {
  // Temporarily bypass: always redirect to dashboard
  return <Navigate to="/dashboard" replace />;
}

console.log('About to create root and render');

const rootElement = document.getElementById('root');
if (rootElement) {
  console.log('Root element found:', rootElement);
  const root = createRoot(rootElement);
  console.log('Root created:', root);
  root.render(
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
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
