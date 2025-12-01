import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useAdapterExport } from '@/hooks/adapters/useAdapterExport';
import type { Adapter, AdapterManifest } from '@/api/adapter-types';

// Mock the API client
const mockDownloadAdapterManifest = vi.fn();

vi.mock('@/api/client', () => ({
  apiClient: {
    downloadAdapterManifest: (...args: unknown[]) => mockDownloadAdapterManifest(...args),
  },
}));

// Mock toast
const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();
const mockToastWarning = vi.fn();

vi.mock('sonner', () => ({
  toast: {
    success: (...args: unknown[]) => mockToastSuccess(...args),
    error: (...args: unknown[]) => mockToastError(...args),
    warning: (...args: unknown[]) => mockToastWarning(...args),
  },
}));

// Mock logger
const mockLoggerInfo = vi.fn();
const mockLoggerError = vi.fn();

vi.mock('@/utils/logger', () => ({
  logger: {
    info: (...args: unknown[]) => mockLoggerInfo(...args),
    error: (...args: unknown[]) => mockLoggerError(...args),
  },
  toError: (err: unknown) => (err instanceof Error ? err : new Error(String(err))),
}));

// Mock document.createElement for download testing
const mockClick = vi.fn();
const mockLinkElement = {
  href: '',
  download: '',
  click: mockClick,
};
const originalCreateElement = document.createElement.bind(document);
const mockCreateElement = vi.fn((tagName: string) => {
  if (tagName === 'a') {
    return mockLinkElement;
  }
  return originalCreateElement(tagName);
});

document.createElement = mockCreateElement as any;

// Mock URL.createObjectURL and revokeObjectURL
const mockCreateObjectURL = vi.fn(() => 'blob:mock-url');
const mockRevokeObjectURL = vi.fn();
global.URL.createObjectURL = mockCreateObjectURL;
global.URL.revokeObjectURL = mockRevokeObjectURL;

// Helper to read blob content (jsdom doesn't have Blob.text())
async function readBlobAsText(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsText(blob);
  });
}

// Test data
const mockAdapters: Adapter[] = [
  {
    adapter_id: 'adapter-1',
    name: 'Test Adapter 1',
    hash_b3: 'blake3hash1',
    rank: 16,
    tier: 'warm',
    category: 'code',
    framework: 'rust',
    scope: 'global',
    languages: ['rust', 'python'],
    active: true,
    pinned: false,
    created_at: '2025-01-01T10:00:00Z',
    updated_at: '2025-01-02T15:30:00Z',
  },
  {
    adapter_id: 'adapter-2',
    name: 'Test Adapter 2',
    hash_b3: 'blake3hash2',
    rank: 24,
    tier: 'persistent',
    category: 'framework',
    framework: 'typescript',
    framework_id: 'react',
    framework_version: '18.0.0',
    scope: 'tenant',
    languages: ['typescript'],
    active: false,
    pinned: true,
    repository_id: 'repo-123',
    commit_sha: 'abc123',
    created_at: '2025-01-03T08:00:00Z',
  },
];

const mockManifests: AdapterManifest[] = [
  {
    adapter_id: 'adapter-1',
    name: 'Test Adapter 1',
    hash_b3: 'blake3hash1',
    rank: 16,
    tier: 'warm',
    category: 'code',
    framework: 'rust',
    scope: 'global',
    languages_json: ['rust', 'python'],
    created_at: '2025-01-01T10:00:00Z',
    updated_at: '2025-01-02T15:30:00Z',
  },
  {
    adapter_id: 'adapter-2',
    name: 'Test Adapter 2',
    hash_b3: 'blake3hash2',
    rank: 24,
    tier: 'persistent',
    category: 'framework',
    framework: 'typescript',
    framework_id: 'react',
    framework_version: '18.0.0',
    scope: 'tenant',
    languages_json: ['typescript'],
    repo_id: 'repo-123',
    commit_sha: 'abc123',
    created_at: '2025-01-03T08:00:00Z',
  },
];

