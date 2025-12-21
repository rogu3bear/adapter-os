export interface PreselectParams {
  adapterId?: string;
  datasetId?: string;
}

/**
 * Parse query (and optional hash) for preselection params.
 * Keeps names consistent across Adapters, Training, and Router Config.
 */
export function parsePreselectParams(search: string, hash?: string): PreselectParams {
  const params = new URLSearchParams(search);
  const adapterId = params.get('adapterId') || undefined;
  const datasetId = params.get('datasetId') || undefined;

  // Hash-based fallback if ever used (keeps backward compatibility)
  if (!adapterId && hash) {
    const hashParams = new URLSearchParams(hash.replace(/^#/, ''));
    const hashAdapterId = hashParams.get('adapterId');
    if (hashAdapterId) {
      return { adapterId: hashAdapterId, datasetId };
    }
  }

  return { adapterId, datasetId };
}

/**
 * Remove provided keys from a URLSearchParams string and return the new search string.
 */
export function removeParams(search: string, keys: string[]): string {
  const params = new URLSearchParams(search);
  keys.forEach((key) => params.delete(key));
  const next = params.toString();
  return next ? `?${next}` : '';
}

