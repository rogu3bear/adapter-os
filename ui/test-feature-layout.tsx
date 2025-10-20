import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { LayoutProvider } from './src/layout/LayoutProvider';
import RootLayout from './src/layout/RootLayout';
import FeatureLayout from './src/layout/FeatureLayout';

console.log('Test feature layout script loaded');

function TestDashboard() {
  console.log('TestDashboard component rendering');
  
  return (
    <FeatureLayout title="Test Dashboard" description="This is a test dashboard component.">
      <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
        <h1>Test Dashboard</h1>
        <p>This is a test dashboard component.</p>
      </div>
    </FeatureLayout>
  );
}

function TestLogin() {
  console.log('TestLogin component rendering');
  
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
            <Route path="/login" element={<TestLogin />} />
            <Route path="/dashboard" element={<TestDashboard />} />
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
