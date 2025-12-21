import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';
import { Toaster } from './ui/sonner';

interface Toast {
  id: string;
  title?: string;
  description?: string;
  type: 'success' | 'error' | 'warning' | 'info';
  duration?: number;
}

interface ToastContextType {
  addToast: (toast: Omit<Toast, 'id'>) => void;
  removeToast: (id: string) => void;
  clearAll: () => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context;
};

interface ToastProviderProps {
  children: ReactNode;
}

export const ToastProvider: React.FC<ToastProviderProps> = ({ children }) => {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((toast: Omit<Toast, 'id'>) => {
    const id = Date.now().toString() + Math.random().toString(36).substr(2, 9);
    const newToast: Toast = {
      ...toast,
      id,
      duration: toast.duration || 5000,
    };

    setToasts(prev => [...prev, newToast]);

    // Auto-remove toast after duration
    setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id));
    }, newToast.duration);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  const clearAll = useCallback(() => {
    setToasts([]);
  }, []);

  const contextValue: ToastContextType = {
    addToast,
    removeToast,
    clearAll,
  };

  return (
    <ToastContext.Provider value={contextValue}>
      {children}
      <Toaster />
      
      {/* Custom toast container for additional styling */}
      <div className="fixed top-4 right-4 z-50 space-y-2">
        {toasts.map(toast => (
          <ToastItem
            key={toast.id}
            toast={toast}
            onRemove={() => removeToast(toast.id)}
          />
        ))}
      </div>
    </ToastContext.Provider>
  );
};

interface ToastItemProps {
  toast: Toast;
  onRemove: () => void;
}

const ToastItem: React.FC<ToastItemProps> = ({ toast, onRemove }) => {
  const getToastStyles = (type: Toast['type']) => {
    switch (type) {
      case 'success':
        return 'bg-green-500/10 border-green-500/30 text-green-700 dark:text-green-300';
      case 'error':
        return 'bg-red-500/10 border-red-500/30 text-red-700 dark:text-red-300';
      case 'warning':
        return 'bg-yellow-500/10 border-yellow-500/30 text-yellow-700 dark:text-yellow-300';
      case 'info':
        return 'bg-blue-500/10 border-blue-500/30 text-blue-700 dark:text-blue-300';
      default:
        return 'bg-background/80 border-border/50 text-foreground';
    }
  };

  const getIcon = (type: Toast['type']) => {
    switch (type) {
      case 'success':
        return '✓';
      case 'error':
        return '✕';
      case 'warning':
        return '⚠';
      case 'info':
        return 'ℹ';
      default:
        return '•';
    }
  };

  return (
    <div
      className={`
        max-w-sm w-full border rounded-lg shadow-lg p-4
        backdrop-blur-xl
        ${getToastStyles(toast.type)}
        animate-in slide-in-from-right-full duration-300
      `}
    >
      <div className="flex items-start">
        <div className="flex-shrink-0">
          <span className="text-lg">{getIcon(toast.type)}</span>
        </div>
        <div className="ml-3 flex-1">
          {toast.title && (
            <div className="text-sm font-medium">{toast.title}</div>
          )}
          {toast.description && (
            <div className="text-sm mt-1 opacity-90">{toast.description}</div>
          )}
        </div>
        <div className="ml-4 flex-shrink-0">
          <button
            onClick={onRemove}
            className="opacity-60 hover:opacity-100 focus:outline-hidden transition-opacity"
          >
            <span className="sr-only">Close</span>
            <svg className="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
              <path
                fillRule="evenodd"
                d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
                clipRule="evenodd"
              />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
};

// Convenience hooks for different toast types
export const useToastNotifications = () => {
  const { addToast } = useToast();

  return {
    success: (title: string, description?: string) =>
      addToast({ title, description, type: 'success' }),
    error: (title: string, description?: string) =>
      addToast({ title, description, type: 'error' }),
    warning: (title: string, description?: string) =>
      addToast({ title, description, type: 'warning' }),
    info: (title: string, description?: string) =>
      addToast({ title, description, type: 'info' }),
  };
};
