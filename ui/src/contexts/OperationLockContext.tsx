/**
 * Operation Lock Context
 *
 * Prevents workspace switching during critical operations like streaming,
 * training, or file uploads. This ensures data integrity by blocking
 * workspace changes that could cause mixed-tenant evidence.
 */

import React, { createContext, useContext, useState, useCallback, useRef } from 'react';

/**
 * Types of operations that can acquire a lock
 */
export type OperationLockType = 'streaming' | 'training' | 'upload' | 'mutation';

/**
 * Represents an active operation lock
 */
export interface OperationLock {
  id: string;
  type: OperationLockType;
  description: string;
  workspaceId: string;
  acquiredAt: number;
}

/**
 * Context value for operation locks
 */
interface OperationLockContextValue {
  /** All currently active locks */
  locks: OperationLock[];
  /** True if any locks are active */
  isLocked: boolean;
  /** Acquire a lock for a critical operation. Returns the lock ID for later release. */
  acquireLock: (type: OperationLockType, description: string, workspaceId: string) => string;
  /** Release a lock by its ID */
  releaseLock: (lockId: string) => void;
  /** Get descriptions of all blocking operations */
  getBlockingOperations: () => string[];
  /** Check if a specific lock type is active */
  hasLockType: (type: OperationLockType) => boolean;
}

const OperationLockContext = createContext<OperationLockContextValue | undefined>(undefined);

let lockIdCounter = 0;

/**
 * Generate a unique lock ID
 */
function generateLockId(): string {
  lockIdCounter += 1;
  return `lock-${Date.now()}-${lockIdCounter}`;
}

export function OperationLockProvider({ children }: { children: React.ReactNode }) {
  const [locks, setLocks] = useState<OperationLock[]>([]);
  const locksRef = useRef<OperationLock[]>([]);

  // Keep ref in sync for callbacks
  locksRef.current = locks;

  const acquireLock = useCallback((
    type: OperationLockType,
    description: string,
    workspaceId: string
  ): string => {
    const lockId = generateLockId();
    const lock: OperationLock = {
      id: lockId,
      type,
      description,
      workspaceId,
      acquiredAt: Date.now(),
    };

    setLocks(prev => [...prev, lock]);
    return lockId;
  }, []);

  const releaseLock = useCallback((lockId: string): void => {
    setLocks(prev => prev.filter(lock => lock.id !== lockId));
  }, []);

  const getBlockingOperations = useCallback((): string[] => {
    return locksRef.current.map(lock => lock.description);
  }, []);

  const hasLockType = useCallback((type: OperationLockType): boolean => {
    return locksRef.current.some(lock => lock.type === type);
  }, []);

  const isLocked = locks.length > 0;

  const contextValue: OperationLockContextValue = {
    locks,
    isLocked,
    acquireLock,
    releaseLock,
    getBlockingOperations,
    hasLockType,
  };

  return (
    <OperationLockContext.Provider value={contextValue}>
      {children}
    </OperationLockContext.Provider>
  );
}

/**
 * Hook to access operation lock context
 */
export function useOperationLock(): OperationLockContextValue {
  const context = useContext(OperationLockContext);
  if (context === undefined) {
    throw new Error('useOperationLock must be used within an OperationLockProvider');
  }
  return context;
}

/**
 * Hook for components that optionally use operation lock
 * Returns null if not within provider (for backward compatibility)
 */
export function useOperationLockOptional(): OperationLockContextValue | null {
  return useContext(OperationLockContext) ?? null;
}

export default OperationLockContext;
