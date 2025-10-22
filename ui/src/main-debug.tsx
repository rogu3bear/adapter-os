import React from "react";
import { createRoot } from "react-dom/client";
import { logger } from "./utils/logger";

logger.debug("Starting debug bundle bootstrap", { component: "DebugBootstrap" });

function App() {
  logger.debug("Rendering App component", { component: "DebugBootstrap" });
  return React.createElement('div', { style: { padding: '20px', fontSize: '24px' } }, 'Hello World - Debug Test');
}

logger.debug("Resolving root element", { component: "DebugBootstrap" });
const rootElement = document.getElementById("root");
logger.debug("Resolved root element", { component: "DebugBootstrap", hasRoot: Boolean(rootElement) });

if (!rootElement) {
  logger.error("Root element not found for debug bootstrap", { component: "DebugBootstrap" });
} else {
  logger.debug("Creating root", { component: "DebugBootstrap" });
  const root = createRoot(rootElement);
  logger.debug("Rendering app root", { component: "DebugBootstrap" });
  root.render(React.createElement(App));
  logger.info("Debug app render complete", { component: "DebugBootstrap" });
}
