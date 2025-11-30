//! History Utilities
//!
//! Helper functions for action history analysis, formatting, and manipulation.

import {
  ActionHistoryItem,
  ActionType,
  ResourceType,
  ActionStats,
} from '../types/history';

// Import and re-export formatting utilities from shared format module
import { formatTimestamp as _formatTimestamp, formatDurationMs as _formatDurationMs } from './format';

// Re-export with original names for backward compatibility
export const formatTimestamp = _formatTimestamp;
export const formatDuration = _formatDurationMs;

/**
 * Get a readable label for action type
 */
export function getActionLabel(action: ActionType): string {
  const labels: Record<ActionType, string> = {
    create: 'Created',
    update: 'Updated',
    delete: 'Deleted',
    load: 'Loaded',
    unload: 'Unloaded',
    swap: 'Swapped',
    train: 'Trained',
    deploy: 'Deployed',
    rollback: 'Rolled Back',
    configure: 'Configured',
    other: 'Other',
  };
  return labels[action] || labels.other;
}

/**
 * Get a readable label for resource type
 */
export function getResourceLabel(resource: ResourceType): string {
  const labels: Record<ResourceType, string> = {
    adapter: 'Adapter',
    stack: 'Stack',
    training: 'Training Job',
    model: 'Model',
    policy: 'Policy',
    node: 'Node',
    tenant: 'Tenant',
    other: 'Other',
  };
  return labels[resource] || labels.other;
}

/**
 * Categorize actions by time period
 */
export function categorizeByTimePeriod(
  actions: ActionHistoryItem[]
): Record<string, ActionHistoryItem[]> {
  const now = Date.now();
  const oneHourAgo = now - 60 * 60 * 1000;
  const oneDayAgo = now - 24 * 60 * 60 * 1000;
  const oneWeekAgo = now - 7 * 24 * 60 * 60 * 1000;

  const categories: Record<string, ActionHistoryItem[]> = {
    'Last Hour': [],
    'Last 24 Hours': [],
    'Last 7 Days': [],
    'Older': [],
  };

  actions.forEach((action) => {
    if (action.timestamp > oneHourAgo) {
      categories['Last Hour'].push(action);
    } else if (action.timestamp > oneDayAgo) {
      categories['Last 24 Hours'].push(action);
    } else if (action.timestamp > oneWeekAgo) {
      categories['Last 7 Days'].push(action);
    } else {
      categories['Older'].push(action);
    }
  });

  return categories;
}

/**
 * Find related actions (same resource, similar timeframe)
 */
export function findRelatedActions(
  action: ActionHistoryItem,
  allActions: ActionHistoryItem[],
  timeWindowMs: number = 5 * 60 * 1000
): ActionHistoryItem[] {
  return allActions.filter(
    (other) =>
      other.id !== action.id &&
      other.resource === action.resource &&
      Math.abs(other.timestamp - action.timestamp) <= timeWindowMs
  );
}

/**
 * Build action chain (sequence of related actions)
 */
export function buildActionChain(
  startAction: ActionHistoryItem,
  allActions: ActionHistoryItem[],
  maxDepth: number = 10
): ActionHistoryItem[] {
  const chain: ActionHistoryItem[] = [startAction];
  const visited = new Set<string>([startAction.id]);
  const timeWindowMs = 10 * 60 * 1000; // 10 minutes

  const traverse = (current: ActionHistoryItem, depth: number) => {
    if (depth >= maxDepth || visited.size >= allActions.length) return;

    const related = allActions.filter(
      (action) =>
        !visited.has(action.id) &&
        action.resource === current.resource &&
        action.timestamp > current.timestamp &&
        action.timestamp - current.timestamp <= timeWindowMs
    );

    related.sort((a, b) => a.timestamp - b.timestamp);

    for (const action of related) {
      visited.add(action.id);
      chain.push(action);
      traverse(action, depth + 1);
    }
  };

  traverse(startAction, 0);
  return chain;
}

/**
 * Calculate success rate for a subset of actions
 */
export function calculateSuccessRate(actions: ActionHistoryItem[]): number {
  if (actions.length === 0) return 0;
  const successCount = actions.filter((a) => a.status === 'success').length;
  return (successCount / actions.length) * 100;
}

/**
 * Calculate average duration for actions
 */
export function calculateAverageDuration(actions: ActionHistoryItem[]): number {
  const withDuration = actions.filter((a) => a.duration !== undefined);
  if (withDuration.length === 0) return 0;
  const total = withDuration.reduce((sum, a) => sum + (a.duration || 0), 0);
  return total / withDuration.length;
}

/**
 * Get frequency of actions over time buckets
 */
export function getActionFrequency(
  actions: ActionHistoryItem[],
  bucketSizeMs: number = 3600000
): Array<{ timestamp: number; count: number }> {
  const buckets = new Map<number, number>();

  actions.forEach((action) => {
    const bucket = Math.floor(action.timestamp / bucketSizeMs) * bucketSizeMs;
    buckets.set(bucket, (buckets.get(bucket) || 0) + 1);
  });

  return Array.from(buckets.entries())
    .map(([timestamp, count]) => ({ timestamp, count }))
    .sort((a, b) => a.timestamp - b.timestamp);
}

/**
 * Find anomalies (unusual action patterns)
 */
