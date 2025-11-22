import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";
import { StatCard, CompactStatCard, type StatCardProps, type CompactStatCardProps } from "./StatCard";

const statGridVariants = cva(
  "grid gap-4",
  {
    variants: {
      columns: {
        1: "grid-cols-1",
        2: "grid-cols-1 sm:grid-cols-2",
        3: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3",
        4: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-4",
        5: "grid-cols-2 sm:grid-cols-3 lg:grid-cols-5",
        6: "grid-cols-2 sm:grid-cols-3 lg:grid-cols-6",
        auto: "grid-cols-[repeat(auto-fit,minmax(200px,1fr))]",
      },
      gap: {
        sm: "gap-2",
        md: "gap-4",
        lg: "gap-6",
      },
    },
    defaultVariants: {
      columns: 4,
      gap: "md",
    },
  }
);

export interface StatItem extends Omit<StatCardProps, "size" | "variant"> {
  /** Unique identifier for the stat */
  id: string;
}

export interface StatGridProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof statGridVariants> {
  /** Array of stat items to display */
  stats: StatItem[];
  /** Card size */
  cardSize?: StatCardProps["size"];
  /** Card variant */
  cardVariant?: StatCardProps["variant"];
  /** Use compact cards */
  compact?: boolean;
  /** Loading state */
  loading?: boolean;
  /** Number of skeleton cards to show when loading */
  skeletonCount?: number;
}

/**
 * Grid layout component for displaying multiple StatCards.
 */
export function StatGrid({
  className,
  columns,
  gap,
  stats,
  cardSize,
  cardVariant,
  compact = false,
  loading = false,
  skeletonCount = 4,
  ...props
}: StatGridProps) {
  if (loading) {
    return (
      <div className={cn(statGridVariants({ columns, gap }), className)} {...props}>
        {Array.from({ length: skeletonCount }).map((_, index) =>
          compact ? (
            <CompactStatCard
              key={index}
              label=""
              value=""
              loading
            />
          ) : (
            <StatCard
              key={index}
              label=""
              value=""
              size={cardSize}
              variant={cardVariant}
              loading
            />
          )
        )}
      </div>
    );
  }

  return (
    <div className={cn(statGridVariants({ columns, gap }), className)} {...props}>
      {stats.map((stat) =>
        compact ? (
          <CompactStatCard
            key={stat.id}
            label={stat.label}
            value={stat.value}
            trend={stat.trend}
            trendLabel={stat.trendLabel}
            icon={stat.icon}
            formatValue={stat.formatValue}
          />
        ) : (
          <StatCard
            key={stat.id}
            label={stat.label}
            value={stat.value}
            description={stat.description}
            trend={stat.trend}
            trendLabel={stat.trendLabel}
            icon={stat.icon}
            sparklineData={stat.sparklineData}
            action={stat.action}
            formatValue={stat.formatValue}
            size={cardSize}
            variant={cardVariant}
          />
        )
      )}
    </div>
  );
}

export interface StatRowProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** Array of stat items to display */
  stats: StatItem[];
  /** Card size */
  cardSize?: StatCardProps["size"];
  /** Use dividers between stats */
  dividers?: boolean;
  /** Loading state */
  loading?: boolean;
}

/**
 * Horizontal row layout for displaying stats inline.
 */
export function StatRow({
  className,
  stats,
  cardSize = "sm",
  dividers = true,
  loading = false,
  ...props
}: StatRowProps) {
  if (loading) {
    return (
      <div
        className={cn(
          "flex items-center overflow-x-auto",
          dividers && "divide-x",
          className
        )}
        {...props}
      >
        {Array.from({ length: 4 }).map((_, index) => (
          <div key={index} className="flex-shrink-0 px-4 py-2 first:pl-0 last:pr-0">
            <div className="space-y-1">
              <div className="h-3 w-16 animate-pulse rounded bg-muted" />
              <div className="h-6 w-12 animate-pulse rounded bg-muted" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex items-center overflow-x-auto",
        dividers && "divide-x",
        className
      )}
      {...props}
    >
      {stats.map((stat) => (
        <div
          key={stat.id}
          className="flex-shrink-0 px-4 py-2 first:pl-0 last:pr-0"
        >
          <p className="text-xs text-muted-foreground">{stat.label}</p>
          <div className="flex items-baseline gap-2">
            <span className="text-lg font-semibold">
              {stat.formatValue ? stat.formatValue(stat.value) : stat.value}
            </span>
            {stat.trend !== undefined && (
              <span
                className={cn(
                  "text-xs font-medium",
                  stat.trend > 0 && "text-emerald-600 dark:text-emerald-400",
                  stat.trend < 0 && "text-red-600 dark:text-red-400",
                  stat.trend === 0 && "text-muted-foreground"
                )}
              >
                {stat.trend > 0 && "+"}
                {stat.trend.toFixed(1)}%
              </span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

export interface StatSummaryProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** Main stat */
  primary: StatItem;
  /** Secondary stats */
  secondary?: StatItem[];
  /** Loading state */
  loading?: boolean;
}

/**
 * Summary layout with a prominent primary stat and smaller secondary stats.
 */
export function StatSummary({
  className,
  primary,
  secondary = [],
  loading = false,
  ...props
}: StatSummaryProps) {
  if (loading) {
    return (
      <div className={cn("space-y-4", className)} {...props}>
        <div className="space-y-2">
          <div className="h-4 w-24 animate-pulse rounded bg-muted" />
          <div className="h-10 w-32 animate-pulse rounded bg-muted" />
        </div>
        <div className="flex gap-6">
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className="space-y-1">
              <div className="h-3 w-16 animate-pulse rounded bg-muted" />
              <div className="h-5 w-12 animate-pulse rounded bg-muted" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className={cn("space-y-4", className)} {...props}>
      <div>
        <p className="text-sm text-muted-foreground">{primary.label}</p>
        <div className="flex items-baseline gap-3">
          <span className="text-4xl font-bold">
            {primary.formatValue
              ? primary.formatValue(primary.value)
              : primary.value}
          </span>
          {primary.trend !== undefined && (
            <span
              className={cn(
                "text-sm font-medium",
                primary.trend > 0 && "text-emerald-600 dark:text-emerald-400",
                primary.trend < 0 && "text-red-600 dark:text-red-400",
                primary.trend === 0 && "text-muted-foreground"
              )}
            >
              {primary.trend > 0 && "+"}
              {primary.trend.toFixed(1)}%
              {primary.trendLabel && (
                <span className="ml-1 text-muted-foreground">
                  {primary.trendLabel}
                </span>
              )}
            </span>
          )}
        </div>
        {primary.description && (
          <p className="mt-1 text-sm text-muted-foreground">
            {primary.description}
          </p>
        )}
      </div>
      {secondary.length > 0 && (
        <div className="flex flex-wrap gap-6">
          {secondary.map((stat) => (
            <div key={stat.id}>
              <p className="text-xs text-muted-foreground">{stat.label}</p>
              <div className="flex items-baseline gap-1.5">
                <span className="text-lg font-semibold">
                  {stat.formatValue ? stat.formatValue(stat.value) : stat.value}
                </span>
                {stat.trend !== undefined && (
                  <span
                    className={cn(
                      "text-xs",
                      stat.trend > 0 && "text-emerald-600 dark:text-emerald-400",
                      stat.trend < 0 && "text-red-600 dark:text-red-400",
                      stat.trend === 0 && "text-muted-foreground"
                    )}
                  >
                    {stat.trend > 0 && "+"}
                    {stat.trend.toFixed(1)}%
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export { statGridVariants };
