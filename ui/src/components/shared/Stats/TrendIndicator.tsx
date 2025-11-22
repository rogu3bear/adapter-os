import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";

const trendIndicatorVariants = cva(
  "inline-flex items-center gap-1 font-medium",
  {
    variants: {
      size: {
        xs: "text-xs",
        sm: "text-sm",
        md: "text-base",
        lg: "text-lg",
      },
      trend: {
        up: "text-emerald-600 dark:text-emerald-400",
        down: "text-red-600 dark:text-red-400",
        neutral: "text-muted-foreground",
      },
    },
    defaultVariants: {
      size: "sm",
      trend: "neutral",
    },
  }
);

export interface TrendIndicatorProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof trendIndicatorVariants> {
  /** The percentage change value */
  value: number;
  /** Show the percentage symbol */
  showPercent?: boolean;
  /** Show the arrow icon */
  showArrow?: boolean;
  /** Custom label for screen readers */
  label?: string;
  /** Format the value with fixed decimal places */
  decimals?: number;
}

/**
 * Displays a trend indicator with an arrow and percentage change.
 * Automatically determines direction based on positive/negative value.
 */
export function TrendIndicator({
  className,
  size,
  trend: trendProp,
  value,
  showPercent = true,
  showArrow = true,
  label,
  decimals = 1,
  ...props
}: TrendIndicatorProps) {
  const trend = trendProp ?? (value > 0 ? "up" : value < 0 ? "down" : "neutral");
  const displayValue = Math.abs(value).toFixed(decimals);
  const ariaLabel = label ?? `${value > 0 ? "Increased" : value < 0 ? "Decreased" : "Unchanged"} by ${displayValue}${showPercent ? "%" : ""}`;

  return (
    <span
      className={cn(trendIndicatorVariants({ size, trend }), className)}
      aria-label={ariaLabel}
      role="status"
      {...props}
    >
      {showArrow && (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          fill="currentColor"
          className={cn(
            "h-4 w-4 shrink-0",
            trend === "down" && "rotate-180",
            trend === "neutral" && "rotate-90"
          )}
          aria-hidden="true"
        >
          <path
            fillRule="evenodd"
            d="M8 3.5a.5.5 0 0 1 .5.5v7.793l2.146-2.147a.5.5 0 0 1 .708.708l-3 3a.5.5 0 0 1-.708 0l-3-3a.5.5 0 1 1 .708-.708L7.5 11.793V4a.5.5 0 0 1 .5-.5z"
            clipRule="evenodd"
            transform="rotate(180, 8, 8)"
          />
        </svg>
      )}
      <span>
        {value > 0 && "+"}
        {displayValue}
        {showPercent && "%"}
      </span>
    </span>
  );
}

export { trendIndicatorVariants };
