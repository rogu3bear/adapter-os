import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/components/ui/utils";

const statusIndicatorVariants = cva(
  "inline-flex items-center gap-2",
  {
    variants: {
      size: {
        xs: "text-xs",
        sm: "text-sm",
        md: "text-base",
        lg: "text-lg",
      },
    },
    defaultVariants: {
      size: "sm",
    },
  }
);

const statusDotVariants = cva(
  "shrink-0 rounded-full",
  {
    variants: {
      status: {
        online: "bg-emerald-500",
        offline: "bg-gray-400 dark:bg-gray-600",
        warning: "bg-amber-500",
        error: "bg-red-500",
        pending: "bg-blue-500",
        idle: "bg-gray-300 dark:bg-gray-500",
      },
      size: {
        xs: "h-1.5 w-1.5",
        sm: "h-2 w-2",
        md: "h-2.5 w-2.5",
        lg: "h-3 w-3",
      },
      pulse: {
        true: "animate-pulse",
        false: "",
      },
    },
    defaultVariants: {
      status: "offline",
      size: "sm",
      pulse: false,
    },
  }
);

const statusBadgeVariants = cva(
  "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 font-medium",
  {
    variants: {
      status: {
        online: "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-400",
        offline: "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400",
        warning: "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-400",
        error: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-400",
        pending: "bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-400",
        idle: "bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-500",
      },
      size: {
        xs: "text-xs px-1.5 py-0.5",
        sm: "text-xs px-2 py-0.5",
        md: "text-sm px-2.5 py-1",
        lg: "text-base px-3 py-1",
      },
    },
    defaultVariants: {
      status: "offline",
      size: "sm",
    },
  }
);

export type StatusType = "online" | "offline" | "warning" | "error" | "pending" | "idle";

export interface StatusIndicatorProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof statusIndicatorVariants> {
  /** Current status */
  status: StatusType;
  /** Optional label to display */
  label?: string;
  /** Show pulsing animation for active states */
  pulse?: boolean;
  /** Display as dot only (no label) */
  dotOnly?: boolean;
}

/**
 * Displays a status indicator dot with optional label.
 * Supports multiple status states with semantic colors.
 */
export function StatusIndicator({
  className,
  size,
  status,
  label,
  pulse = false,
  dotOnly = false,
  ...props
}: StatusIndicatorProps) {
  const defaultLabels: Record<StatusType, string> = {
    online: "Online",
    offline: "Offline",
    warning: "Warning",
    error: "Error",
    pending: "Pending",
    idle: "Idle",
  };

  const displayLabel = label ?? defaultLabels[status];
  const shouldPulse = pulse || status === "pending";

  return (
    <span
      className={cn(statusIndicatorVariants({ size }), className)}
      role="status"
      aria-label={displayLabel}
      {...props}
    >
      <span
        className={cn(statusDotVariants({ status, size, pulse: shouldPulse }))}
        aria-hidden="true"
      />
      {!dotOnly && <span>{displayLabel}</span>}
    </span>
  );
}

export interface StatusBadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof statusBadgeVariants> {
  /** Current status */
  status: StatusType;
  /** Optional label to display */
  label?: string;
  /** Show dot indicator inside badge */
  showDot?: boolean;
}

/**
 * Displays a status badge with background color and optional dot.
 */
export function StatusBadge({
  className,
  size,
  status,
  label,
  showDot = true,
  ...props
}: StatusBadgeProps) {
  const defaultLabels: Record<StatusType, string> = {
    online: "Online",
    offline: "Offline",
    warning: "Warning",
    error: "Error",
    pending: "Pending",
    idle: "Idle",
  };

  const displayLabel = label ?? defaultLabels[status];

  return (
    <span
      className={cn(statusBadgeVariants({ status, size }), className)}
      role="status"
      aria-label={displayLabel}
      {...props}
    >
      {showDot && (
        <span
          className={cn(statusDotVariants({ status, size: "xs" }))}
          aria-hidden="true"
        />
      )}
      <span>{displayLabel}</span>
    </span>
  );
}

export { statusIndicatorVariants, statusDotVariants, statusBadgeVariants };
