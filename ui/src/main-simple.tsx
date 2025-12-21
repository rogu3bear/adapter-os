import React from "react";
import { createRoot } from "react-dom/client";

// Simple test without complex routing
function App() {
  return React.createElement('div', { 
    style: { padding: '20px', fontSize: '24px', color: 'green' } 
  }, 'Hello World - Simple Test');
}

// Wait for DOM to be ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    const rootElement = document.getElementById("root");
    if (rootElement) {
      const root = createRoot(rootElement);
      root.render(React.createElement(App));
    }
  });
} else {
  const rootElement = document.getElementById("root");
  if (rootElement) {
    const root = createRoot(rootElement);
    root.render(React.createElement(App));
  }
}
