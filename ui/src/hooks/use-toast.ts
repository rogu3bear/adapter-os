import { useState, useCallback } from 'react';

export interface Toast {
  id: string;
  title?: string;
  description?: string;
  variant?: 'default' | 'destructive';
}

export interface ToastAction {
  toast: (props: Omit<Toast, 'id'>) => void;
  dismiss: (id: string) => void;
}

<<<<<<< HEAD
/**
 * Hook for managing toast notifications.
 *
 * Provides toast creation and dismissal functionality with auto-dismiss after 5 seconds.
 *
 * @returns Object with toast creation and dismissal functions
 */
=======
>>>>>>> integration-branch
export function useToast(): ToastAction {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const toast = useCallback((props: Omit<Toast, 'id'>) => {
    const id = Math.random().toString(36).substr(2, 9);
    const newToast: Toast = { id, ...props };
    
    setToasts((prev) => [...prev, newToast]);
    
    // Auto dismiss after 5 seconds
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 5000);
  }, []);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return { toast, dismiss };
}
