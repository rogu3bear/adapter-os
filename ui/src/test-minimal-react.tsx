import React from "react";
import { createRoot } from "react-dom/client";
import { logger } from "./utils/logger";

logger.debug("Starting minimal React test", { component: "MinimalReactTest" });

function App() {
  logger.debug("Rendering App component", { component: "MinimalReactTest" });
  return React.createElement('div', { 
    style: { padding: '20px', fontSize: '24px', color: 'blue' } 
  }, 'Hello World - Minimal React Test');
}

logger.debug("Resolving root element", { component: "MinimalReactTest" });
const rootElement = document.getElementById("root");
logger.debug("Resolved root element", { component: "MinimalReactTest", hasRoot: Boolean(rootElement) });

if (!rootElement) {
  logger.error("Root element not found for minimal React test", { component: "MinimalReactTest" });
} else {
  logger.debug("Creating root", { component: "MinimalReactTest" });
  const root = createRoot(rootElement);
  logger.debug("Rendering app", { component: "MinimalReactTest" });
  root.render(React.createElement(App));
  logger.info("Minimal React test render complete", { component: "MinimalReactTest" });
}
