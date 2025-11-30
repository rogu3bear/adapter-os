/**
 * Utils barrel export
 *
 * Provides centralized exports for all utility functions.
 * Import pattern: import { logger, rbac } from '@/utils';
 */

// Logging
export { logger, toError } from './logger';

// Formatting utilities (unified - replaces scattered implementations)
export {
  formatDuration,
  formatDurationMs,
  formatDurationSeconds,
  formatBytes,
  formatTimestamp,
  formatRelativeTime,
  formatPercent,
  formatNumber,
} from './format';

// Adapter utilities
export * from './adapters';

// RBAC
export * from './rbac';

// Accessibility
export * from './accessibility';

// Breadcrumbs
export * from './breadcrumbs';

// Diff utilities
export * from './diff';

// Document loading
export * from './doc-loader';

// Error messages
export * from './errorMessages';

// History utilities (formatTimestamp now from ./format)
export {
  getActionLabel,
  getResourceLabel,
  categorizeByTimePeriod,
  findRelatedActions,
  buildActionChain,
  calculateSuccessRate,
  calculateAverageDuration,
  getActionFrequency,
  findAnomalies,
  groupActions,
  calculateImpactScore,
  generateSummary,
  generateDetailedReport,
} from './history-utils';

// Lifecycle utilities
export * from './lifecycle';

// Navigation
export * from './navigation';

// Retry utilities
export * from './retry';

// Visual hierarchy
export * from './visual-hierarchy';

// Memory estimation
export * from './memoryEstimation';

// Training ETA (formatDuration now from ./format)
export { calculateTrainingETA } from './trainingEta';

// Peer sync
export * from './peerSync';

// Mock peer data (for development/testing)
export * from './mockPeerData';