export function findAnomalies(actions: ActionHistoryItem[]): ActionHistoryItem[] {
  if (actions.length < 10) return [];

  const durations = actions
    .filter((a) => a.duration !== undefined)
    .map((a) => a.duration || 0);

  if (durations.length === 0) return [];

  const mean = durations.reduce((a, b) => a + b, 0) / durations.length;
  const variance = durations.reduce((sum, d) => sum + Math.pow(d - mean, 2), 0) / durations.length;
  const stdDev = Math.sqrt(variance);
  const threshold = mean + 3 * stdDev; // 3-sigma rule

  return actions.filter((a) => a.duration && a.duration > threshold);
}

/**
 * Group actions by resource and action type
 */
export function groupActions(
  actions: ActionHistoryItem[]
): Record<string, Record<string, ActionHistoryItem[]>> {
  const grouped: Record<string, Record<string, ActionHistoryItem[]>> = {};

  actions.forEach((action) => {
    const resourceKey = `${action.resource}`;
    const actionKey = `${action.action}`;

    if (!grouped[resourceKey]) {
      grouped[resourceKey] = {};
    }

    if (!grouped[resourceKey][actionKey]) {
      grouped[resourceKey][actionKey] = [];
    }

    grouped[resourceKey][actionKey].push(action);
  });

  return grouped;
}

/**
 * Calculate impact score (combination of factors)
 */
export function calculateImpactScore(action: ActionHistoryItem): number {
  let score = 0;

  // Status impact
  if (action.status === 'success') score += 10;
  if (action.status === 'failed') score -= 5;
  if (action.status === 'pending') score += 2;

  // Action type impact
  const criticalActions: ActionType[] = ['delete', 'rollback', 'deploy'];
  if (criticalActions.includes(action.action)) score += 10;

  // Duration impact (longer = higher impact)
  if (action.duration) {
    score += Math.min(action.duration / 100, 5);
  }

  // Resource type impact
  const criticalResources: ResourceType[] = ['policy', 'model'];
  if (criticalResources.includes(action.resource)) score += 5;

  return Math.round(score * 10) / 10;
}

/**
 * Generate summary text for action sequence
 */
export function generateSummary(actions: ActionHistoryItem[]): string {
  if (actions.length === 0) return 'No actions';

  const actionCounts: Record<string, number> = {};
  const resourceCounts: Record<string, number> = {};

  actions.forEach((action) => {
    actionCounts[action.action] = (actionCounts[action.action] || 0) + 1;
    resourceCounts[action.resource] = (resourceCounts[action.resource] || 0) + 1;
  });

  const actionSummaries = Object.entries(actionCounts)
    .map(([action, count]) => `${count} ${getActionLabel(action as ActionType).toLowerCase()}${count > 1 ? 's' : ''}`)
    .join(', ');

  const resourceSummaries = Object.entries(resourceCounts)
    .map(([resource, count]) => `${count} ${getResourceLabel(resource as ResourceType).toLowerCase()}${count > 1 ? 's' : ''}`)
    .join(', ');

  const startTime = new Date(actions[0].timestamp).toLocaleString();
  const endTime = new Date(actions[actions.length - 1].timestamp).toLocaleString();

  return `${actions.length} actions: ${actionSummaries} on ${resourceSummaries} from ${startTime} to ${endTime}`;
}

/**
 * Export actions as detailed report
 */
export function generateDetailedReport(actions: ActionHistoryItem[]): string {
  let report = '# Action History Report\n\n';
  report += `Generated: ${new Date().toISOString()}\n`;
  report += `Total Actions: ${actions.length}\n\n`;

  // Summary statistics
  const successCount = actions.filter((a) => a.status === 'success').length;
  const failedCount = actions.filter((a) => a.status === 'failed').length;
  const avgDuration = calculateAverageDuration(actions);

  report += '## Summary\n\n';
  report += `- Success Rate: ${calculateSuccessRate(actions).toFixed(1)}%\n`;
  report += `- Successful: ${successCount}\n`;
  report += `- Failed: ${failedCount}\n`;
  report += `- Average Duration: ${formatDuration(avgDuration)}\n\n`;

  // Group by time period
  report += '## By Time Period\n\n';
  const byPeriod = categorizeByTimePeriod(actions);
  Object.entries(byPeriod).forEach(([period, periodActions]) => {
    if (periodActions.length > 0) {
      report += `### ${period} (${periodActions.length} actions)\n\n`;
      report += `Success Rate: ${calculateSuccessRate(periodActions).toFixed(1)}%\n\n`;
    }
  });

  // Detailed action list
  report += '## Action Details\n\n';
  actions.forEach((action) => {
    report += `### ${getActionLabel(action.action)} ${getResourceLabel(action.resource)}\n\n`;
    report += `- ID: ${action.id}\n`;
    report += `- Time: ${formatTimestamp(action.timestamp, 'long')}\n`;
    report += `- Status: ${action.status}\n`;
    if (action.duration) report += `- Duration: ${formatDuration(action.duration)}\n`;
    report += `- Description: ${action.description}\n`;
    if (action.errorMessage) report += `- Error: ${action.errorMessage}\n`;
    if (action.tags?.length) report += `- Tags: ${action.tags.join(', ')}\n`;
    report += '\n';
  });

  return report;
}
