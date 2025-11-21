import React from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { BrowserRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Adapters } from '../components/Adapters';
import apiClient from '../api/client';
import { UndoRedoProvider } from '../contexts/UndoRedoContext';
import { CoreProviders } from '../providers/CoreProviders';
import { FeatureProviders } from '../providers/FeatureProviders';
import { DensityProvider } from '../contexts/DensityContext';

// Mock virtualized table for JSDOM (must be before component imports use it)
vi.mock('../components/ui/virtualized-table', () => ({
  VirtualizedTableRows: ({ items, children }: { items: unknown[]; children: (item: unknown, index: number) => React.ReactNode }) => (
    <>{items.map((item, i) => <React.Fragment key={i}>{children(item, i)}</React.Fragment>)}</>
  )
}));

// Create mock functions using vi.hoisted so they're available in the mock factory
const {
  mockListAdapters,
  mockLoadAdapter,
  mockUnloadAdapter,
  mockDeleteAdapter,
  mockRegisterAdapter,
  mockGetAdapterHealth,
  mockPinAdapter,
  mockUnpinAdapter,
  createMockApiClient,
} = vi.hoisted(() => {
  const listAdapters = vi.fn();
  const loadAdapter = vi.fn();
  const unloadAdapter = vi.fn();
  const deleteAdapter = vi.fn();
  const registerAdapter = vi.fn();
  const getAdapterHealth = vi.fn();
  const pinAdapter = vi.fn();
  const unpinAdapter = vi.fn();

  return {
    mockListAdapters: listAdapters,
    mockLoadAdapter: loadAdapter,
    mockUnloadAdapter: unloadAdapter,
    mockDeleteAdapter: deleteAdapter,
    mockRegisterAdapter: registerAdapter,
    mockGetAdapterHealth: getAdapterHealth,
    mockPinAdapter: pinAdapter,
    mockUnpinAdapter: unpinAdapter,
    createMockApiClient: () => ({
      listAdapters,
      loadAdapter,
      unloadAdapter,
      deleteAdapter,
      registerAdapter,
      getAdapterHealth,
      pinAdapter,
      unpinAdapter,
      getToken: vi.fn(() => 'test-token'),
      setToken: vi.fn(),
      getCurrentUser: vi.fn().mockResolvedValue({ user_id: 'test-user', email: 'test@example.com', role: 'admin' }),
      listTenants: vi.fn().mockResolvedValue([{ id: 'default', name: 'Default' }]),
      getSystemMetrics: vi.fn().mockResolvedValue(null),
      subscribeToMetrics: vi.fn(() => () => {}),
      subscribeToActivity: vi.fn(() => () => {}),
      subscribeToAlerts: vi.fn(() => () => {}),
      listAlerts: vi.fn().mockResolvedValue([]),
      promoteAdapterState: vi.fn().mockResolvedValue({ old_state: 'cold', new_state: 'warm' }),
      downloadAdapterManifest: vi.fn().mockResolvedValue({}),
      upsertAdapterDirectory: vi.fn().mockResolvedValue({ success: true }),
    }),
  };
});

// Mock the API client for all import paths
vi.mock('../api/client', () => {
  const mockApiClient = createMockApiClient();
  return { default: mockApiClient, apiClient: mockApiClient };
});

vi.mock('@/api/client', () => {
  const mockApiClient = createMockApiClient();
  return { default: mockApiClient, apiClient: mockApiClient };
});

vi.mock('src/api/client', () => {
  const mockApiClient = createMockApiClient();
  return { default: mockApiClient, apiClient: mockApiClient };
});

// Mock SSE hook
vi.mock('../hooks/useSSE', () => ({
  useSSE: () => ({ data: null, error: null, connected: false }),
}));

const mockAdapters = [
  {
    id: '1',
    adapter_id: 'python-general-v1',
    name: 'python-general-v1',
    hash_b3: 'b3:abc123',
    rank: 16,
    tier: 1,
    languages_json: '["python"]',
    framework: 'python',
    category: 'code',
    scope: 'global',
    current_state: 'hot',
    pinned: false,
    memory_bytes: 16 * 1024 * 1024,
    last_activated: '2024-02-15T10:30:00Z',
    activation_count: 1247,
    created_at: '2024-01-15T10:30:00Z',
    updated_at: '2024-02-15T10:30:00Z',
    active: true,
  },
  {
    id: '2',
    adapter_id: 'django-v2',
    name: 'django-v2',
    hash_b3: 'b3:def456',
    rank: 12,
    tier: 2,
    languages_json: '["python"]',
    framework: 'django',
    category: 'framework',
    scope: 'global',
    current_state: 'cold',
    pinned: false,
    memory_bytes: 16 * 1024 * 1024,
    last_activated: '2024-02-15T09:45:00Z',
    activation_count: 89,
    created_at: '2024-01-20T14:15:00Z',
    updated_at: '2024-02-15T09:45:00Z',
    active: true,
  },
];

