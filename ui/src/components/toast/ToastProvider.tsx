import { createContext, useCallback, useContext, useMemo, useRef, type ReactNode } from 'react';
import { toast } from 'sonner';
import { Toaster } from '@/components/ui/sonner';
import { isE2EMode } from '@/utils/e2e';

type ToastVariant = 'info' | 'success' | 'warning' | 'error';

export type ToastRequest = {
  title: string;
  description?: string;
  variant?: ToastVariant;
  persist?: boolean;
};

type ToastQueueContextValue = {
  enqueue: (toastRequest: ToastRequest) => string | number;
  dismiss: (id?: string | number) => void;
};

const ToastQueueContext = createContext<ToastQueueContextValue | undefined>(undefined);

export function ToastProvider({ children }: { children: ReactNode }) {
  const idCounter = useRef(0);

  const enqueue = useCallback((toastRequest: ToastRequest): string | number => {
    idCounter.current += 1;
    const toastId = idCounter.current;
    const variant = toastRequest.variant ?? 'info';
    const duration = toastRequest.persist ? Number.POSITIVE_INFINITY : undefined;
    const testId =
      isE2EMode() && variant === 'success'
        ? 'toast-success'
        : isE2EMode() && variant === 'error'
          ? 'toast-error'
          : isE2EMode()
            ? 'toast-default'
            : undefined;

    const payload: Record<string, unknown> = {
      description: toastRequest.description,
      duration,
      dismissible: true,
      ...(testId ? { 'data-testid': testId, testId } : {}),
    };

    switch (variant) {
      case 'success':
        toast.success(toastRequest.title, payload);
        break;
      case 'warning':
        toast.warning?.(toastRequest.title, payload) ?? toast(toastRequest.title, payload);
        break;
      case 'error':
        toast.error(toastRequest.title, payload);
        break;
      default:
        toast.info?.(toastRequest.title, payload) ?? toast(toastRequest.title, payload);
    }

    return toastId;
  }, []);

  const dismiss = useCallback((id?: string | number) => {
    toast.dismiss(id);
  }, []);

  const value = useMemo<ToastQueueContextValue>(() => ({
    enqueue,
    dismiss,
  }), [enqueue, dismiss]);

  return (
    <ToastQueueContext.Provider value={value}>
      {children}
      <Toaster position="top-right" className="z-[60]" />
    </ToastQueueContext.Provider>
  );
}

export function useToastQueue(): ToastQueueContextValue {
  const ctx = useContext(ToastQueueContext);
  if (!ctx) {
    throw new Error('useToastQueue must be used within ToastProvider');
  }
  return ctx;
}

