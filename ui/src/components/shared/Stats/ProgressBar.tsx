import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";

const progressBarVariants = cva(
  "relative w-full overflow-hidden rounded-full bg-muted",
  {
    variants: {
      size: {
        xs: "h-1",
        sm: "h-1.5",
        md: "h-2",
        lg: "h-3",
        xl: "h-4",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

const progressFillVariants = cva(
  "h-full transition-all duration-300 ease-out",
  {
    variants: {
      variant: {
        default: "bg-primary",
        success: "bg-emerald-500",
        warning: "bg-amber-500",
        error: "bg-red-500",
        info: "bg-blue-500",
      },
      animated: {
        true: "animate-pulse",
        false: "",
      },
    },
    defaultVariants: {
      variant: "default",
      animated: false,
    },
  }
);

export interface ProgressBarProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "children">,
    VariantProps<typeof progressBarVariants>,
    VariantProps<typeof progressFillVariants> {
  /** Progress value (0-100) */
  value: number;
  /** Maximum value (default: 100) */
  max?: number;
  /** Show percentage label */
  showLabel?: boolean;
  /** Label position */
  labelPosition?: "inside" | "outside" | "tooltip";
  /** Custom label format function */
  formatLabel?: (value: number, max: number) => string;
  /** Indeterminate state (shows animation) */
  indeterminate?: boolean;
}

/**
 * A progress bar component with multiple variants and label options.
 */
export function ProgressBar({
  className,
  size,
  variant,
  animated,
  value,
  max = 100,
  showLabel = false,
  labelPosition = "outside",
  formatLabel,
  indeterminate = false,
  ...props
}: ProgressBarProps) {
  const percentage = Math.min(100, Math.max(0, (value / max) * 100));
  const displayLabel = formatLabel
    ? formatLabel(value, max)
    : `${Math.round(percentage)}%`;

  // Auto-select variant based on percentage if not specified
  const autoVariant = variant ?? (
    percentage >= 90 ? "error" :
    percentage >= 75 ? "warning" :
    "default"
  );

  return (
    <div className={cn("w-full", className)} {...props}>
      {showLabel && labelPosition === "outside" && (
        <div className="mb-1 flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Progress</span>
          <span className="font-medium">{displayLabel}</span>
        </div>
      )}
      <div
        className={cn(progressBarVariants({ size }))}
        role="progressbar"
        aria-valuenow={value}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-label={displayLabel}
      >
        {indeterminate ? (
          <div
            className={cn(
              progressFillVariants({ variant: autoVariant }),
              "w-1/3 animate-[progress-indeterminate_1.5s_infinite_linear]"
            )}
            style={{
              animation: "progress-indeterminate 1.5s infinite linear",
            }}
          />
        ) : (
          <div
            className={cn(progressFillVariants({ variant: autoVariant, animated }))}
            style={{ width: `${percentage}%` }}
          >
            {showLabel && labelPosition === "inside" && size !== "xs" && size !== "sm" && (
              <span className="absolute inset-0 flex items-center justify-center text-xs font-medium text-white">
                {displayLabel}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export interface SegmentedProgressProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "children">,
    VariantProps<typeof progressBarVariants> {
  /** Array of segments with value and optional variant */
  segments: Array<{
    value: number;
    variant?: "default" | "success" | "warning" | "error" | "info";
    label?: string;
  }>;
  /** Maximum value (default: 100) */
  max?: number;
  /** Gap between segments */
  gap?: boolean;
}

/**
 * A segmented progress bar showing multiple values.
 */
export function SegmentedProgress({
  className,
  size,
  segments,
  max = 100,
  gap = false,
  ...props
}: SegmentedProgressProps) {
  const total = segments.reduce((acc, seg) => acc + seg.value, 0);

  return (
    <div
      className={cn(
        progressBarVariants({ size }),
        gap && "flex gap-0.5",
        className
      )}
      role="progressbar"
      aria-valuenow={total}
      aria-valuemin={0}
      aria-valuemax={max}
      {...props}
    >
      {segments.map((segment, index) => {
        const percentage = (segment.value / max) * 100;
        return (
          <div
            key={index}
            className={cn(
              progressFillVariants({ variant: segment.variant }),
              gap && "first:rounded-l-full last:rounded-r-full"
            )}
            style={{ width: `${percentage}%` }}
            title={segment.label}
          />
        );
      })}
    </div>
  );
}

export { progressBarVariants, progressFillVariants };
