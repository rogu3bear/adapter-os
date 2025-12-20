/**
 * UI State Types
 * Types for managing UI state and interactions
 */

export type Theme = 'light' | 'dark' | 'system';

export interface UIPreferences {
  theme: Theme;
  sidebarCollapsed: boolean;
  compactMode: boolean;
  showMinimap: boolean;
  codeTheme: 'vs-dark' | 'github-light' | 'monokai';
  fontSize: number;
  animationsEnabled: boolean;
}

export interface ModalState {
  isOpen: boolean;
  component?: React.ComponentType<any>;
  props?: Record<string, any>;
  onClose?: () => void;
}

export interface ToastNotification {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message?: string;
  duration?: number;
  action?: {
    label: string;
    onClick: () => void;
  };
}

export interface DrawerState {
  isOpen: boolean;
  position: 'left' | 'right' | 'top' | 'bottom';
  content?: React.ReactNode;
  width?: string | number;
  height?: string | number;
}

export interface PanelState {
  id: string;
  isExpanded: boolean;
  size?: number;
  minSize?: number;
  maxSize?: number;
}

export interface LayoutState {
  panels: Record<string, PanelState>;
  activePanel?: string;
  splitRatio?: number;
}

export interface FilterState<T = any> {
  filters: Record<string, T>;
  activeFilters: string[];
  searchQuery: string;
}

export interface SortState {
  field: string;
  direction: 'asc' | 'desc';
}

export interface PaginationState {
  page: number;
  pageSize: number;
  total: number;
}

export interface TableState {
  sort?: SortState;
  pagination: PaginationState;
  filters: FilterState;
  selectedRows: Set<string>;
}

export interface NavigationState {
  currentPath: string;
  previousPath?: string;
  breadcrumbs: Array<{ label: string; path: string }>;
}

export interface LoadingState {
  isLoading: boolean;
  loadingMessage?: string;
  progress?: number;
}

export interface ErrorState {
  hasError: boolean;
  error?: Error;
  errorMessage?: string;
  errorCode?: string;
}

export interface AsyncOperationState {
  status: 'idle' | 'loading' | 'success' | 'error';
  data?: any;
  error?: Error;
}
