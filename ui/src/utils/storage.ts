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
