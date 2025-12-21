import { useSyncExternalStore } from 'react';

type BackendReachabilityStatus = 'unknown' | 'online' | 'offline';

export interface BackendReachabilityError {
  at: number;
  method?: string;
  path?: string;
  status?: number;
  error: unknown;
}

export interface BackendReachabilitySnapshot {
  status: BackendReachabilityStatus;
  lastOkAt?: number;
  lastError?: BackendReachabilityError;
}

type Listener = () => void;

let snapshot: BackendReachabilitySnapshot = { status: 'unknown' };
const listeners = new Set<Listener>();

function emit() {
  listeners.forEach((listener) => listener());
}

export function getBackendReachabilitySnapshot(): BackendReachabilitySnapshot {
  return snapshot;
}

export function subscribeBackendReachability(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useBackendReachability(): BackendReachabilitySnapshot {
  return useSyncExternalStore(
    subscribeBackendReachability,
    getBackendReachabilitySnapshot,
    getBackendReachabilitySnapshot,
  );
}

export function markBackendReachable(): void {
  const now = Date.now();
  if (snapshot.status === 'online' && snapshot.lastOkAt && now - snapshot.lastOkAt < 1000) {
    return;
  }
  snapshot = {
    status: 'online',
    lastOkAt: now,
    lastError: undefined,
  };
  emit();
}

export function markBackendUnreachable(error: unknown, context?: { method?: string; path?: string; status?: number }): void {
  const now = Date.now();
  if (snapshot.status === 'offline' && snapshot.lastError && now - snapshot.lastError.at < 1000) {
    const prev = snapshot.lastError;
    const prevMessage = prev.error instanceof Error ? prev.error.message : String(prev.error);
    const nextMessage = error instanceof Error ? error.message : String(error);
    if (
      prev.method === context?.method &&
      prev.path === context?.path &&
      prev.status === context?.status &&
      prevMessage === nextMessage
    ) {
      return;
    }
  }

  snapshot = {
    status: 'offline',
    lastOkAt: snapshot.lastOkAt,
    lastError: {
      at: now,
      method: context?.method,
      path: context?.path,
      status: context?.status,
      error,
    },
  };
  emit();
}

export function clearBackendUnreachable(): void {
  if (snapshot.status === 'unknown') return;
  snapshot = { status: 'unknown', lastOkAt: snapshot.lastOkAt, lastError: undefined };
  emit();
}
