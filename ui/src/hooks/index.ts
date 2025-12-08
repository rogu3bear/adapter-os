/**
 * React hooks exports
 *
 * This file provides centralized exports for all custom hooks in the application.
 * Hooks are organized by category for better discoverability.
 */

// UI/UX Enhancement Hooks
export * from './use-toast';
export * from './useBreadcrumbs';
export * from './useConfirmation';
export * from './useContextualTutorial';
export * from './useDashboardConfig';
export * from './useInformationDensity';
export * from './useProgressiveDisclosure';
export * from './useProgressiveHints';
export * from './useViewTransition';

// API Integration Hooks
export * from './useAdaptersApi';
export * from './useChatSessionsApi';
export * from './useChatSearch';
export * from './useChatCategories';
export * from './useChatTags';
export * from './useChatArchive';
export * from './useChatSharing';
export * from './useDocumentsApi';
export * from './useCollectionsApi';
export * from './useEvidenceApi';
export * from './useSettings';
export * from './useSSE';

// Data Management Hooks
export * from './useActivityEvents';
export * from './useActivityFeed';
export * from './useAdapterDetail';
export * from './useAdapterOperations';
export * from './useAdapterActions';
export * from './useMessages';
export * from './useNotifications';
export * from './usePolicies';
export * from './usePolicyChecks';
export * from './usePromptTemplates';
export * from './useServiceStatus';
export * from './useSystem';
export * from './useSystemMetrics';
export * from './useWorkspaces';
export * from './useTraining';
export * from './useAdmin';
export * from './useChatSessions';

// State Management Hooks
export * from './useActionHistory';
export * from './useCanonicalState';
export * from './useEnhancedActionHistory';
export * from './useHistoryPersistence';
export * from './useOptimisticUpdate';
export * from './useUndoRedo';
export * from './useWizardPersistence';
export * from './useWorkflowPersistence';

// Utility Hooks
export * from './useAsyncAction';
export * from './useAsyncOperation';
export * from './useBulkActions';
export * from './useCancellableOperation';
export * from './useDebouncedValue';
export * from './useFeatureDegradation';
export * from './useFeatureFlags';
export * from './useFilter';
export * from './useFirstRunRedirect';
export * from './useFormValidation';
export * from './useInfiniteScroll';
export * from './usePagination';
export * from './usePolling';
export * from './useProgressOperation';
export * from './useRBAC';
export * from './useRetry';
export * from './useSecurity';
export * from './useSelection';
export * from './useSort';
export * from './useStreamingEndpoints';
export * from './useTimestamp';
export * from './useZodFormValidation';

// Notification Hooks
export * from './useBatchedTrainingNotifications';
export * from './useTrainingNotifications';

// Inference Hooks (organized in subdirectory)
export * from './inference';

// Adapter Hooks (organized in subdirectory)
export * from './adapters';
