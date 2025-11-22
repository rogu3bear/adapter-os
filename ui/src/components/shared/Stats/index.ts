/**
 * Stats and Metrics Components
 *
 * A comprehensive set of components for displaying statistics, metrics,
 * progress indicators, and resource usage visualizations.
 *
 * @example
 * // Single stat card
 * import { StatCard } from './Stats';
 * <StatCard
 *   label="Total Users"
 *   value={1234}
 *   trend={12.5}
 *   trendLabel="vs last month"
 *   sparklineData={[10, 20, 15, 30, 25, 35]}
 * />
 *
 * @example
 * // Grid of stats
 * import { StatGrid } from './Stats';
 * <StatGrid
 *   columns={4}
 *   stats={[
 *     { id: '1', label: 'Users', value: 1234, trend: 12.5 },
 *     { id: '2', label: 'Revenue', value: '$45,678', trend: -2.3 },
 *   ]}
 * />
 *
 * @example
 * // Usage bar for resources
 * import { UsageBar } from './Stats';
 * <UsageBar
 *   label="Memory"
 *   value={6.4}
 *   max={8}
 *   unit="GB"
 * />
 *
 * @example
 * // Status indicator
 * import { StatusIndicator, StatusBadge } from './Stats';
 * <StatusIndicator status="online" />
 * <StatusBadge status="warning" label="High Load" />
 */

// Stat Cards
export {
  StatCard,
  CompactStatCard,
  statCardVariants,
  statValueVariants,
  statLabelVariants,
  type StatCardProps,
  type CompactStatCardProps,
} from "./StatCard";

// Stat Grid Layouts
export {
  StatGrid,
  StatRow,
  StatSummary,
  statGridVariants,
  type StatItem,
  type StatGridProps,
  type StatRowProps,
  type StatSummaryProps,
} from "./StatGrid";

// Progress Bars
export {
  ProgressBar,
  SegmentedProgress,
  progressBarVariants,
  progressFillVariants,
  type ProgressBarProps,
  type SegmentedProgressProps,
} from "./ProgressBar";

// Trend Indicator
export {
  TrendIndicator,
  trendIndicatorVariants,
  type TrendIndicatorProps,
} from "./TrendIndicator";

// Charts
export {
  MetricChart,
  BarChart,
  AreaChart,
  metricChartVariants,
  type MetricChartProps,
  type BarChartProps,
  type AreaChartProps,
} from "./MetricChart";

// Usage Bars
export {
  UsageBar,
  MultiUsageBar,
  usageBarVariants,
  usageBarTrackVariants,
  usageFillVariants,
  type UsageLevel,
  type UsageBarProps,
  type MultiUsageBarProps,
} from "./UsageBar";

// Status Indicators
export {
  StatusIndicator,
  StatusBadge,
  statusIndicatorVariants,
  statusDotVariants,
  statusBadgeVariants,
  type StatusType,
  type StatusIndicatorProps,
  type StatusBadgeProps,
} from "./StatusIndicator";
