import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { LoadingSpinner } from "./LoadingSpinner";

const overlayVariants = cva(
  "flex flex-col items-center justify-center gap-3",
  {
    variants: {
      variant: {
        /** Full screen overlay covering the viewport */
        fullscreen: "fixed inset-0 z-50 bg-background/80 backdrop-blur-sm",
        /** Container overlay covering a parent element */
        container: "absolute inset-0 z-10 bg-background/60 backdrop-blur-[2px] rounded-inherit",
        /** Inline overlay without positioning */
        inline: "relative bg-muted/50 rounded-lg p-8",
      },
    },
    defaultVariants: {
      variant: "container",
    },
  }
);

export interface LoadingOverlayProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof overlayVariants> {
  /** Loading message to display */
  message?: string;
  /** Spinner size */
  spinnerSize?: "xs" | "sm" | "md" | "lg" | "xl";
  /** Show/hide the overlay */
  show?: boolean;
}

/**
 * Loading overlay component that can cover the full screen or a container.
 * Includes a spinner and optional message.
 */
export function LoadingOverlay({
  className,
  variant,
  message,
  spinnerSize = "lg",
  show = true,
  children,
  ...props
}: LoadingOverlayProps) {
  if (!show) return null;

  return (
    <div
      className={cn(overlayVariants({ variant }), className)}
      role="status"
      aria-live="polite"
      aria-busy="true"
      {...props}
    >
      <LoadingSpinner size={spinnerSize} className="text-primary" />
      {message && (
        <p className="text-sm font-medium text-muted-foreground animate-pulse">
          {message}
        </p>
      )}
      {children}
    </div>
  );
}

export { overlayVariants };
