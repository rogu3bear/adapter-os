/**
 * Utils barrel export
 *
 * Provides centralized exports for all utility functions.
 * Import pattern: import { logger, rbac } from '@/utils';
 *
 * NOTE: Some modules have naming collisions (e.g., formatDuration).
 * Import those directly from their source files when needed.
 */

// Logging
export { logger, toError } from './logger';

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

// History utilities - NOTE: formatDuration conflicts with trainingEta
// Use import { formatDuration } from '@/utils/history-utils' for ms-based version
export {
  formatTimestamp,
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

// Training ETA - NOTE: formatDuration conflicts with history-utils
export {
  calculateTrainingETA,
  formatDuration, // Using this version (seconds-based) as primary
} from './trainingEta';

// Peer sync
export * from './peerSync';

// Mock peer data (for development/testing)
export * from './mockPeerData';
