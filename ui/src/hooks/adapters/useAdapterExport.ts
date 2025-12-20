/**
 * useAdapterExport - Hook for exporting adapter data
 *
 * Handles exporting adapter manifests to CSV or JSON formats with proper
 * escaping, progress tracking, and download management.
 *
 * @example Basic usage
 * ```tsx
 * function AdapterList() {
 *   const { adapters, selectedIds } = useAdapters();
 *   const { exportAdapters, downloadManifest, isExporting, exportProgress } =
 *     useAdapterExport({ adapters, selectedIds });
 *
 *   return (
 *     <div>
 *       <Button onClick={() => exportAdapters('csv', 'selected')} disabled={isExporting}>
 *         Export to CSV {isExporting && `(${exportProgress}%)`}
 *       </Button>
 *       <Button onClick={() => downloadManifest('adapter-123')}>
 *         Download Manifest
 *       </Button>
 *     </div>
 *   );
 * }
 * ```
 *
 * @example With export dialog
 * ```tsx
 * function AdapterExportDialog() {
 *   const { exportAdapters, exportDialogOpen, openExportDialog, closeExportDialog } =
 *     useAdapterExport({ adapters });
 *
 *   return (
 *     <>
 *       <Button onClick={openExportDialog}>Export</Button>
 *       <Dialog open={exportDialogOpen} onOpenChange={closeExportDialog}>
 *         <DialogContent>
 *           <Button onClick={() => exportAdapters('json', 'all')}>
 *             Export All as JSON
 *           </Button>
 *         </DialogContent>
 *       </Dialog>
 *     </>
 *   );
 * }
 * ```
 */

import { useState, useCallback } from 'react';
import { apiClient } from '@/api/services';
import type { Adapter, AdapterManifest } from '@/api/adapter-types';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';

export type ExportFormat = 'csv' | 'json';
export type ExportScope = 'selected' | 'filtered' | 'all';

export interface UseAdapterExportOptions {
  /**
   * Full list of available adapters
   */
  adapters: Adapter[];

  /**
   * Set of currently selected adapter IDs
   * Used when scope is 'selected'
   */
  selectedIds?: Set<string>;

  /**
   * Array of filtered adapter IDs
   * Used when scope is 'filtered'
   */
  filteredIds?: string[];
}

export interface UseAdapterExportReturn {
  /**
   * Export adapters to CSV or JSON format
   * @param format - Export format ('csv' or 'json')
   * @param scope - Which adapters to export ('selected', 'filtered', or 'all')
   */
  exportAdapters: (format: ExportFormat, scope: ExportScope) => Promise<void>;

  /**
   * Download manifest for a specific adapter
   * @param adapterId - Adapter ID to download manifest for
   */
  downloadManifest: (adapterId: string) => Promise<void>;

  /**
   * Whether an export operation is currently in progress
   */
  isExporting: boolean;

  /**
   * Current export progress (0-100), null if not exporting
   */
  exportProgress: number | null;

  /**
   * Whether the export dialog is open
   */
  exportDialogOpen: boolean;

  /**
   * Open the export dialog
   */
  openExportDialog: () => void;

  /**
   * Close the export dialog
   */
  closeExportDialog: () => void;
}

/**
 * CSV header fields for adapter export
 * Order matters for CSV column ordering
 */
const CSV_HEADERS = [
  // Primary identifiers
  'adapter_id',
  'name',

  // Content classification
  'category',
  'scope',
  'intent',
  'languages',

  // Technical details
  'framework',
  'framework_id',
  'framework_version',
  'blake3_hash',

  // Quality metrics
  'tier',
  'rank',

  // Provenance tracking
  'repository_id',
  'commit_sha',

  // Metadata
  'created_at',
  'updated_at',
] as const;

/**
 * Escape a CSV field value
 * Handles commas, quotes, and newlines
 */
function escapeCsvField(value: unknown): string {
  if (value === null || value === undefined) {
    return '';
  }

  const stringValue = String(value);

  // If the value contains comma, quote, or newline, wrap in quotes
  // and escape internal quotes by doubling them
  if (stringValue.includes(',') || stringValue.includes('"') || stringValue.includes('\n')) {
    return `"${stringValue.replace(/"/g, '""')}"`;
  }

  return stringValue;
}

/**
 * Map CSV header to manifest field name
 * Some headers use user-friendly names that need to be mapped to API fields
 */
function mapHeaderToField(header: string): string {
  const fieldMap: Record<string, string> = {
    'languages': 'languages_json',
    'blake3_hash': 'hash_b3',
    'repository_id': 'repo_id',
  };

  return fieldMap[header] || header;
}

/**
 * Convert manifests to CSV format
 */
