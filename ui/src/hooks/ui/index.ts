/**
 * UI-specific hooks
 *
 * This module contains hooks for UI interactions, modals, responsive design, etc.
 */

// Mobile/responsive detection
export * from './useMobile';

// Modal state management
export * from './useModal';

// Action history and undo/redo
export * from './useActionHistory';
export * from './useEnhancedActionHistory';
export * from './useUndoRedo';

// Bulk operations and selection
export * from './useBulkActions';
export * from './useSelection';

// User confirmations and dialogs
// Note: useConfirmation is exported from useModal
// Note: useDialogManager exports domain-specific dialog hooks (useAdapterDialogs, useChatDialogs, etc.)
// which conflict with domain-specific hooks. Import directly from '@/hooks/ui/useDialogManager' if needed.
// export * from './useDialogManager'; // Commented out to avoid conflicts

// Data loading and filtering
export * from './useDataLoader';
export * from './useFilter';
export * from './useSort';
export * from './usePagination';
export * from './useInfiniteScroll';

// Debouncing and timing
export * from './useDebouncedValue';
export * from './useTimestamp';

// Progress and operations
export * from './useProgressOperation';

// UI modes and features
export * from './useUiMode';
export * from './useFeatureDegradation';
export * from './useInformationDensity';

// Layout and transitions
export * from './useLayoutDebug';
export * from './useViewTransition';

// Accessibility
export * from './useReducedMotion';

// Navigation and redirects
export * from './useFirstRunRedirect';

// Evidence and specialized UI
export * from './useEvidenceDrawer';

// Error handling
export * from './useErrorHandler';
