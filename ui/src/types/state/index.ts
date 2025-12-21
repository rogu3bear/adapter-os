/**
 * State Types Index
 *
 * Central export for all state management types.
 * Organizes state patterns by category.
 */

// Selection state
export type {
  SelectionState,
  SelectionActions,
  SelectionStateWithActions,
  MultiSelectConfig,
  SelectionMode,
  SelectionStateWithMode,
  BulkActionConfirmationState,
  BulkOperationProgress,
  BulkOperationState,
} from './selection';

// Filter and sort state
export type {
  SortState,
  SortActions,
  FilterState,
  FilterActions,
  SearchState,
  SearchActions,
  PaginationState,
  PaginationActions,
  FilteredListState,
  FilteredListActions,
  FilteredListStateWithActions,
  FilterPreset,
  FilterPresetManager,
} from './filters';

// Modal and dialog state
export type {
  ModalState,
  ModalActions,
  ModalStateWithActions,
  DialogState,
  DialogActions,
  DialogStateWithActions,
  ConfirmationDialogData,
  ConfirmationDialogState,
  MultiStepDialogState,
  MultiStepDialogActions,
  DrawerState,
  DrawerActions,
  DrawerStateWithActions,
  ToastState,
  ToastManager,
  PanelState,
  PanelActions,
  PanelStateWithActions,
} from './modals';

// Async operation state
export type {
  AsyncStatus,
  AsyncOperationState,
  AsyncOperationActions,
  AsyncOperationStateWithActions,
  RetryConfig,
  RetryState,
  RetryActions,
  CancellableOperationState,
  CancellableOperationActions,
  ProgressState,
  LoadingStateWithProgress,
  OptimisticUpdateState,
  OptimisticUpdateActions,
  DebouncedState,
  DebouncedActions,
} from './async';

// Navigation state
export type {
  BreadcrumbItem,
  BreadcrumbState,
  NavigationTab,
  TabNavigationState,
  TabNavigationActions,
  NavigationHistoryState,
  NavigationHistoryActions,
  SidebarState,
  SidebarActions,
  SidebarStateWithActions,
  RouteParamState,
  NavigationContext,
} from './navigation';

// Legacy exports (for backward compatibility)
export * from './bulk-actions';
export * from './ui';

/**
 * Generic state slice pattern
 */
export interface StateSlice<T> {
  state: T;
  setState: (update: Partial<T> | ((prev: T) => T)) => void;
  resetState: () => void;
}

/**
 * Persisted state configuration
 */
export interface PersistedStateConfig<T> {
  /** Storage key */
  key: string;
  /** Storage type */
  storage?: 'local' | 'session';
  /** Serializer function */
  serialize?: (value: T) => string;
  /** Deserializer function */
  deserialize?: (value: string) => T;
  /** Version for migration */
  version?: number;
}

/**
 * Feature flag state
 */
export interface FeatureFlags {
  [key: string]: boolean;
}

/**
 * Feature flag manager
 */
export interface FeatureFlagManager {
  /** Feature flags */
  flags: FeatureFlags;
  /** Check if feature is enabled */
  isEnabled: (feature: string) => boolean;
  /** Enable feature */
  enable: (feature: string) => void;
  /** Disable feature */
  disable: (feature: string) => void;
  /** Toggle feature */
  toggle: (feature: string) => void;
}
