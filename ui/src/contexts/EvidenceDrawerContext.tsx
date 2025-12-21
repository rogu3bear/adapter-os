/**
 * EvidenceDrawerContext - Manages state for evidence drawer in chat interface
 *
 * Provides shared state and actions for the evidence drawer that displays
 * router decisions and evidence items for chat messages.
 */

import { createContext, useContext, useState, useCallback, useRef, ReactNode } from 'react';
import type { EvidenceItem } from '@/components/chat/ChatMessage';
import type { ExtendedRouterDecision } from '@/api/api-types';

/** Available tabs in the evidence drawer */
export type EvidenceDrawerTab = 'rulebook' | 'calculation' | 'trace';

interface EvidenceDrawerState {
  /** Whether drawer is open */
  isOpen: boolean;
  /** ID of the message whose evidence is displayed */
  activeMessageId: string | null;
  /** Active tab in the drawer */
  activeTab: EvidenceDrawerTab;
  /** Current evidence items being displayed */
  currentEvidence: EvidenceItem[] | null;
  /** Current router decision being displayed */
  currentRouterDecision: ExtendedRouterDecision | null;
  /** Current request ID */
  currentRequestId: string | null;
  /** Current trace ID */
  currentTraceId: string | null;
  /** Current proof digest */
  currentProofDigest: string | null;
  /** Whether the response is verified */
  currentIsVerified: boolean;
  /** When the response was verified */
  currentVerifiedAt: string | null;
  /** Token throughput statistics */
  currentThroughputStats: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number } | null;
  /** Whether drawer is pinned to a specific message */
  isPinned: boolean;
  /** ID of the pinned message (when pinned) */
  pinnedMessageId: string | null;
  /** ID of the latest message (tracks most recent for auto-follow) */
  latestMessageId: string | null;
}

interface EvidenceDrawerActions {
  /** Open drawer for a specific message, optionally setting active tab */
  openDrawer: (messageId: string, tab?: EvidenceDrawerTab) => void;
  /** Close the drawer */
  closeDrawer: () => void;
  /** Set the active tab */
  setActiveTab: (tab: EvidenceDrawerTab) => void;
  /** Update the message data being displayed */
  setMessageData: (data: {
    evidence?: EvidenceItem[];
    routerDecision?: ExtendedRouterDecision;
    requestId?: string;
    traceId?: string;
    proofDigest?: string | null;
    isVerified?: boolean;
    verifiedAt?: string;
    throughputStats?: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number };
  }) => void;
  /** Toggle pin state for current message */
  togglePin: () => void;
  /** Pin drawer to a specific message */
  pinToMessage: (messageId: string) => void;
  /** Unpin the drawer */
  unpin: () => void;
  /** Update the latest message ID for auto-follow tracking */
  setLatestMessageId: (messageId: string) => void;
  /** Jump to latest message and unpin */
  jumpToLatest: () => void;
  /** Auto-follow to message with data (only if not pinned) */
  autoFollowToMessage: (messageId: string, data: {
    evidence?: EvidenceItem[];
    routerDecision?: ExtendedRouterDecision;
    requestId?: string;
    traceId?: string;
    proofDigest?: string | null;
    isVerified?: boolean;
    verifiedAt?: string;
  }) => void;
}

interface EvidenceDrawerContextValue extends EvidenceDrawerState, EvidenceDrawerActions {}

const EvidenceDrawerContext = createContext<EvidenceDrawerContextValue | null>(null);

interface EvidenceDrawerProviderProps {
  children: ReactNode;
}

