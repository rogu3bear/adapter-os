import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import RootLayout from './src/layout/RootLayout';
import FeatureLayout from './src/layout/FeatureLayout';
import { LayoutProvider, useTenant } from './src/layout/LayoutProvider';
import { Dashboard } from './src/components/Dashboard';

console.log('Test full app script loaded');

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
