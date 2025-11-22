"use client";

import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { X, CheckCircle, AlertCircle, AlertTriangle, Info } from "lucide-react";
import { cn } from "@/components/ui/utils";
import { Button } from "@/components/ui/button";

const alertBannerVariants = cva(
  "relative w-full rounded-lg border px-4 py-3 text-sm flex items-start gap-3",
  {
    variants: {
      variant: {
        default: "bg-background border-border text-foreground",
        success: "bg-green-50 border-green-200 text-green-900 dark:bg-green-950 dark:border-green-800 dark:text-green-100",
        error: "bg-destructive/10 border-destructive/50 text-destructive dark:bg-destructive/20 dark:border-destructive",
        warning: "bg-yellow-50 border-yellow-200 text-yellow-900 dark:bg-yellow-950 dark:border-yellow-800 dark:text-yellow-100",
        info: "bg-blue-50 border-blue-200 text-blue-900 dark:bg-blue-950 dark:border-blue-800 dark:text-blue-100",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

const iconMap = {
  default: Info,
  success: CheckCircle,
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
};

const iconColorMap = {
  default: "text-muted-foreground",
  success: "text-green-600 dark:text-green-400",
  error: "text-destructive",
  warning: "text-yellow-600 dark:text-yellow-400",
  info: "text-blue-600 dark:text-blue-400",
};

export type AlertBannerVariant = "default" | "success" | "error" | "warning" | "info";

export interface AlertBannerProps extends VariantProps<typeof alertBannerVariants> {
  title?: string;
  children: React.ReactNode;
  variant?: AlertBannerVariant;
  icon?: React.ReactNode;
  showIcon?: boolean;
  dismissible?: boolean;
  onDismiss?: () => void;
  action?: {
    label: string;
    onClick: () => void;
  };
  className?: string;
}

export function AlertBanner({
  title,
  children,
  variant = "default",
  icon,
  showIcon = true,
  dismissible = false,
  onDismiss,
  action,
  className,
}: AlertBannerProps) {
  const [dismissed, setDismissed] = React.useState(false);

  const handleDismiss = () => {
    setDismissed(true);
    onDismiss?.();
  };

  if (dismissed) return null;

  const Icon = iconMap[variant || "default"];
  const iconColor = iconColorMap[variant || "default"];

  return (
    <div
      data-slot="alert-banner"
      role="alert"
      className={cn(alertBannerVariants({ variant }), className)}
    >
      {showIcon && (
        <div className="shrink-0 mt-0.5">
          {icon || <Icon className={cn("h-5 w-5", iconColor)} />}
        </div>
      )}
      <div className="flex-1 min-w-0">
        {title && (
          <div className="font-semibold mb-1">{title}</div>
        )}
        <div className="text-sm opacity-90">{children}</div>
        {action && (
          <div className="mt-2">
            <Button
              variant="outline"
              size="sm"
              onClick={action.onClick}
              className="h-7 text-xs"
            >
              {action.label}
            </Button>
          </div>
        )}
      </div>
      {dismissible && (
        <button
          type="button"
          onClick={handleDismiss}
          className={cn(
            "shrink-0 rounded-md p-1 opacity-70 transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-offset-2",
            variant === "error" && "focus:ring-destructive",
            variant === "success" && "focus:ring-green-500",
            variant === "warning" && "focus:ring-yellow-500",
            variant === "info" && "focus:ring-blue-500"
          )}
          aria-label="Dismiss alert"
        >
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

// Convenience components for common alert types
export function SuccessAlert({
  children,
  title,
  ...props
}: Omit<AlertBannerProps, "variant">) {
  return (
    <AlertBanner variant="success" title={title} {...props}>
      {children}
    </AlertBanner>
  );
}

export function ErrorAlert({
  children,
  title,
  ...props
}: Omit<AlertBannerProps, "variant">) {
  return (
    <AlertBanner variant="error" title={title} {...props}>
      {children}
    </AlertBanner>
  );
}

export function WarningAlert({
  children,
  title,
  ...props
}: Omit<AlertBannerProps, "variant">) {
  return (
    <AlertBanner variant="warning" title={title} {...props}>
      {children}
    </AlertBanner>
  );
}

export function InfoAlert({
  children,
  title,
  ...props
}: Omit<AlertBannerProps, "variant">) {
  return (
    <AlertBanner variant="info" title={title} {...props}>
      {children}
    </AlertBanner>
  );
}

export { alertBannerVariants };
