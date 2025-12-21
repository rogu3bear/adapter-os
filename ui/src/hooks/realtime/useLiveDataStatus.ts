/**
 * useLiveDataStatus - Global aggregator for live data connection status
 *
 * Provides a centralized view of all SSE stream statuses for the
 * global connection indicator in the app header.
 */

import { createContext, useContext, useState, useCallback, useMemo, ReactNode } from 'react';
import React from 'react';

// ============================================================================
// Types
// ============================================================================

export type OverallConnectionStatus = 'live' | 'partial' | 'polling' | 'offline';

export interface StreamStatus {
  connected: boolean;
  lastUpdate: Date | null;
  error: Error | null;
  reconnecting: boolean;
  reconnectAttempt?: number;
}

export interface GlobalConnectionStatus {
  /** Overall connection status across all streams */
  overall: OverallConnectionStatus;

  /** Individual stream statuses */
  streams: Record<string, StreamStatus>;

  /** Count of connected streams */
  connectedCount: number;

  /** Total registered streams */
  totalStreams: number;

  /** Reconnect all disconnected streams */
  reconnectAll: () => void;

  /** Register a stream */
  registerStream: (id: string, status: StreamStatus) => void;

  /** Unregister a stream */
  unregisterStream: (id: string) => void;

  /** Update a stream status */
  updateStream: (id: string, status: Partial<StreamStatus>) => void;
}

// ============================================================================
// Context
// ============================================================================

const LiveDataStatusContext = createContext<GlobalConnectionStatus | null>(null);

// ============================================================================
// Provider
// ============================================================================

interface LiveDataStatusProviderProps {
  children: ReactNode;
}

export function LiveDataStatusProvider({ children }: LiveDataStatusProviderProps) {
  const [streams, setStreams] = useState<Record<string, StreamStatus>>({});
  const [reconnectCallbacks, setReconnectCallbacks] = useState<Record<string, () => void>>({});

  const registerStream = useCallback((id: string, status: StreamStatus, onReconnect?: () => void) => {
    setStreams((prev) => ({ ...prev, [id]: status }));
    if (onReconnect) {
      setReconnectCallbacks((prev) => ({ ...prev, [id]: onReconnect }));
    }
  }, []);

  const unregisterStream = useCallback((id: string) => {
    setStreams((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    setReconnectCallbacks((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const updateStream = useCallback((id: string, status: Partial<StreamStatus>) => {
    setStreams((prev) => {
      if (!prev[id]) return prev;
      return { ...prev, [id]: { ...prev[id], ...status } };
    });
  }, []);

  const reconnectAll = useCallback(() => {
    Object.values(reconnectCallbacks).forEach((callback) => {
      try {
        callback();
      } catch (e) {
        // Ignore errors during reconnect
      }
    });
  }, [reconnectCallbacks]);

  const connectedCount = useMemo(
    () => Object.values(streams).filter((s) => s.connected).length,
    [streams]
  );

  const totalStreams = useMemo(() => Object.keys(streams).length, [streams]);

  const overall: OverallConnectionStatus = useMemo(() => {
    if (totalStreams === 0) return 'offline';
    if (connectedCount === totalStreams) return 'live';
    if (connectedCount > 0) return 'partial';
    // Check if any are reconnecting
    const anyReconnecting = Object.values(streams).some((s) => s.reconnecting);
    if (anyReconnecting) return 'polling';
    return 'offline';
  }, [totalStreams, connectedCount, streams]);

  const value: GlobalConnectionStatus = useMemo(
    () => ({
      overall,
      streams,
      connectedCount,
      totalStreams,
      reconnectAll,
      registerStream,
      unregisterStream,
      updateStream,
    }),
    [overall, streams, connectedCount, totalStreams, reconnectAll, registerStream, unregisterStream, updateStream]
  );

  return React.createElement(LiveDataStatusContext.Provider, { value }, children);
}

// ============================================================================
// Hook
// ============================================================================

export function useLiveDataStatus(): GlobalConnectionStatus {
  const context = useContext(LiveDataStatusContext);
  if (!context) {
    // Return a default object if used outside provider
    return {
      overall: 'offline',
      streams: {},
      connectedCount: 0,
      totalStreams: 0,
      reconnectAll: () => {},
      registerStream: () => {},
      unregisterStream: () => {},
      updateStream: () => {},
    };
  }
  return context;
}

// ============================================================================
// Registration Hook
// ============================================================================

/**
 * Hook to register a stream with the global status tracker.
 * Call this from components that use useLiveData to report their status.
 */
export function useRegisterLiveDataStream(
  id: string,
  status: StreamStatus,
  onReconnect?: () => void
) {
  const { registerStream, unregisterStream, updateStream } = useLiveDataStatus();

  // Register on mount, unregister on unmount
  React.useEffect(() => {
    registerStream(id, status);
    return () => unregisterStream(id);
  }, [id, status, registerStream, unregisterStream]);

  // Update status when it changes
  React.useEffect(() => {
    updateStream(id, status);
  }, [id, status, updateStream]);

  // Store reconnect callback
  React.useEffect(() => {
    if (onReconnect) {
      // Re-register with callback
      registerStream(id, status);
    }
  }, [id, onReconnect, registerStream, status]);
}

export default useLiveDataStatus;
