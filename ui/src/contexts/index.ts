/**
 * React Context exports
 *
 * Central export point for all React contexts used in the application.
 * Each context provides specialized state management for different features.
 */

// Bookmark management - Persistent bookmarks across the application
export {
  BookmarkProvider,
  useBookmarks,
  type Bookmark,
  type BookmarkType,
} from './BookmarkContext';

// Breadcrumb navigation - Dynamic breadcrumb trails
export {
  BreadcrumbProvider,
  useBreadcrumb,
  type BreadcrumbItem,
} from './BreadcrumbContext';

// Command palette - Global command/search interface
export {
  CommandPaletteProvider,
  useCommandPalette,
  type CommandItem,
  type CommandItemType,
} from './CommandPaletteContext';

// Information density - UI spacing and sizing preferences
export {
  DensityProvider,
  useDensity,
} from './DensityContext';

// Document viewer - PDF viewer state for chat interface
export {
  DocumentViewerProvider,
  useDocumentViewer,
  useDocumentViewerOptional,
  default as DocumentViewerContext,
} from './DocumentViewerContext';

// Dataset chat - Dataset-scoped chat context
export {
  DatasetChatProvider,
  useDatasetChat,
  useDatasetChatOptional,
  default as DatasetChatContext,
} from './DatasetChatContext';

// Evidence drawer - Evidence and router decision display
export {
  EvidenceDrawerProvider,
  useEvidenceDrawer,
  useEvidenceDrawerOptional,
  default as EvidenceDrawerContext,
} from './EvidenceDrawerContext';

// Action history - Global action tracking and replay
export {
  HistoryProvider,
  useHistory,
  default as HistoryContext,
} from './HistoryContext';

// Undo/Redo - Global undo/redo with keyboard shortcuts
export {
  UndoRedoProvider,
  useUndoRedoContext,
  default as UndoRedoContext,
} from './UndoRedoContext';

// Workbench - Unified chat hub state management
export {
  WorkbenchProvider,
  useWorkbench,
  useWorkbenchOptional,
  default as WorkbenchContext,
  type LeftRailTab,
  type UndoAction,
} from './WorkbenchContext';
