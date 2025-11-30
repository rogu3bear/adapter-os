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

// Status Indicators (remaining after cleanup)
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