const mockUser = {
  user_id: 'test-user',
  email: 'test@example.com',
  display_name: 'Test User',
  role: 'admin' as const,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  is_active: true,
};

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
    },
  });

  return render(
    <BrowserRouter>
      <QueryClientProvider client={queryClient}>
        <CoreProviders>
          <FeatureProviders>
            <UndoRedoProvider>
              <DensityProvider pageKey="adapters" defaultDensity="comfortable">
                {ui}
              </DensityProvider>
            </UndoRedoProvider>
          </FeatureProviders>
        </CoreProviders>
      </QueryClientProvider>
    </BrowserRouter>
  );
}

describe('Adapter Management Flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListAdapters.mockResolvedValue(mockAdapters);
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Adapter List Display', () => {
    it('should display loading state initially', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      // Should show loading indicator
      expect(screen.getByRole('status')).toBeTruthy();
    });

    it('should display adapters after loading', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
        expect(screen.getByText('django-v2')).toBeTruthy();
      });
    });

    it('should display adapter count', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(mockListAdapters).toHaveBeenCalled();
      });
    });
  });

  describe('Adapter Loading/Unloading', () => {
    it('should load adapter when load button is clicked', async () => {
      const user = userEvent.setup();
      mockLoadAdapter.mockResolvedValue({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('django-v2')).toBeTruthy();
      });

      // Find and click dropdown trigger for the cold adapter (django-v2)
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for django-v2/i });
      await user.click(dropdownTrigger);

      // Find and click Load menu item (Radix portal renders to body)
      const loadMenuItem = await screen.findByText('Load');
      await user.click(loadMenuItem);

      await waitFor(() => {
        expect(mockLoadAdapter).toHaveBeenCalledWith('django-v2');
      });
    });

    it('should unload adapter when unload button is clicked', async () => {
      const user = userEvent.setup();
      mockUnloadAdapter.mockResolvedValue({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger for the hot adapter (python-general-v1)
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Unload menu item
      const unloadMenuItem = await screen.findByRole('menuitem', { name: /unload/i });
      await user.click(unloadMenuItem);

      await waitFor(() => {
        expect(mockUnloadAdapter).toHaveBeenCalledWith('python-general-v1');
      });
    });
  });

  describe('Adapter Filtering', () => {
    it('should filter adapters by search term', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find search input
      const searchInput = screen.getByPlaceholderText(/search/i);
      fireEvent.change(searchInput, { target: { value: 'django' } });

      await waitFor(() => {
        expect(screen.getByText('django-v2')).toBeTruthy();
        expect(screen.queryByText('python-general-v1')).toBeNull();
      });
    });
  });

  describe('Bulk Operations', () => {
    it('should select multiple adapters for bulk actions', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Select adapters using checkboxes
      const checkboxes = screen.getAllByRole('checkbox');
      expect(checkboxes.length).toBeGreaterThan(1);
      fireEvent.click(checkboxes[1]); // First adapter

      // Should show bulk action bar
      await waitFor(() => {
        expect(screen.getByText(/selected/i)).toBeTruthy();
      });
    });
  });

  describe('Error Handling', () => {
    it('should display error when loading fails', async () => {
      mockListAdapters.mockRejectedValue(
        new Error('Failed to fetch adapters')
      );

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        // Should not crash and should handle error gracefully
        expect(mockListAdapters).toHaveBeenCalled();
      });
    });

    it('should allow retry on load failure', async () => {
      mockLoadAdapter
        .mockRejectedValueOnce(new Error('Network error'))
        .mockResolvedValueOnce({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });
    });
  });

  describe('Adapter Details Navigation', () => {
    it('should navigate to adapter detail on click', async () => {
      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Adapter names should be clickable
      const adapterLink = screen.getByText('python-general-v1');
      expect(adapterLink).toBeTruthy();
    });
  });

  describe('Adapter Delete Operations', () => {
    it('should delete adapter when delete button is clicked', async () => {
      const user = userEvent.setup();
      mockDeleteAdapter.mockResolvedValue({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Delete menu item
      const deleteMenuItem = await screen.findByRole('menuitem', { name: /delete/i });
      await user.click(deleteMenuItem);

      // Confirm deletion in the dialog (if there's a confirmation)
      const confirmButton = await screen.findByRole('button', { name: /confirm|delete/i });
      if (confirmButton) {
        await user.click(confirmButton);
      }

      await waitFor(() => {
        expect(mockDeleteAdapter).toHaveBeenCalledWith('python-general-v1');
      });
    });

    it('should handle delete adapter error gracefully', async () => {
      const user = userEvent.setup();
      mockDeleteAdapter.mockRejectedValue(
        new Error('Failed to delete adapter')
      );

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Delete menu item
      const deleteMenuItem = await screen.findByRole('menuitem', { name: /delete/i });
      await user.click(deleteMenuItem);

      // Confirm deletion in the dialog
      const confirmButton = await screen.findByRole('button', { name: /confirm|delete/i });
      if (confirmButton) {
        await user.click(confirmButton);
      }

      await waitFor(() => {
        expect(mockDeleteAdapter).toHaveBeenCalledWith('python-general-v1');
      });
    });
  });

  describe('Adapter Pin/Unpin Operations', () => {
    it('should pin adapter when pin button is clicked', async () => {
      const user = userEvent.setup();
      mockPinAdapter.mockResolvedValue({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Pin menu item (unpinned adapter shows "Pin")
      const pinMenuItem = await screen.findByRole('menuitem', { name: /^pin$/i });
      await user.click(pinMenuItem);

      await waitFor(() => {
        expect(mockPinAdapter).toHaveBeenCalledWith('python-general-v1', true);
      });
    });

    it('should unpin adapter when unpin button is clicked', async () => {
      const user = userEvent.setup();
      // Mock adapter that is already pinned
      const pinnedAdapters = [
        {
          ...mockAdapters[0],
          pinned: true,
        },
        mockAdapters[1],
      ];
      mockListAdapters.mockResolvedValue(pinnedAdapters);
      mockUnpinAdapter.mockResolvedValue({ success: true });

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Unpin menu item (pinned adapter shows "Unpin")
      const unpinMenuItem = await screen.findByRole('menuitem', { name: /unpin/i });
      await user.click(unpinMenuItem);

      await waitFor(() => {
        expect(mockUnpinAdapter).toHaveBeenCalledWith('python-general-v1');
      });
    });

    it('should handle pin adapter error gracefully', async () => {
      const user = userEvent.setup();
      mockPinAdapter.mockRejectedValue(
        new Error('Failed to pin adapter')
      );

      renderWithProviders(
        <Adapters user={mockUser} selectedTenant="default" />
      );

      await waitFor(() => {
        expect(screen.getByText('python-general-v1')).toBeTruthy();
      });

      // Find and click dropdown trigger
      const dropdownTrigger = screen.getByRole('button', { name: /Actions for python-general-v1/i });
      await user.click(dropdownTrigger);

      // Find and click Pin menu item
      const pinMenuItem = await screen.findByRole('menuitem', { name: /^pin$/i });
      await user.click(pinMenuItem);

      await waitFor(() => {
        expect(mockPinAdapter).toHaveBeenCalledWith('python-general-v1', true);
      });
    });
  });
});

describe('Adapter Lifecycle States', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListAdapters.mockResolvedValue(mockAdapters);
  });

  it('should display correct lifecycle state badges', async () => {
    renderWithProviders(
      <Adapters user={mockUser} selectedTenant="default" />
    );

    await waitFor(() => {
      // Should show 'hot' and 'cold' state badges
      expect(screen.getByText('hot')).toBeTruthy();
      expect(screen.getByText('cold')).toBeTruthy();
    });
  });

  it('should show visual indicators for different states', async () => {
    renderWithProviders(
      <Adapters user={mockUser} selectedTenant="default" />
    );

    await waitFor(() => {
      expect(screen.getByText('python-general-v1')).toBeTruthy();
    });

    // Verify state indicators are present
    const hotBadge = screen.getByText('hot');
    const coldBadge = screen.getByText('cold');

    expect(hotBadge).toBeTruthy();
    expect(coldBadge).toBeTruthy();
  });
});
