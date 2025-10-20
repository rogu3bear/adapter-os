import React from 'react';
import { createRoot } from 'react-dom/client';

function SimpleTest() {
  const [count, setCount] = React.useState(0);
  
  return (
    <div style={{ padding: '20px', fontFamily: 'Arial, sans-serif' }}>
      <h1>Simple React Test</h1>
      <p>Count: {count}</p>
      <button onClick={() => setCount(count + 1)}>
        Increment
      </button>
      <p>If this button works, React hydration is successful!</p>
    </div>
  );
}

const rootElement = document.getElementById('root');
if (rootElement) {
  const root = createRoot(rootElement);
  root.render(<SimpleTest />);
} else {
  console.error('Root element not found');
}
