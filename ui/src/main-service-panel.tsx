import React from 'react';
import { createRoot } from 'react-dom/client';
import ServicePanel from './components/ServicePanel';
import './index.css';
import { logger } from './utils/logger';

// Simple error boundary for the service panel
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error?: Error }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    logger.error('Service panel error', {
      component: 'ServicePanelErrorBoundary',
      errorInfo
    }, error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-red-50 flex items-center justify-center p-8">
          <div className="max-w-md w-full bg-white rounded-lg shadow-lg p-6">
            <h1 className="text-2xl font-bold text-red-600 mb-4">
              Service Panel Error
            </h1>
            <p className="text-gray-600 mb-4">
              Something went wrong with the service management panel.
            </p>
            <details className="text-sm text-gray-500">
              <summary className="cursor-pointer font-medium">
                Error Details
              </summary>
              <pre className="mt-2 p-2 bg-gray-100 rounded text-xs overflow-auto">
                {this.state.error?.message}
              </pre>
            </details>
            <button
              onClick={() => window.location.reload()}
              className="mt-4 px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700"
            >
              Reload Panel
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

const container = document.getElementById('root');
if (!container) {
  throw new Error('Root element not found');
}

const root = createRoot(container);
root.render(
  <ErrorBoundary>
    <ServicePanel />
  </ErrorBoundary>
);
