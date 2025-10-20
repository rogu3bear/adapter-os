import React from "react";
import { createRoot } from "react-dom/client";

console.log("Starting main-debug.tsx");

function App() {
  console.log("Rendering App component");
  return React.createElement('div', { style: { padding: '20px', fontSize: '24px' } }, 'Hello World - Debug Test');
}

console.log("Getting root element");
const rootElement = document.getElementById("root");
console.log("Root element:", rootElement);

if (!rootElement) {
  console.error("Root element not found!");
} else {
  console.log("Creating root");
  const root = createRoot(rootElement);
  console.log("Rendering app");
  root.render(React.createElement(App));
  console.log("Render complete");
}