function manifestsToCsv(manifests: AdapterManifest[]): string {
  if (manifests.length === 0) {
    return '';
  }

  // Header row
  const headerRow = CSV_HEADERS.join(',');

  // Data rows
  const dataRows = manifests.map(manifest => {
    return CSV_HEADERS.map(header => {
      const fieldName = mapHeaderToField(header);
      const value = (manifest as unknown as Record<string, unknown>)[fieldName];
      return escapeCsvField(value);
    }).join(',');
  });

  return [headerRow, ...dataRows].join('\n');
}

/**
 * Create a download link and trigger download
 */
function downloadFile(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);

  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();

  // Clean up the blob URL
  URL.revokeObjectURL(url);
}

/**
 * Generate filename with timestamp
 */
function generateFilename(format: ExportFormat): string {
  const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
  return `adapters-export-${timestamp}.${format}`;
}

export function useAdapterExport({
  adapters,
  selectedIds = new Set(),
  filteredIds,
}: UseAdapterExportOptions): UseAdapterExportReturn {
  const [isExporting, setIsExporting] = useState(false);
  const [exportProgress, setExportProgress] = useState<number | null>(null);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);

  /**
   * Get adapter IDs based on scope
   */
  const getAdapterIds = useCallback((scope: ExportScope): string[] => {
    switch (scope) {
      case 'selected':
        return Array.from(selectedIds);
      case 'filtered':
        return filteredIds || adapters.map(a => a.adapter_id);
      case 'all':
        return adapters.map(a => a.adapter_id);
      default:
        return [];
    }
  }, [adapters, selectedIds, filteredIds]);

  /**
   * Download manifest for a specific adapter
   */
  const downloadManifest = useCallback(async (adapterId: string): Promise<void> => {
    try {
      logger.info('Downloading adapter manifest', {
        component: 'useAdapterExport',
        operation: 'downloadManifest',
        details: adapterId,
      });

      const manifest = await apiClient.downloadAdapterManifest(adapterId);
      const content = JSON.stringify(manifest, null, 2);
      const filename = `${adapterId}-manifest.json`;

      downloadFile(content, filename, 'application/json');

      toast.success('Manifest downloaded.');
    } catch (err) {
      const error = toError(err);
      logger.error('Failed to download manifest', {
        component: 'useAdapterExport',
        operation: 'downloadManifest',
        details: adapterId,
      }, error);

      toast.error('Failed to download manifest', {
        description: error.message,
      });

      throw error;
    }
  }, []);

  /**
   * Export adapters to specified format
   */
  const exportAdapters = useCallback(async (
    format: ExportFormat,
    scope: ExportScope
  ): Promise<void> => {
    try {
      setIsExporting(true);
      setExportProgress(0);

      const adapterIds = getAdapterIds(scope);

      if (adapterIds.length === 0) {
        toast.warning('No adapters to export.');
        setExportDialogOpen(false);
        return;
      }

      logger.info('Starting adapter export', {
        component: 'useAdapterExport',
        operation: 'exportAdapters',
        details: `format=${format}, scope=${scope}, count=${adapterIds.length}`,
      });

      // Download all manifests
      const manifests: AdapterManifest[] = [];

      for (let i = 0; i < adapterIds.length; i++) {
        const adapterId = adapterIds[i];

        try {
          const manifest = await apiClient.downloadAdapterManifest(adapterId);
          manifests.push(manifest);

          // Update progress
          const progress = Math.round(((i + 1) / adapterIds.length) * 100);
          setExportProgress(progress);
        } catch (err) {
          logger.error('Failed to download manifest for export', {
            component: 'useAdapterExport',
            operation: 'exportAdapters',
            details: adapterId,
          }, toError(err));
          // Continue with other adapters even if one fails
        }
      }

      if (manifests.length === 0) {
        toast.warning('No manifests could be downloaded.');
        return;
      }

      // Generate export file
      const filename = generateFilename(format);

      if (format === 'json') {
        const content = JSON.stringify(manifests, null, 2);
        downloadFile(content, filename, 'application/json');
      } else {
        // CSV export
        const content = manifestsToCsv(manifests);
        downloadFile(content, filename, 'text/csv');
      }

      toast.success(`Exported ${manifests.length} adapter manifest(s).`);
      setExportDialogOpen(false);

      logger.info('Export completed successfully', {
        component: 'useAdapterExport',
        operation: 'exportAdapters',
        details: `format=${format}, count=${manifests.length}`,
      });
    } catch (err) {
      const error = toError(err);
      logger.error('Failed to export adapters', {
        component: 'useAdapterExport',
        operation: 'exportAdapters',
      }, error);

      toast.error('Failed to export adapters', {
        description: error.message,
      });

      throw error;
    } finally {
      setIsExporting(false);
      setExportProgress(null);
    }
  }, [getAdapterIds]);

  const openExportDialog = useCallback(() => {
    setExportDialogOpen(true);
  }, []);

  const closeExportDialog = useCallback(() => {
    setExportDialogOpen(false);
  }, []);

  return {
    exportAdapters,
    downloadManifest,
    isExporting,
    exportProgress,
    exportDialogOpen,
    openExportDialog,
    closeExportDialog,
  };
}