export function EvidenceDrawerProvider({ children }: EvidenceDrawerProviderProps) {
  const [state, setState] = useState<EvidenceDrawerState>({
    isOpen: false,
    activeMessageId: null,
    activeTab: 'rulebook',
    currentEvidence: null,
    currentRouterDecision: null,
    currentRequestId: null,
    currentTraceId: null,
    currentProofDigest: null,
    currentIsVerified: false,
    currentVerifiedAt: null,
    currentThroughputStats: null,
    isPinned: false,
    pinnedMessageId: null,
    latestMessageId: null,
  });

  // Use ref to avoid stale closures in autoFollowToMessage
  const isPinnedRef = useRef<boolean>(false);
  isPinnedRef.current = state.isPinned;

  const openDrawer = useCallback((messageId: string, tab?: EvidenceDrawerTab) => {
    setState((prev) => {
      // If drawer is already open and we're clicking a different message, auto-pin to it
      const shouldAutoPinOnSwitch = prev.isOpen && prev.activeMessageId !== messageId;

      return {
        ...prev,
        isOpen: true,
        activeMessageId: messageId,
        activeTab: tab ?? prev.activeTab,
        isPinned: shouldAutoPinOnSwitch ? true : prev.isPinned,
        pinnedMessageId: shouldAutoPinOnSwitch ? messageId : prev.pinnedMessageId,
      };
    });
  }, []);

  const closeDrawer = useCallback(() => {
    setState((prev) => ({
      ...prev,
      isOpen: false,
    }));
  }, []);

  const setActiveTab = useCallback((tab: EvidenceDrawerTab) => {
    setState((prev) => ({
      ...prev,
      activeTab: tab,
    }));
  }, []);

  const setMessageData = useCallback((data: {
    evidence?: EvidenceItem[];
    routerDecision?: ExtendedRouterDecision;
    requestId?: string;
    traceId?: string;
    proofDigest?: string | null;
    isVerified?: boolean;
    verifiedAt?: string;
    throughputStats?: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number };
  }) => {
    setState((prev) => ({
      ...prev,
      currentEvidence: data.evidence ?? prev.currentEvidence,
      currentRouterDecision: data.routerDecision ?? prev.currentRouterDecision,
      currentRequestId: data.requestId ?? prev.currentRequestId,
      currentTraceId: data.traceId ?? prev.currentTraceId,
      currentProofDigest: data.proofDigest ?? prev.currentProofDigest,
      currentIsVerified: data.isVerified ?? prev.currentIsVerified,
      currentVerifiedAt: data.verifiedAt ?? prev.currentVerifiedAt,
      currentThroughputStats: data.throughputStats ?? prev.currentThroughputStats,
    }));
  }, []);

  const togglePin = useCallback(() => {
    setState((prev) => ({
      ...prev,
      isPinned: !prev.isPinned,
      pinnedMessageId: !prev.isPinned ? prev.activeMessageId : null,
    }));
  }, []);

  const pinToMessage = useCallback((messageId: string) => {
    setState((prev) => ({
      ...prev,
      isPinned: true,
      pinnedMessageId: messageId,
      activeMessageId: messageId,
    }));
  }, []);

  const unpin = useCallback(() => {
    setState((prev) => ({
      ...prev,
      isPinned: false,
      pinnedMessageId: null,
    }));
  }, []);

  const setLatestMessageId = useCallback((messageId: string) => {
    setState((prev) => ({
      ...prev,
      latestMessageId: messageId,
    }));
  }, []);

  const jumpToLatest = useCallback(() => {
    setState((prev) => ({
      ...prev,
      isPinned: false,
      pinnedMessageId: null,
      activeMessageId: prev.latestMessageId,
    }));
  }, []);

  const autoFollowToMessage = useCallback((messageId: string, data: {
    evidence?: EvidenceItem[];
    routerDecision?: ExtendedRouterDecision;
    requestId?: string;
    traceId?: string;
    proofDigest?: string | null;
    isVerified?: boolean;
    verifiedAt?: string;
    throughputStats?: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number };
  }) => {
    // Only update if NOT pinned (use ref to avoid stale closure)
    if (isPinnedRef.current) {
      return;
    }

    setState((prev) => ({
      ...prev,
      activeMessageId: messageId,
      latestMessageId: messageId,
      currentEvidence: data.evidence ?? prev.currentEvidence,
      currentRouterDecision: data.routerDecision ?? prev.currentRouterDecision,
      currentRequestId: data.requestId ?? prev.currentRequestId,
      currentTraceId: data.traceId ?? prev.currentTraceId,
      currentProofDigest: data.proofDigest ?? prev.currentProofDigest,
      currentIsVerified: data.isVerified ?? prev.currentIsVerified,
      currentVerifiedAt: data.verifiedAt ?? prev.currentVerifiedAt,
      currentThroughputStats: data.throughputStats ?? prev.currentThroughputStats,
    }));
  }, []);

  const value: EvidenceDrawerContextValue = {
    ...state,
    openDrawer,
    closeDrawer,
    setActiveTab,
    setMessageData,
    togglePin,
    pinToMessage,
    unpin,
    setLatestMessageId,
    jumpToLatest,
    autoFollowToMessage,
  };

  return (
    <EvidenceDrawerContext.Provider value={value}>
      {children}
    </EvidenceDrawerContext.Provider>
  );
}

/**
 * Hook to access evidence drawer context
 * @throws Error if used outside of EvidenceDrawerProvider
 */
export function useEvidenceDrawer(): EvidenceDrawerContextValue {
  const context = useContext(EvidenceDrawerContext);
  if (!context) {
    throw new Error(
      'useEvidenceDrawer must be used within an EvidenceDrawerProvider'
    );
  }
  return context;
}

/**
 * Hook to access evidence drawer context without throwing
 * Returns null if outside provider
 */
export function useEvidenceDrawerOptional(): EvidenceDrawerContextValue | null {
  return useContext(EvidenceDrawerContext);
}

export default EvidenceDrawerContext;
