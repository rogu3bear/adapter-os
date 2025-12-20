/**
 * Tests for DatasetChatPage component
 *
 * Covers:
 * - Page rendering with valid dataset
 * - Loading states
 * - Error handling
 * - Dataset not ready state
 * - Navigation
 * - Export functionality
 * - Chat interface integration
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import DatasetChatPage from '@/pages/Training/DatasetChatPage';
import type { Dataset } from '@/api/training-types';

// Mock hooks
const mockUseDataset = vi.hoisted(() => vi.fn());
const mockUseNavigate = vi.hoisted(() => vi.fn());
const mockUseTenant = vi.hoisted(() => vi.fn());

vi.mock('@/hooks/training', () => ({
  useTraining: {
    useDataset: vi.fn((...args: any[]) => mockUseDataset(...args)),
  },
}));

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: vi.fn(() => mockUseNavigate()),
    useParams: vi.fn(() => ({ datasetId: 'dataset-123' })),
  };
});

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: vi.fn(() => mockUseTenant()),
}));

// Mock ChatInterface
vi.mock('@/components/ChatInterface', () => ({
  ChatInterface: ({ datasetContext }: any) => (
    <div data-testid="chat-interface">
      Chat Interface
      {datasetContext && <span data-testid="dataset-context">{datasetContext.datasetName}</span>}
    </div>
  ),
}));

// Mock ExportDialog
vi.mock('@/components/export', () => ({
  ExportDialog: ({ open, onExport }: any) =>
    open ? (
      <div data-testid="export-dialog">
        Export Dialog
        <button onClick={() => onExport('markdown')}>Export Markdown</button>
      </div>
    ) : null,
}));

// Mock DatasetChatContext
vi.mock('@/contexts/DatasetChatContext', () => ({
  DatasetChatProvider: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="dataset-chat-provider">{children}</div>
  ),
  useDatasetChat: () => ({}),
}));

// Mock toast
const mockToastInfo = vi.hoisted(() => vi.fn());

vi.mock('sonner', () => ({
  toast: {
    info: (...args: unknown[]) => mockToastInfo(...args),
  },
}));

// Mock API client
vi.mock('@/api/services', () => ({
  __esModule: true,
  default: {
    createChatSession: vi.fn(() =>
      Promise.resolve({
        session_id: 'session-123',
        name: 'Test Session',
        stack_id: null,
        messages: [],
        created_at: '2025-01-01T10:00:00Z',
      })
    ),
    addChatMessage: vi.fn(() => Promise.resolve()),
    request: vi.fn(() =>
      Promise.resolve({
        dataset_id: 'dataset-123',
        format: 'jsonl',
        total_examples: 10,
        examples: [{ input: 'test', output: 'response' }],
      })
    ),
    getToken: vi.fn(() => 'test-token'),
  },
}));

// Test wrapper
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter initialEntries={['/training/datasets/dataset-123/chat']}>
      <QueryClientProvider client={queryClient}>
        <Routes>
          <Route path="/training/datasets/:datasetId/chat" element={children} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>
  );
}

// Mock data
const mockDataset: Dataset = {
  id: 'dataset-123',
  name: 'Test Dataset',
  dataset_version_id: 'version-456',
  validation_status: 'valid',
  file_count: 42,
  total_size_bytes: 1024 * 1024 * 5,
  total_tokens: 50000,
  created_at: '2025-01-01T10:00:00Z',
  updated_at: '2025-01-01T11:00:00Z',
};

const mockInvalidDataset: Dataset = {
  ...mockDataset,
  validation_status: 'invalid',
};

describe('DatasetChatPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseTenant.mockReturnValue({
      selectedTenant: 'test-tenant',
    });
    mockUseNavigate.mockReturnValue(vi.fn());
  });

  describe('rendering with valid dataset', () => {
    it('renders chat interface with valid dataset', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });

      expect(screen.getByText('Test Dataset')).toBeInTheDocument();
      expect(screen.getByTestId('dataset-context')).toHaveTextContent('Test Dataset');
    });

    it('displays dataset name in header', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText(/Chat with:/)).toBeInTheDocument();
      });

      expect(screen.getByText('Test Dataset')).toBeInTheDocument();
    });

    it('shows Dataset Context badge', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Dataset Context')).toBeInTheDocument();
      });
    });

    it('renders export button', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('dataset-chat-export')).toBeInTheDocument();
      });
    });

    it('renders view dataset details button', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('View Dataset Details')).toBeInTheDocument();
      });
    });

    it('renders ChatInterface with dataset context', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
        expect(screen.getByTestId('dataset-context')).toHaveTextContent('Test Dataset');
      });
    });
  });

  describe('loading state', () => {
    it('shows loading state while fetching dataset', () => {
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: true,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Loading dataset...')).toBeInTheDocument();
    });

    it('does not render chat interface while loading', () => {
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: true,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      expect(screen.queryByTestId('chat-interface')).not.toBeInTheDocument();
    });
  });

  describe('error handling', () => {
    it('shows error state when dataset fetch fails', () => {
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: false,
        error: new Error('Failed to fetch dataset'),
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Failed to fetch dataset')).toBeInTheDocument();
    });

    it('shows error state when dataset is not found', () => {
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Dataset not found')).toBeInTheDocument();
    });

    it('provides retry button on error', () => {
      const refetch = vi.fn();
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: false,
        error: new Error('Network error'),
        refetch,
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const retryButton = screen.getByText('Retry');
      expect(retryButton).toBeInTheDocument();
    });

    it('calls refetch when retry button is clicked', async () => {
      const refetch = vi.fn();
      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: false,
        error: new Error('Network error'),
        refetch,
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const retryButton = screen.getByText('Retry');
      await user.click(retryButton);

      expect(refetch).toHaveBeenCalled();
    });
  });

  describe('dataset not ready state', () => {
    it('shows not ready message for invalid dataset', async () => {
      mockUseDataset.mockReturnValue({
        data: mockInvalidDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Dataset Not Ready')).toBeInTheDocument();
      });

      expect(
        screen.getByText(/This dataset needs to be validated before you can chat with it/)
      ).toBeInTheDocument();
    });

    it('displays current validation status', async () => {
      mockUseDataset.mockReturnValue({
        data: mockInvalidDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('invalid')).toBeInTheDocument();
      });
    });

    it('provides link to dataset details when not ready', async () => {
      mockUseDataset.mockReturnValue({
        data: mockInvalidDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Go to Dataset Details')).toBeInTheDocument();
      });
    });

    it('does not render chat interface when dataset not ready', async () => {
      mockUseDataset.mockReturnValue({
        data: mockInvalidDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Dataset Not Ready')).toBeInTheDocument();
      });

      expect(screen.queryByTestId('chat-interface')).not.toBeInTheDocument();
    });
  });

  describe('navigation', () => {
    it('navigates back when back button is clicked', async () => {
      const navigate = vi.fn();
      mockUseNavigate.mockReturnValue(navigate);
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByText('Back')).toBeInTheDocument();
      });

      const backButton = screen.getByText('Back');
      await user.click(backButton);

      expect(navigate).toHaveBeenCalledWith(-1);
    });

    it('navigates to dataset details when button is clicked', async () => {
      const navigate = vi.fn();
      mockUseNavigate.mockReturnValue(navigate);
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByText('View Dataset Details')).toBeInTheDocument();
      });

      const detailsButton = screen.getByText('View Dataset Details');
      await user.click(detailsButton);

      expect(navigate).toHaveBeenCalledWith('/training/datasets/dataset-123');
    });
  });

  describe('export functionality', () => {
    it('opens export dialog when export button is clicked', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByTestId('dataset-chat-export')).toBeInTheDocument();
      });

      const exportButton = screen.getByTestId('dataset-chat-export');
      await user.click(exportButton);

      expect(screen.getByTestId('export-dialog')).toBeInTheDocument();
    });

    it('closes export dialog after export', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByTestId('dataset-chat-export')).toBeInTheDocument();
      });

      // Open export dialog
      const exportButton = screen.getByTestId('dataset-chat-export');
      await user.click(exportButton);

      // Export
      const exportMarkdownButton = screen.getByText('Export Markdown');
      await user.click(exportMarkdownButton);

      await waitFor(() => {
        expect(screen.queryByTestId('export-dialog')).not.toBeInTheDocument();
      });
    });

    it('shows info toast when export button is clicked', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByTestId('dataset-chat-export')).toBeInTheDocument();
      });

      // Open export dialog
      const exportButton = screen.getByTestId('dataset-chat-export');
      await user.click(exportButton);

      // Export
      const exportMarkdownButton = screen.getByText('Export Markdown');
      await user.click(exportMarkdownButton);

      expect(mockToastInfo).toHaveBeenCalledWith(
        expect.stringContaining('Use the export button in the chat area')
      );
    });
  });

  describe('chat interface props', () => {
    it('passes correct dataset context to ChatInterface', async () => {
      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        const contextElement = screen.getByTestId('dataset-context');
        expect(contextElement).toHaveTextContent('Test Dataset');
      });
    });

    it('passes selected tenant to ChatInterface', async () => {
      mockUseTenant.mockReturnValue({
        selectedTenant: 'custom-tenant',
      });

      mockUseDataset.mockReturnValue({
        data: mockDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });
    });
  });

  describe('edge cases', () => {
    it('handles missing datasetId in URL params', () => {
      vi.doMock('react-router-dom', async () => {
        const actual = await vi.importActual('react-router-dom');
        return {
          ...actual,
          useParams: () => ({ datasetId: undefined }),
        };
      });

      mockUseDataset.mockReturnValue({
        data: null,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Dataset not found')).toBeInTheDocument();
    });

    it('handles dataset without dataset_version_id', async () => {
      const datasetWithoutVersion = {
        ...mockDataset,
        dataset_version_id: undefined,
      };

      mockUseDataset.mockReturnValue({
        data: datasetWithoutVersion,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });
    });

    it('handles validation_status other than valid', async () => {
      const pendingDataset = {
        ...mockDataset,
        validation_status: 'pending' as const,
      };

      mockUseDataset.mockReturnValue({
        data: pendingDataset,
        isLoading: false,
        error: null,
        refetch: vi.fn(),
      });

      render(
        <TestWrapper>
          <DatasetChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Dataset Not Ready')).toBeInTheDocument();
      });

      expect(screen.getByText('pending')).toBeInTheDocument();
    });
  });
});
