import React from 'react';
import { createRoot } from 'react-dom/client';

console.log('Test simple script loaded');

function TestSimple() {
  console.log('TestSimple component rendering');
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Simple Test</h1>
      <p>React is working!</p>
    </div>
  );
}

console.log('About to create root and render');

const rootElement = document.getElementById('root');
if (rootElement) {
  console.log('Root element found:', rootElement);
  const root = createRoot(rootElement);
  console.log('Root created:', root);
  root.render(<TestSimple />);
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
