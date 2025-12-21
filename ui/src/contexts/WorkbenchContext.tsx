/**
 * WorkbenchContext - Manages state for the unified Workbench view
 *
 * Coordinates left rail tabs, right rail collapse/pin state,
 * and undo actions for the Workbench chat interface.
 */

import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  ReactNode,
} from 'react';
import { readLocalStorage, writeLocalStorage } from '@/utils/storage';

// ============================================================================
// Types
// ============================================================================

export type LeftRailTab = 'sessions' | 'datasets' | 'stacks';

export interface UndoAction {
  type: 'detach_all';
  previousStackId: string | null;
  previousAdapterOverrides: Record<string, number>;
  previousScope: {
    selectedStackId: string | null;
    stackName: string | null;
  } | null;
  expiresAt: number;
}

interface WorkbenchState {
  // Left rail
  activeLeftTab: LeftRailTab;
  leftRailScrollPositions: Record<string, number>;

  // Right rail
  rightRailCollapsed: boolean;
  pinnedMessageId: string | null;
  selectedMessageId: string | null;

  // Adapter strength overrides (lifted from ChatInterface for undo support)
  strengthOverrides: Record<string, number>;

  // Undo
  undoAction: UndoAction | null;
}

interface WorkbenchActions {
  // Left rail
  setActiveLeftTab: (tab: LeftRailTab) => void;
  saveScrollPosition: (tab: string, position: number) => void;
  getScrollPosition: (tab: string) => number;

  // Right rail
  setRightRailCollapsed: (collapsed: boolean) => void;
  toggleRightRail: () => void;
  pinMessage: (messageId: string | null) => void;
  selectMessage: (messageId: string | null) => void;

  // Adapter strength overrides
  setStrengthOverrides: (overrides: Record<string, number>) => void;
  updateStrengthOverride: (adapterId: string, strength: number) => void;
  clearStrengthOverrides: () => void;

  // Undo
  setUndoAction: (action: UndoAction | null) => void;
  clearUndoAction: () => void;

  // Keyboard
  handleGlobalEscape: () => boolean;
}

interface WorkbenchContextValue extends WorkbenchState, WorkbenchActions {}

// ============================================================================
// Storage Keys
// ============================================================================

const STORAGE_KEYS = {
  LEFT_RAIL_TAB: 'workbench:leftRail:activeTab',
  LEFT_RAIL_SCROLL: 'workbench:leftRail:scrollPositions',
  RIGHT_RAIL_COLLAPSED: 'workbench:rightRail:collapsed',
} as const;

// ============================================================================
// Storage Helpers
// ============================================================================

function getStoredLeftRailTab(): LeftRailTab {
  const stored = readLocalStorage(STORAGE_KEYS.LEFT_RAIL_TAB);
  if (stored === 'sessions' || stored === 'datasets' || stored === 'stacks') {
    return stored;
  }
  return 'sessions';
}

function getStoredScrollPositions(): Record<string, number> {
  const stored = readLocalStorage(STORAGE_KEYS.LEFT_RAIL_SCROLL);
  if (stored) {
    try {
      return JSON.parse(stored);
    } catch {
      return {};
    }
  }
  return {};
}

function getStoredRightRailCollapsed(): boolean {
  return readLocalStorage(STORAGE_KEYS.RIGHT_RAIL_COLLAPSED) === 'true';
}

// ============================================================================
// Context
// ============================================================================

const WorkbenchContext = createContext<WorkbenchContextValue | null>(null);

// ============================================================================
// Provider
// ============================================================================

interface WorkbenchProviderProps {
  children: ReactNode;
}

