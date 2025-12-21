import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';

console.log('Test layout script loaded');

function TestLayout() {
  console.log('TestLayout component rendering');
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Layout Test</h1>
      <p>Testing LayoutProvider...</p>
    </div>
  );
}

function TestLayoutProvider({ children }: { children: React.ReactNode }) {
  console.log('TestLayoutProvider rendering');
  return <div>{children}</div>;
}

console.log('About to create root and render');

const rootElement = document.getElementById('root');
if (rootElement) {
  console.log('Root element found:', rootElement);
  const root = createRoot(rootElement);
  console.log('Root created:', root);
  root.render(
    <BrowserRouter>
      <TestLayoutProvider>
        <Routes>
          <Route path="/" element={<TestLayout />} />
        </Routes>
      </TestLayoutProvider>
    </BrowserRouter>
  );
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
