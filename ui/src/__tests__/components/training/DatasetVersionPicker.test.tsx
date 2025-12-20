import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DatasetVersionPicker } from '@/components/training/DatasetVersionPicker';

// Mock API client
const mockListDatasetVersions = vi.hoisted(() => vi.fn());

vi.mock('@/api/services', () => {
  const methods = { listDatasetVersions: mockListDatasetVersions };
  return { default: methods, apiClient: methods };
});

const createTestQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
        staleTime: 0,
      },
    },
  });

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createTestQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>
  );
}

const mockVersions = [
  {
    dataset_version_id: 'v1-uuid',
    version_number: 1,
    version_label: 'Initial',
    trust_state: 'allowed' as const,
    hash_b3: 'abc123def456789012345678901234567890',
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    dataset_version_id: 'v2-uuid',
    version_number: 2,
    version_label: 'Updated',
    trust_state: 'allowed_with_warning' as const,
    hash_b3: 'def456abc789012345678901234567890123',
    created_at: '2025-01-15T00:00:00Z',
  },
  {
    dataset_version_id: 'v3-uuid',
    version_number: 3,
    version_label: null,
    trust_state: 'blocked' as const,
    hash_b3: 'ghi789jkl012345678901234567890123456',
    created_at: '2025-02-01T00:00:00Z',
  },
];

describe('DatasetVersionPicker', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should render nothing when no datasetId provided', () => {
    const onVersionSelect = vi.fn();
    const { container } = renderWithProviders(
      <DatasetVersionPicker
        datasetId=""
        onVersionSelect={onVersionSelect}
      />
    );

    expect(container.firstChild).toBeNull();
  });

  it('should show loading skeleton while fetching versions', () => {
    mockListDatasetVersions.mockImplementation(
      () => new Promise(() => {}) // Never resolves
    );

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        onVersionSelect={vi.fn()}
      />
    );

    expect(screen.getByText('Dataset Version')).toBeTruthy();
    // Skeleton should be present during loading
  });

  it('should show error message when fetch fails', async () => {
    mockListDatasetVersions.mockRejectedValue(new Error('Network error'));

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Failed to load versions')).toBeTruthy();
    });
  });

  it('should show message when no versions available', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: [] });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('No versions available for this dataset')).toBeTruthy();
    });
  });

  it('should render version selector when versions exist', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v1-uuid"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByRole('combobox')).toBeTruthy();
    });
  });

  it('should auto-select first version when none selected', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });
    const onVersionSelect = vi.fn();

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        onVersionSelect={onVersionSelect}
      />
    );

    await waitFor(() => {
      expect(onVersionSelect).toHaveBeenCalledWith('v1-uuid');
    });
  });

  it('should not auto-select when version already selected', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });
    const onVersionSelect = vi.fn();

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v2-uuid"
        onVersionSelect={onVersionSelect}
      />
    );

    await waitFor(() => {
      expect(screen.getByRole('combobox')).toBeTruthy();
    });

    // Should not be called since we already have a selection
    expect(onVersionSelect).not.toHaveBeenCalled();
  });

  it('should show trust state badge for selected version', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v1-uuid"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Trusted')).toBeTruthy();
    });
  });

  it('should show truncated hash for selected version', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v1-uuid"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      // Hash is truncated to first 12 chars + "..."
      expect(screen.getByText('abc123def456...')).toBeTruthy();
    });
  });

  it('should disable selector when disabled prop is true', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: mockVersions });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v1-uuid"
        onVersionSelect={vi.fn()}
        disabled={true}
      />
    );

    await waitFor(() => {
      const combobox = screen.getByRole('combobox');
      expect(combobox).toHaveAttribute('disabled');
    });
  });

  it('should disable selector when only one version exists', async () => {
    mockListDatasetVersions.mockResolvedValue({ versions: [mockVersions[0]] });

    renderWithProviders(
      <DatasetVersionPicker
        datasetId="dataset-123"
        selectedVersionId="v1-uuid"
        onVersionSelect={vi.fn()}
      />
    );

    await waitFor(() => {
      const combobox = screen.getByRole('combobox');
      expect(combobox).toHaveAttribute('disabled');
    });
  });

  describe('trust state indicators', () => {
    it('should show Trusted badge for allowed state', async () => {
      mockListDatasetVersions.mockResolvedValue({
        versions: [{ ...mockVersions[0], trust_state: 'allowed' }],
      });

      renderWithProviders(
        <DatasetVersionPicker
          datasetId="dataset-123"
          selectedVersionId="v1-uuid"
          onVersionSelect={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Trusted')).toBeTruthy();
      });
    });

    it('should show Warning badge for allowed_with_warning state', async () => {
      mockListDatasetVersions.mockResolvedValue({
        versions: [{ ...mockVersions[0], dataset_version_id: 'v1-uuid', trust_state: 'allowed_with_warning' }],
      });

      renderWithProviders(
        <DatasetVersionPicker
          datasetId="dataset-123"
          selectedVersionId="v1-uuid"
          onVersionSelect={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Warning')).toBeTruthy();
      });
    });

    it('should show Blocked badge for blocked state', async () => {
      mockListDatasetVersions.mockResolvedValue({
        versions: [{ ...mockVersions[0], dataset_version_id: 'v1-uuid', trust_state: 'blocked' }],
      });

      renderWithProviders(
        <DatasetVersionPicker
          datasetId="dataset-123"
          selectedVersionId="v1-uuid"
          onVersionSelect={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Blocked')).toBeTruthy();
      });
    });

    it('should show Pending badge for needs_approval state', async () => {
      mockListDatasetVersions.mockResolvedValue({
        versions: [{ ...mockVersions[0], dataset_version_id: 'v1-uuid', trust_state: 'needs_approval' }],
      });

      renderWithProviders(
        <DatasetVersionPicker
          datasetId="dataset-123"
          selectedVersionId="v1-uuid"
          onVersionSelect={vi.fn()}
        />
      );

      await waitFor(() => {
        expect(screen.getByText('Pending')).toBeTruthy();
      });
    });
  });
});