export function WorkbenchProvider({ children }: WorkbenchProviderProps) {
  // Left rail state
  const [activeLeftTab, setActiveLeftTabInternal] = useState<LeftRailTab>(
    getStoredLeftRailTab
  );
  const [leftRailScrollPositions, setLeftRailScrollPositions] = useState<
    Record<string, number>
  >(getStoredScrollPositions);

  // Right rail state
  const [rightRailCollapsed, setRightRailCollapsedInternal] = useState(
    getStoredRightRailCollapsed
  );
  const [pinnedMessageId, setPinnedMessageId] = useState<string | null>(null);
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(
    null
  );

  // Use refs to avoid callbacks changing on every state change
  const pinnedMessageIdRef = useRef<string | null>(null);
  pinnedMessageIdRef.current = pinnedMessageId;

  const leftRailScrollPositionsRef = useRef<Record<string, number>>({});
  leftRailScrollPositionsRef.current = leftRailScrollPositions;

  const rightRailCollapsedRef = useRef<boolean>(false);
  rightRailCollapsedRef.current = rightRailCollapsed;

  // Adapter strength overrides state
  const [strengthOverrides, setStrengthOverridesInternal] = useState<
    Record<string, number>
  >({});

  // Undo state
  const [undoAction, setUndoActionInternal] = useState<UndoAction | null>(null);

  // Auto-expire undo action
  useEffect(() => {
    if (!undoAction) return;

    const timeUntilExpiry = undoAction.expiresAt - Date.now();
    if (timeUntilExpiry <= 0) {
      setUndoActionInternal(null);
      return;
    }

    const timer = setTimeout(() => {
      setUndoActionInternal(null);
    }, timeUntilExpiry);

    return () => clearTimeout(timer);
  }, [undoAction]);

  // -------------------------------------------------------------------------
  // Left rail actions
  // -------------------------------------------------------------------------

  const setActiveLeftTab = useCallback((tab: LeftRailTab) => {
    setActiveLeftTabInternal(tab);
    writeLocalStorage(STORAGE_KEYS.LEFT_RAIL_TAB, tab);
  }, []);

  const saveScrollPosition = useCallback((tab: string, position: number) => {
    setLeftRailScrollPositions((prev) => {
      const next = { ...prev, [tab]: position };
      writeLocalStorage(STORAGE_KEYS.LEFT_RAIL_SCROLL, JSON.stringify(next));
      return next;
    });
  }, []);

  const getScrollPosition = useCallback((tab: string) => {
    return leftRailScrollPositionsRef.current[tab] ?? 0;
  }, []);

  // -------------------------------------------------------------------------
  // Right rail actions
  // -------------------------------------------------------------------------

  const setRightRailCollapsed = useCallback((collapsed: boolean) => {
    setRightRailCollapsedInternal(collapsed);
    writeLocalStorage(STORAGE_KEYS.RIGHT_RAIL_COLLAPSED, String(collapsed));
  }, []);

  const toggleRightRail = useCallback(() => {
    setRightRailCollapsedInternal((prev) => {
      const next = !prev;
      writeLocalStorage(STORAGE_KEYS.RIGHT_RAIL_COLLAPSED, String(next));
      return next;
    });
  }, []);

  const pinMessage = useCallback((messageId: string | null) => {
    setPinnedMessageId(messageId);
  }, []);

  const selectMessage = useCallback((messageId: string | null) => {
    // Only auto-select if not pinned, or if explicitly setting to null
    if (messageId === null || !pinnedMessageIdRef.current) {
      setSelectedMessageId(messageId);
    }
  }, []);

  // -------------------------------------------------------------------------
  // Adapter strength override actions
  // -------------------------------------------------------------------------

  const setStrengthOverrides = useCallback(
    (overrides: Record<string, number>) => {
      setStrengthOverridesInternal(overrides);
    },
    []
  );

  const updateStrengthOverride = useCallback(
    (adapterId: string, strength: number) => {
      setStrengthOverridesInternal((prev) => ({
        ...prev,
        [adapterId]: strength,
      }));
    },
    []
  );

  const clearStrengthOverrides = useCallback(() => {
    setStrengthOverridesInternal({});
  }, []);

  // -------------------------------------------------------------------------
  // Undo actions
  // -------------------------------------------------------------------------

  const setUndoAction = useCallback((action: UndoAction | null) => {
    setUndoActionInternal(action);
  }, []);

  const clearUndoAction = useCallback(() => {
    setUndoActionInternal(null);
  }, []);

  // -------------------------------------------------------------------------
  // Keyboard handling
  // -------------------------------------------------------------------------

  const handleGlobalEscape = useCallback((): boolean => {
    // 1. If right rail is open, collapse it
    if (!rightRailCollapsedRef.current) {
      setRightRailCollapsed(true);
      return true;
    }

    // 2. Focus chat input
    const chatInput = document.querySelector<HTMLElement>(
      '[data-testid="chat-input"]'
    );
    if (chatInput) {
      chatInput.focus();
      return true;
    }

    return false;
  }, [setRightRailCollapsed]);

  // -------------------------------------------------------------------------
  // Context value
  // -------------------------------------------------------------------------

  const value = useMemo<WorkbenchContextValue>(
    () => ({
      // State
      activeLeftTab,
      leftRailScrollPositions,
      rightRailCollapsed,
      pinnedMessageId,
      selectedMessageId,
      strengthOverrides,
      undoAction,

      // Actions
      setActiveLeftTab,
      saveScrollPosition,
      getScrollPosition,
      setRightRailCollapsed,
      toggleRightRail,
      pinMessage,
      selectMessage,
      setStrengthOverrides,
      updateStrengthOverride,
      clearStrengthOverrides,
      setUndoAction,
      clearUndoAction,
      handleGlobalEscape,
    }),
    [
      activeLeftTab,
      leftRailScrollPositions,
      rightRailCollapsed,
      pinnedMessageId,
      selectedMessageId,
      strengthOverrides,
      undoAction,
      setActiveLeftTab,
      saveScrollPosition,
      getScrollPosition,
      setRightRailCollapsed,
      toggleRightRail,
      pinMessage,
      selectMessage,
      setStrengthOverrides,
      updateStrengthOverride,
      clearStrengthOverrides,
      setUndoAction,
      clearUndoAction,
      handleGlobalEscape,
    ]
  );

  return (
    <WorkbenchContext.Provider value={value}>
      {children}
    </WorkbenchContext.Provider>
  );
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook to access workbench context
 * @throws Error if used outside of WorkbenchProvider
 */
export function useWorkbench(): WorkbenchContextValue {
  const context = useContext(WorkbenchContext);
  if (!context) {
    throw new Error('useWorkbench must be used within a WorkbenchProvider');
  }
  return context;
}

/**
 * Hook to access workbench context without throwing
 * Returns null if outside provider
 */
export function useWorkbenchOptional(): WorkbenchContextValue | null {
  return useContext(WorkbenchContext);
}

export default WorkbenchContext;
