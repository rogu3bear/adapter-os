import React from "react";
import { createRoot } from "react-dom/client";

function App() {
  return React.createElement('div', null, 'Hello World - Minimal Test');
}

const root = createRoot(document.getElementById("root")!);
root.render(React.createElement(App));