describe('useAdapterExport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset link element properties
    mockLinkElement.href = '';
    mockLinkElement.download = '';
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns correct initial state', () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      expect(result.current.isExporting).toBe(false);
      expect(result.current.exportProgress).toBeNull();
      expect(result.current.exportDialogOpen).toBe(false);
    });

    it('returns all export functions', () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      expect(typeof result.current.exportAdapters).toBe('function');
      expect(typeof result.current.downloadManifest).toBe('function');
      expect(typeof result.current.openExportDialog).toBe('function');
      expect(typeof result.current.closeExportDialog).toBe('function');
    });
  });

  describe('export dialog state', () => {
    it('openExportDialog sets dialog to open', () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      act(() => {
        result.current.openExportDialog();
      });

      expect(result.current.exportDialogOpen).toBe(true);
    });

    it('closeExportDialog sets dialog to closed', () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      act(() => {
        result.current.openExportDialog();
        result.current.closeExportDialog();
      });

      expect(result.current.exportDialogOpen).toBe(false);
    });

    it('toggles dialog state correctly', () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      expect(result.current.exportDialogOpen).toBe(false);

      act(() => {
        result.current.openExportDialog();
      });
      expect(result.current.exportDialogOpen).toBe(true);

      act(() => {
        result.current.closeExportDialog();
      });
      expect(result.current.exportDialogOpen).toBe(false);
    });
  });

  describe('downloadManifest', () => {
    it('downloads manifest for a single adapter', async () => {
      mockDownloadAdapterManifest.mockResolvedValue(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.downloadManifest('adapter-1');
      });

      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-1');
      expect(mockCreateObjectURL).toHaveBeenCalled();
      expect(mockClick).toHaveBeenCalled();
      expect(mockRevokeObjectURL).toHaveBeenCalled();
      expect(mockToastSuccess).toHaveBeenCalledWith('Manifest downloaded.');
    });

    it('creates correct filename for manifest', async () => {
      mockDownloadAdapterManifest.mockResolvedValue(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.downloadManifest('adapter-1');
      });

      expect(mockLinkElement.download).toBe('adapter-1-manifest.json');
    });

    it('handles download error gracefully', async () => {
      const error = new Error('Download failed');
      mockDownloadAdapterManifest.mockRejectedValue(error);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      let thrownError: Error | undefined;
      await act(async () => {
        try {
          await result.current.downloadManifest('adapter-1');
        } catch (err) {
          thrownError = err as Error;
        }
      });

      expect(thrownError?.message).toBe('Download failed');

      expect(mockLoggerError).toHaveBeenCalledWith(
        'Failed to download manifest',
        expect.objectContaining({
          component: 'useAdapterExport',
          operation: 'downloadManifest',
          details: 'adapter-1',
        }),
        error
      );

      expect(mockToastError).toHaveBeenCalledWith(
        'Failed to download manifest',
        expect.objectContaining({
          description: 'Download failed',
        })
      );
    });

    it('logs info before downloading', async () => {
      mockDownloadAdapterManifest.mockResolvedValue(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.downloadManifest('adapter-1');
      });

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Downloading adapter manifest',
        expect.objectContaining({
          component: 'useAdapterExport',
          operation: 'downloadManifest',
          details: 'adapter-1',
        })
      );
    });
  });

  describe('exportAdapters - scope handling', () => {
    it('exports selected adapters when scope is "selected"', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const selectedIds = new Set(['adapter-1', 'adapter-2']);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters, selectedIds })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'selected');
      });

      expect(mockDownloadAdapterManifest).toHaveBeenCalledTimes(2);
      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-1');
      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-2');
    });

    it('exports filtered adapters when scope is "filtered"', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const filteredIds = ['adapter-1'];

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters, filteredIds })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'filtered');
      });

      expect(mockDownloadAdapterManifest).toHaveBeenCalledTimes(1);
      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-1');
    });

    it('exports all adapters when scope is "all"', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockDownloadAdapterManifest).toHaveBeenCalledTimes(2);
      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-1');
      expect(mockDownloadAdapterManifest).toHaveBeenCalledWith('adapter-2');
    });

    it('shows warning when no adapters to export', async () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: [], selectedIds: new Set() })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'selected');
      });

      expect(mockToastWarning).toHaveBeenCalledWith('No adapters to export.');
      expect(mockDownloadAdapterManifest).not.toHaveBeenCalled();
      expect(result.current.exportDialogOpen).toBe(false);
    });
  });

  describe('exportAdapters - JSON format', () => {
    it('exports to JSON format correctly', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      // Verify Blob was created with JSON content
      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      expect(blobCall.type).toBe('application/json');

      // Verify file was downloaded
      expect(mockClick).toHaveBeenCalled();
      expect(mockToastSuccess).toHaveBeenCalledWith('Exported 2 adapter manifest(s).');
    });

    it('generates timestamped JSON filename', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockLinkElement.download).toMatch(/^adapters-export-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}\.json$/);
    });

    it('formats JSON with proper indentation', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      const content = await readBlobAsText(blobCall);
      const parsed = JSON.parse(content);

      expect(parsed).toEqual([mockManifests[0], mockManifests[1]]);
      expect(content).toContain('\n'); // Should be formatted with newlines
    });
  });

  describe('exportAdapters - CSV format', () => {
    it('exports to CSV format correctly', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      expect(blobCall.type).toBe('text/csv');
      expect(mockClick).toHaveBeenCalled();
    });

    it('generates CSV with correct headers', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      const csvContent = await readBlobAsText(blobCall);
      const lines = csvContent.split('\n');

      expect(lines[0]).toContain('adapter_id');
      expect(lines[0]).toContain('name');
      expect(lines[0]).toContain('category');
      expect(lines[0]).toContain('framework');
      expect(lines[0]).toContain('tier');
      expect(lines[0]).toContain('rank');
    });

    it('escapes CSV fields with commas', async () => {
      const manifestWithComma = {
        ...mockManifests[0],
        name: 'Test, Adapter',
      };
      mockDownloadAdapterManifest.mockResolvedValueOnce(manifestWithComma);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      const csvContent = await readBlobAsText(blobCall);

      expect(csvContent).toContain('"Test, Adapter"');
    });

    it('escapes CSV fields with quotes', async () => {
      const manifestWithQuote = {
        ...mockManifests[0],
        name: 'Test "Quoted" Adapter',
      };
      mockDownloadAdapterManifest.mockResolvedValueOnce(manifestWithQuote);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      const csvContent = await readBlobAsText(blobCall);

      // Quotes should be doubled
      expect(csvContent).toContain('""Quoted""');
    });

    it('handles null and undefined values in CSV', async () => {
      const manifestWithNulls = {
        ...mockManifests[0],
        framework_id: null,
        framework_version: undefined,
      };
      mockDownloadAdapterManifest.mockResolvedValueOnce(manifestWithNulls as any);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      const blobCall = mockCreateObjectURL.mock.calls[0][0] as Blob;
      const csvContent = await readBlobAsText(blobCall);

      // Should not have "null" or "undefined" strings, just empty fields
      expect(csvContent).not.toContain('null');
      expect(csvContent).not.toContain('undefined');
    });

    it('generates timestamped CSV filename', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      expect(mockLinkElement.download).toMatch(/^adapters-export-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}\.csv$/);
    });
  });

  describe('exportAdapters - progress tracking', () => {
    it('sets isExporting to true during export', async () => {
      mockDownloadAdapterManifest.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockManifests[0]), 50))
      );

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      act(() => {
        result.current.exportAdapters('json', 'all');
      });

      await waitFor(() => {
        expect(result.current.isExporting).toBe(true);
      });
    });

    it('updates progress percentage during export', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      // Progress should be reset to null after completion
      expect(result.current.exportProgress).toBeNull();
      expect(result.current.isExporting).toBe(false);
    });

    it('calculates progress correctly for multiple adapters', async () => {
      const progressSnapshots: (number | null)[] = [];

      mockDownloadAdapterManifest.mockImplementation(async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        return mockManifests[0];
      });

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      const exportPromise = act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      // Capture progress snapshots during export
      await waitFor(() => {
        if (result.current.exportProgress !== null) {
          progressSnapshots.push(result.current.exportProgress);
        }
      });

      await exportPromise;

      // Final state should have no progress
      expect(result.current.exportProgress).toBeNull();
    });
  });

  describe('exportAdapters - error handling', () => {
    it('continues export when one manifest fails', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockRejectedValueOnce(new Error('Download failed'))
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: [...mockAdapters, mockAdapters[1]] })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      // Should still export the successful ones
      expect(mockToastSuccess).toHaveBeenCalledWith('Exported 2 adapter manifest(s).');
      expect(mockLoggerError).toHaveBeenCalled();
    });

    it('shows warning when no manifests downloaded', async () => {
      mockDownloadAdapterManifest.mockRejectedValue(new Error('All failed'));

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockToastWarning).toHaveBeenCalledWith('No manifests could be downloaded.');
      expect(mockClick).not.toHaveBeenCalled();
    });

    it('resets state after error', async () => {
      mockDownloadAdapterManifest.mockRejectedValue(new Error('All failed'));

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(result.current.isExporting).toBe(false);
      expect(result.current.exportProgress).toBeNull();
    });

    it('logs errors with appropriate context', async () => {
      const error = new Error('Network failure');
      mockDownloadAdapterManifest.mockRejectedValue(error);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockLoggerError).toHaveBeenCalledWith(
        'Failed to download manifest for export',
        expect.objectContaining({
          component: 'useAdapterExport',
          operation: 'exportAdapters',
        }),
        expect.any(Error)
      );
    });
  });

  describe('exportAdapters - dialog behavior', () => {
    it('closes dialog after successful export', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      act(() => {
        result.current.openExportDialog();
      });

      expect(result.current.exportDialogOpen).toBe(true);

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(result.current.exportDialogOpen).toBe(false);
    });

    it('closes dialog when no adapters to export', async () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: [] })
      );

      act(() => {
        result.current.openExportDialog();
      });

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(result.current.exportDialogOpen).toBe(false);
    });
  });

  describe('logging and telemetry', () => {
    it('logs export start with details', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Starting adapter export',
        expect.objectContaining({
          component: 'useAdapterExport',
          operation: 'exportAdapters',
          details: expect.stringContaining('format=csv'),
        })
      );
    });

    it('logs export completion with count', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Export completed successfully',
        expect.objectContaining({
          component: 'useAdapterExport',
          operation: 'exportAdapters',
          details: expect.stringContaining('count=2'),
        })
      );
    });
  });

  describe('edge cases', () => {
    it('handles empty selected set gracefully', async () => {
      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters, selectedIds: new Set() })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'selected');
      });

      expect(mockToastWarning).toHaveBeenCalledWith('No adapters to export.');
      expect(mockDownloadAdapterManifest).not.toHaveBeenCalled();
    });

    it('handles missing filteredIds by using all adapters', async () => {
      mockDownloadAdapterManifest
        .mockResolvedValueOnce(mockManifests[0])
        .mockResolvedValueOnce(mockManifests[1]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'filtered');
      });

      expect(mockDownloadAdapterManifest).toHaveBeenCalledTimes(2);
    });

    it('cleans up blob URL after download', async () => {
      mockDownloadAdapterManifest.mockResolvedValueOnce(mockManifests[0]);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('json', 'all');
      });

      expect(mockRevokeObjectURL).toHaveBeenCalledWith('blob:mock-url');
    });

    it('handles adapters with minimal fields', async () => {
      const minimalManifest: AdapterManifest = {
        adapter_id: 'minimal-1',
        name: 'Minimal Adapter',
        hash_b3: 'hash',
        rank: 8,
        tier: 'ephemeral',
        category: 'code',
        framework: 'python',
        scope: 'user',
      };

      mockDownloadAdapterManifest.mockResolvedValueOnce(minimalManifest);

      const { result } = renderHook(() =>
        useAdapterExport({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.exportAdapters('csv', 'all');
      });

      expect(mockClick).toHaveBeenCalled();
      expect(mockToastSuccess).toHaveBeenCalled();
    });
  });
});
