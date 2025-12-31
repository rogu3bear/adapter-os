// Storage quota thresholds
const WARN_THRESHOLD = 0.8; // 80%
const EVICT_THRESHOLD = 0.9; // 90%

/**
 * Storage usage status with threshold indicators
 */
export interface StorageStatus {
  used: number;
  total: number;
  percent: number;
  shouldWarn: boolean;
  shouldEvict: boolean;
}

/**
 * Get current browser storage usage status.
 * Uses the Storage API estimate() when available.
 *
 * @returns StorageStatus with usage info and threshold indicators, or null if unavailable
 */
export async function getStorageStatus(): Promise<StorageStatus | null> {
  if (typeof window === 'undefined') return null;
  if (!navigator.storage?.estimate) return null;

  try {
    const { usage = 0, quota = 0 } = await navigator.storage.estimate();
    const percent = quota > 0 ? usage / quota : 0;
    return {
      used: usage,
      total: quota,
      percent,
      shouldWarn: percent >= WARN_THRESHOLD,
      shouldEvict: percent >= EVICT_THRESHOLD,
    };
  } catch {
    return null;
  }
}

/**
 * Evict oldest entries from a localStorage key that stores versioned action data.
 * Keeps only the most recent `keepCount` entries.
 *
 * @param storageKey - The localStorage key to evict from
 * @param keepCount - Number of most recent entries to keep
 */
export function evictOldestEntries(storageKey: string, keepCount: number): void {
  if (typeof window === 'undefined') return;

  try {
    const data = window.localStorage.getItem(storageKey);
    if (!data) return;

    const parsed = JSON.parse(data);
    if (parsed && Array.isArray(parsed.actions)) {
      parsed.actions = parsed.actions.slice(-keepCount);
      parsed.timestamp = Date.now();
      window.localStorage.setItem(storageKey, JSON.stringify(parsed));
    }
  } catch {
    // Silent fail - eviction is best-effort
  }
}

export function readLocalStorage(key: string): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeLocalStorage(key: string, value: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // ignore storage failures
  }
}

/**
 * Generate a workspace-scoped storage key to prevent cross-workspace data contamination
 *
 * @param workspaceId - The workspace/tenant ID to scope the key
 * @param baseKey - The base storage key name
 * @returns A prefixed key like "workspace_<id>_<baseKey>"
 */
export function getWorkspaceScopedKey(workspaceId: string, baseKey: string): string {
  const safeWorkspaceId = workspaceId || 'default';
  return `workspace_${safeWorkspaceId}_${baseKey}`;
}
