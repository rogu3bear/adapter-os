import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { LayoutProvider } from './src/layout/LayoutProvider';
import { Dashboard } from './src/components/Dashboard';

console.log('Test dashboard component script loaded');

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
          <Route path="/" element={<Dashboard user={{ id: 'test', role: 'admin', username: 'test' }} selectedTenant="default" onNavigate={() => {}} />} />
        </Routes>
      </LayoutProvider>
    </BrowserRouter>
  );
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
