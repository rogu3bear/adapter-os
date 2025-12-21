import React from 'react';
import { createRoot } from 'react-dom/client';

console.log('Minimal test script loaded');

function MinimalTest() {
  console.log('MinimalTest component rendering');
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Minimal React Test</h1>
      <p>If you can see this, React is working!</p>
      <button onClick={() => alert('Button clicked!')}>
        Test Button
      </button>
    </div>
  );
}

console.log('About to create root and render');

const rootElement = document.getElementById('root');
if (rootElement) {
  console.log('Root element found:', rootElement);
  const root = createRoot(rootElement);
  console.log('Root created:', root);
  root.render(<MinimalTest />);
  console.log('Component rendered');
} else {
  console.error('Root element not found');
}
