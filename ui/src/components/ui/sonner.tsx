"use client";

import { Toaster as Sonner, ToasterProps } from 'sonner';
import { useTheme } from "@/providers/CoreProviders";
import { isE2EMode } from "@/utils/e2e";

const Toaster = ({ ...props }: ToasterProps) => {
  const { theme } = useTheme();
  const e2eMode = isE2EMode();
  const e2eTestIdOptions = e2eMode
    ? ({ 'data-testid': 'toast-default', testId: 'toast-default' } as Record<string, unknown>)
    : undefined;
  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="toaster group z-40"
      duration={e2eMode ? 100 : 5000}
      toastOptions={{
        classNames: {
          toast: 'backdrop-blur-xl bg-background/80 border border-border/50 shadow-lg',
          title: 'text-foreground font-medium',
          description: 'text-muted-foreground',
          actionButton: 'bg-primary text-primary-foreground',
          cancelButton: 'bg-muted text-muted-foreground',
          success: 'backdrop-blur-xl bg-green-500/10 border-green-500/30',
          error: 'backdrop-blur-xl bg-destructive/10 border-destructive/30',
          warning: 'backdrop-blur-xl bg-yellow-500/10 border-yellow-500/30',
          info: 'backdrop-blur-xl bg-blue-500/10 border-blue-500/30',
        },
        ...(e2eTestIdOptions as ToasterProps['toastOptions']),
      }}
      style={{
        "--normal-bg": "rgba(var(--popover), 0.8)",
        "--normal-text": "var(--popover-foreground)",
        "--normal-border": "rgba(var(--border), 0.5)",
      } as React.CSSProperties}
      {...props}
    />
  );
};

export { Toaster };
