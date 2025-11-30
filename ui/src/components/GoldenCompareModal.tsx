/**
 * GoldenCompareModal - Re-exports the enhanced version
 *
 * @deprecated This module re-exports GoldenCompareModalEnhanced for backwards compatibility.
 * Import directly from './golden/GoldenCompareModalEnhanced' for new code.
 *
 * The enhanced version includes additional features:
 * - View mode selector (table/heatmap/stats)
 * - Layer detail modal integration
 * - Statistical summary visualization
 * - Epsilon heatmap visualization
 */

// Re-export the enhanced component with original name for backwards compatibility
export { GoldenCompareModalEnhanced as GoldenCompareModal } from './golden/GoldenCompareModalEnhanced';

// Also provide default export for files using `import GoldenCompareModal from ...`
export { GoldenCompareModalEnhanced as default } from './golden/GoldenCompareModalEnhanced';
