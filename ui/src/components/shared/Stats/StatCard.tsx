import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { TrendIndicator, type TrendIndicatorProps } from "./TrendIndicator";
import { MetricChart } from "./MetricChart";

const statCardVariants = cva(
  "flex flex-col rounded-xl border bg-card text-card-foreground",
  {
    variants: {
      size: {
        sm: "p-4 gap-2",
        md: "p-5 gap-3",
        lg: "p-6 gap-4",
      },
      variant: {
        default: "",
        outline: "border-2",
        filled: "border-0 bg-muted/50",
        gradient: "border-0 bg-gradient-to-br from-card to-muted/50",
      },
    },
    defaultVariants: {
      size: "md",
      variant: "default",
    },
  }
);

const statValueVariants = cva(
  "font-bold tracking-tight",
  {
    variants: {
      size: {
        sm: "text-xl",
        md: "text-2xl",
        lg: "text-3xl",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

const statLabelVariants = cva(
  "text-muted-foreground",
  {
    variants: {
      size: {
        sm: "text-xs",
        md: "text-sm",
        lg: "text-base",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

export interface StatCardProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof statCardVariants> {
  /** The main metric label */
  label: string;
  /** The metric value to display */
  value: string | number;
  /** Optional description or sublabel */
  description?: string;
  /** Trend percentage change */
  trend?: number;
  /** Trend comparison period label */
  trendLabel?: string;
  /** Custom icon component */
  icon?: React.ReactNode;
  /** Sparkline data points */
  sparklineData?: number[];
  /** Action element (button, link, etc.) */
  action?: React.ReactNode;
  /** Format function for the value */
  formatValue?: (value: string | number) => string;
  /** Loading state */
  loading?: boolean;
}

/**
 * A card component for displaying a single metric with optional trend,
 * sparkline, and icon.
 */
export function StatCard({
  className,
  size,
  variant,
  label,
  value,
  description,
  trend,
  trendLabel,
  icon,
  sparklineData,
  action,
  formatValue,
  loading = false,
  ...props
}: StatCardProps) {
  const displayValue = formatValue ? formatValue(value) : value;

  if (loading) {
    return (
      <div className={cn(statCardVariants({ size, variant }), className)} {...props}>
        <div className="flex items-start justify-between">
          <div className="h-4 w-20 animate-pulse rounded bg-muted" />
          {icon && <div className="h-8 w-8 animate-pulse rounded bg-muted" />}
        </div>
        <div className="h-8 w-24 animate-pulse rounded bg-muted" />
        <div className="h-3 w-16 animate-pulse rounded bg-muted" />
      </div>
    );
  }

  return (
    <div className={cn(statCardVariants({ size, variant }), className)} {...props}>
      <div className="flex items-start justify-between">
        <span className={cn(statLabelVariants({ size }))}>{label}</span>
        {icon && (
          <div className="text-muted-foreground [&_svg]:h-5 [&_svg]:w-5">
            {icon}
          </div>
        )}
      </div>

      <div className="flex items-end gap-4">
        <div className="flex-1 space-y-1">
          <span className={cn(statValueVariants({ size }))}>{displayValue}</span>
          {description && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
        </div>
        {sparklineData && sparklineData.length > 1 && (
          <MetricChart
            data={sparklineData}
            size="sm"
            color={trend && trend > 0 ? "success" : trend && trend < 0 ? "error" : "default"}
          />
        )}
      </div>

      {(trend !== undefined || action) && (
        <div className="flex items-center justify-between">
          {trend !== undefined && (
            <div className="flex items-center gap-1.5">
              <TrendIndicator value={trend} size="sm" />
              {trendLabel && (
                <span className="text-xs text-muted-foreground">{trendLabel}</span>
              )}
            </div>
          )}
          {action && <div>{action}</div>}
        </div>
      )}
    </div>
  );
}

export interface CompactStatCardProps
  extends Omit<StatCardProps, "sparklineData" | "action" | "description"> {
  /** Show trend inline with value */
  inlineTrend?: boolean;
}

/**
 * A compact version of StatCard for dense layouts.
 */
export function CompactStatCard({
  className,
  size = "sm",
  label,
  value,
  trend,
  trendLabel,
  icon,
  formatValue,
  loading = false,
  inlineTrend = true,
  ...props
}: CompactStatCardProps) {
  const displayValue = formatValue ? formatValue(value) : value;

  if (loading) {
    return (
      <div
        className={cn(
          "flex items-center gap-3 rounded-lg border bg-card p-3",
          className
        )}
        {...props}
      >
        <div className="h-8 w-8 animate-pulse rounded bg-muted" />
        <div className="space-y-1">
          <div className="h-3 w-16 animate-pulse rounded bg-muted" />
          <div className="h-5 w-12 animate-pulse rounded bg-muted" />
        </div>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-lg border bg-card p-3",
        className
      )}
      {...props}
    >
      {icon && (
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-muted text-muted-foreground [&_svg]:h-4 [&_svg]:w-4">
          {icon}
        </div>
      )}
      <div className="min-w-0 flex-1">
        <p className="truncate text-xs text-muted-foreground">{label}</p>
        <div className="flex items-baseline gap-2">
          <span className="text-lg font-semibold">{displayValue}</span>
          {inlineTrend && trend !== undefined && (
            <TrendIndicator value={trend} size="xs" />
          )}
        </div>
      </div>
      {!inlineTrend && trend !== undefined && (
        <TrendIndicator value={trend} size="sm" />
      )}
    </div>
  );
}

export { statCardVariants, statValueVariants, statLabelVariants };
