/**
 * Dashboard Hooks
 *
 * Provides hooks for fetching and managing dashboard-related data
 * including real-time metrics and computed statistics.
 */

export {
  useDashboardMetrics,
  type UseDashboardMetricsOptions,
  type UseDashboardMetricsReturn,
} from './useDashboardMetrics';

export {
  useDashboardStats,
  type UseDashboardStatsOptions,
  type UseDashboardStatsReturn,
  type DatasetStats,
} from './useDashboardStats';
