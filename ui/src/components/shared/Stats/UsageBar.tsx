import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";

const usageBarVariants = cva(
  "relative w-full",
  {
    variants: {
      size: {
        sm: "",
        md: "",
        lg: "",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

const usageBarTrackVariants = cva(
  "w-full overflow-hidden rounded-full bg-muted",
  {
    variants: {
      size: {
        sm: "h-1.5",
        md: "h-2",
        lg: "h-3",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

const usageFillVariants = cva(
  "h-full rounded-full transition-all duration-300 ease-out",
  {
    variants: {
      level: {
        low: "bg-emerald-500",
        medium: "bg-amber-500",
        high: "bg-orange-500",
        critical: "bg-red-500",
      },
    },
    defaultVariants: {
      level: "low",
    },
  }
);

export type UsageLevel = "low" | "medium" | "high" | "critical";

export interface UsageBarProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "children">,
    VariantProps<typeof usageBarVariants> {
  /** Current usage value */
  value: number;
  /** Maximum capacity */
  max: number;
  /** Resource label (e.g., "Memory", "CPU") */
  label?: string;
  /** Show usage values */
  showValues?: boolean;
  /** Show percentage */
  showPercentage?: boolean;
  /** Custom thresholds for level colors */
  thresholds?: {
    medium?: number;
    high?: number;
    critical?: number;
  };
  /** Format function for displaying values */
  formatValue?: (value: number) => string;
  /** Unit label (e.g., "GB", "MB", "%") */
  unit?: string;
}

/**
 * Resource usage bar with automatic color coding based on usage level.
 * Commonly used for memory, CPU, disk, and other resource metrics.
 */
export function UsageBar({
  className,
  size,
  value,
  max,
  label,
  showValues = true,
  showPercentage = true,
  thresholds = { medium: 50, high: 75, critical: 90 },
  formatValue,
  unit = "",
  ...props
}: UsageBarProps) {
  const percentage = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;

  const level: UsageLevel =
    percentage >= (thresholds.critical ?? 90) ? "critical" :
    percentage >= (thresholds.high ?? 75) ? "high" :
    percentage >= (thresholds.medium ?? 50) ? "medium" :
    "low";

  const displayValue = formatValue ? formatValue(value) : value.toFixed(1);
  const displayMax = formatValue ? formatValue(max) : max.toFixed(1);

  return (
    <div className={cn(usageBarVariants({ size }), className)} {...props}>
      {(label || showValues) && (
        <div className="mb-1.5 flex items-center justify-between text-sm">
          {label && (
            <span className="font-medium text-foreground">{label}</span>
          )}
          <div className="flex items-center gap-2 text-muted-foreground">
            {showValues && (
              <span>
                {displayValue}
                {unit && <span className="ml-0.5">{unit}</span>}
                {" / "}
                {displayMax}
                {unit && <span className="ml-0.5">{unit}</span>}
              </span>
            )}
            {showPercentage && (
              <span className={cn(
                "font-medium",
                level === "critical" && "text-red-600 dark:text-red-400",
                level === "high" && "text-orange-600 dark:text-orange-400",
                level === "medium" && "text-amber-600 dark:text-amber-400",
                level === "low" && "text-emerald-600 dark:text-emerald-400"
              )}>
                {percentage.toFixed(0)}%
              </span>
            )}
          </div>
        </div>
      )}
      <div
        className={cn(usageBarTrackVariants({ size }))}
        role="meter"
        aria-valuenow={value}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-label={`${label ?? "Usage"}: ${percentage.toFixed(0)}%`}
      >
        <div
          className={cn(usageFillVariants({ level }))}
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}

export interface MultiUsageBarProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, "children">,
    VariantProps<typeof usageBarVariants> {
  /** Label for the usage bar */
  label?: string;
  /** Array of usage segments */
  segments: Array<{
    value: number;
    label: string;
    color?: string;
  }>;
  /** Maximum capacity */
  max: number;
  /** Show legend */
  showLegend?: boolean;
  /** Format function for displaying values */
  formatValue?: (value: number) => string;
  /** Unit label */
  unit?: string;
}

/**
 * Multi-segment usage bar for showing breakdown of resource usage.
 */
export function MultiUsageBar({
  className,
  size,
  label,
  segments,
  max,
  showLegend = true,
  formatValue,
  unit = "",
  ...props
}: MultiUsageBarProps) {
  const total = segments.reduce((acc, seg) => acc + seg.value, 0);
  const percentage = max > 0 ? Math.min(100, (total / max) * 100) : 0;

  const colors = [
    "bg-blue-500",
    "bg-emerald-500",
    "bg-amber-500",
    "bg-purple-500",
    "bg-pink-500",
    "bg-cyan-500",
  ];

  return (
    <div className={cn(usageBarVariants({ size }), className)} {...props}>
      {label && (
        <div className="mb-1.5 flex items-center justify-between text-sm">
          <span className="font-medium text-foreground">{label}</span>
          <span className="text-muted-foreground">
            {formatValue ? formatValue(total) : total.toFixed(1)}
            {unit && <span className="ml-0.5">{unit}</span>}
            {" / "}
            {formatValue ? formatValue(max) : max.toFixed(1)}
            {unit && <span className="ml-0.5">{unit}</span>}
            {" ("}
            {percentage.toFixed(0)}%
            {")"}
          </span>
        </div>
      )}
      <div
        className={cn(usageBarTrackVariants({ size }), "flex")}
        role="meter"
        aria-valuenow={total}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-label={`${label ?? "Usage"}: ${percentage.toFixed(0)}%`}
      >
        {segments.map((segment, index) => {
          const segmentPercent = max > 0 ? (segment.value / max) * 100 : 0;
          return (
            <div
              key={index}
              className={cn(
                "h-full transition-all duration-300",
                segment.color ?? colors[index % colors.length],
                index === 0 && "rounded-l-full",
                index === segments.length - 1 && "rounded-r-full"
              )}
              style={{ width: `${segmentPercent}%` }}
              title={`${segment.label}: ${formatValue ? formatValue(segment.value) : segment.value.toFixed(1)}${unit}`}
            />
          );
        })}
      </div>
      {showLegend && (
        <div className="mt-2 flex flex-wrap gap-3 text-xs">
          {segments.map((segment, index) => (
            <div key={index} className="flex items-center gap-1.5">
              <div
                className={cn(
                  "h-2 w-2 rounded-full",
                  segment.color ?? colors[index % colors.length]
                )}
              />
              <span className="text-muted-foreground">
                {segment.label}:{" "}
                <span className="font-medium text-foreground">
                  {formatValue ? formatValue(segment.value) : segment.value.toFixed(1)}
                  {unit}
                </span>
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export {
  usageBarVariants,
  usageBarTrackVariants,
  usageFillVariants,
};
