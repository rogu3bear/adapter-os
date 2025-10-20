import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { LayoutProvider } from './src/layout/LayoutProvider';

console.log('Test layout provider script loaded');

function TestLayout() {
  console.log('TestLayout component rendering');
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Layout Provider Test</h1>
      <p>Testing LayoutProvider...</p>
    </div>
  );
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
          <Route path="/" element={<TestLayout />} />
        </Routes>
      </LayoutProvider>
    </BrowserRouter>
  );
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
