import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';

console.log('Test minimal script loaded');

function TestMinimal() {
  console.log('TestMinimal component rendering');
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Minimal Test</h1>
      <p>React Router is working!</p>
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
      <Routes>
        <Route path="/" element={<TestMinimal />} />
      </Routes>
    </BrowserRouter>
  );
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
