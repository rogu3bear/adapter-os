"use client";

import { useCallback, useState, useEffect, useRef } from "react";
import type { ToastVariant, ToastProps } from "./Toast";

export interface ToastOptions {
  title: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
  action?: React.ReactNode;
}

export interface UseToastReturn {
  toasts: ToastProps[];
  toast: (options: ToastOptions) => string;
  success: (title: string, description?: string) => string;
  error: (title: string, description?: string) => string;
  warning: (title: string, description?: string) => string;
  info: (title: string, description?: string) => string;
  dismiss: (id: string) => void;
  dismissAll: () => void;
}

const DEFAULT_DURATION = 5000;

let toastIdCounter = 0;
const generateId = () => `toast-${++toastIdCounter}-${Date.now()}`;

export function useToast(): UseToastReturn {
  const [toasts, setToasts] = useState<ToastProps[]>([]);
  const timersRef = useRef<Map<string, NodeJS.Timeout>>(new Map());

  // Cleanup timers on unmount
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      timers.forEach((timer) => clearTimeout(timer));
      timers.clear();
    };
  }, []);

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const dismissAll = useCallback(() => {
    timersRef.current.forEach((timer) => clearTimeout(timer));
    timersRef.current.clear();
    setToasts([]);
  }, []);

  const toast = useCallback(
    (options: ToastOptions): string => {
      const id = generateId();
      const duration = options.duration ?? DEFAULT_DURATION;

      const newToast: ToastProps = {
        id,
        title: options.title,
        description: options.description,
        variant: options.variant || "default",
        duration,
        action: options.action,
      };

      setToasts((prev) => [...prev, newToast]);

      // Auto-dismiss after duration (if duration > 0)
      if (duration > 0) {
        const timer = setTimeout(() => {
          dismiss(id);
        }, duration);
        timersRef.current.set(id, timer);
      }

      return id;
    },
    [dismiss]
  );

  const success = useCallback(
    (title: string, description?: string): string => {
      return toast({ title, description, variant: "success" });
    },
    [toast]
  );

  const error = useCallback(
    (title: string, description?: string): string => {
      return toast({ title, description, variant: "error", duration: 8000 });
    },
    [toast]
  );

  const warning = useCallback(
    (title: string, description?: string): string => {
      return toast({ title, description, variant: "warning", duration: 6000 });
    },
    [toast]
  );

  const info = useCallback(
    (title: string, description?: string): string => {
      return toast({ title, description, variant: "info" });
    },
    [toast]
  );

  return {
    toasts,
    toast,
    success,
    error,
    warning,
    info,
    dismiss,
    dismissAll,
  };
}

// Singleton toast manager for imperative usage
type ToastListener = (toasts: ToastProps[]) => void;

class ToastManager {
  private toasts: ToastProps[] = [];
  private listeners: Set<ToastListener> = new Set();
  private timers: Map<string, NodeJS.Timeout> = new Map();

  subscribe(listener: ToastListener) {
    this.listeners.add(listener);
    listener(this.toasts);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify() {
    this.listeners.forEach((listener) => listener([...this.toasts]));
  }

  toast(options: ToastOptions): string {
    const id = generateId();
    const duration = options.duration ?? DEFAULT_DURATION;

    const newToast: ToastProps = {
      id,
      title: options.title,
      description: options.description,
      variant: options.variant || "default",
      duration,
      action: options.action,
    };

    this.toasts = [...this.toasts, newToast];
    this.notify();

    if (duration > 0) {
      const timer = setTimeout(() => {
        this.dismiss(id);
      }, duration);
      this.timers.set(id, timer);
    }

    return id;
  }

  dismiss(id: string) {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }
    this.toasts = this.toasts.filter((t) => t.id !== id);
    this.notify();
  }

  dismissAll() {
    this.timers.forEach((timer) => clearTimeout(timer));
    this.timers.clear();
    this.toasts = [];
    this.notify();
  }

  success(title: string, description?: string): string {
    return this.toast({ title, description, variant: "success" });
  }

  error(title: string, description?: string): string {
    return this.toast({ title, description, variant: "error", duration: 8000 });
  }

  warning(title: string, description?: string): string {
    return this.toast({ title, description, variant: "warning", duration: 6000 });
  }

  info(title: string, description?: string): string {
    return this.toast({ title, description, variant: "info" });
  }
}

export const toastManager = new ToastManager();

// Hook to use the singleton toast manager
export function useToastManager(): UseToastReturn {
  const [toasts, setToasts] = useState<ToastProps[]>([]);

  useEffect(() => {
    return toastManager.subscribe(setToasts);
  }, []);

  return {
    toasts,
    toast: (options: ToastOptions) => toastManager.toast(options),
    success: (title: string, description?: string) => toastManager.success(title, description),
    error: (title: string, description?: string) => toastManager.error(title, description),
    warning: (title: string, description?: string) => toastManager.warning(title, description),
    info: (title: string, description?: string) => toastManager.info(title, description),
    dismiss: (id: string) => toastManager.dismiss(id),
    dismissAll: () => toastManager.dismissAll(),
  };
}

export default useToast;
