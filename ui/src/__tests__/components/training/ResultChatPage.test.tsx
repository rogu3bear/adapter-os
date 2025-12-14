/**
 * Tests for ResultChatPage component
 *
 * Covers:
 * - Page rendering with completed training job
 * - Loading states
 * - Error handling
 * - Chat not ready state
 * - Navigation
 * - Dataset context integration
 * - Header display with adapter and dataset chips
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import ResultChatPage from '@/pages/Training/ResultChatPage';

// Mock hooks
const mockUseNavigate = vi.fn();
const mockUseTenant = vi.fn();
const mockUseQuery = vi.fn();

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockUseNavigate(),
    useParams: () => ({ jobId: 'job-123' }),
  };
});

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => mockUseTenant(),
}));

vi.mock('@tanstack/react-query', async () => {
  const actual = await vi.importActual('@tanstack/react-query');
  return {
    ...actual,
    useQuery: (...args: any[]) => mockUseQuery(...args),
  };
});

// Mock API client
const mockGetTrainingJob = vi.fn();
const mockGetChatBootstrap = vi.fn();

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    getTrainingJob: (...args: any[]) => mockGetTrainingJob(...args),
    getChatBootstrap: (...args: any[]) => mockGetChatBootstrap(...args),
  },
}));

// Mock ChatInterface
vi.mock('@/components/ChatInterface', () => ({
  ChatInterface: ({ initialStackId, datasetContext }: any) => (
    <div data-testid="chat-interface">
      Chat Interface
      <span data-testid="stack-id">{initialStackId}</span>
      {datasetContext && <span data-testid="dataset-context">{datasetContext.datasetName}</span>}
    </div>
  ),
}));

// Mock DatasetChatProvider
vi.mock('@/contexts/DatasetChatContext', () => ({
  DatasetChatProvider: ({ children, initialDataset }: any) => (
    <div data-testid="dataset-chat-provider" data-dataset-id={initialDataset?.id}>
      {children}
    </div>
  ),
}));

// Test wrapper
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter initialEntries={['/training/jobs/job-123/chat']}>
      <QueryClientProvider client={queryClient}>
        <Routes>
          <Route path="/training/jobs/:jobId/chat" element={children} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>
  );
}

// Mock data
const mockTrainingJob = {
  id: 'job-123',
  adapter_name: 'My Adapter',
  status: 'completed',
  repo_id: 'repo-456',
  produced_version_id: 'version-789',
  created_at: '2025-01-01T10:00:00Z',
  updated_at: '2025-01-01T11:00:00Z',
};

const mockChatBootstrap = {
  ready: true,
  stack_id: 'stack-abc',
  adapter_version_id: 'version-789',
  dataset_id: 'dataset-def',
  dataset_name: 'Training Dataset',
  dataset_version_id: 'dsv-001',
  status: 'ready',
};

const mockChatBootstrapNotReady = {
  ready: false,
  stack_id: null,
  status: 'training',
};

describe('ResultChatPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseTenant.mockReturnValue({
      selectedTenant: 'test-tenant',
    });
    mockUseNavigate.mockReturnValue(vi.fn());

    // Default mock implementation for useQuery
    mockUseQuery.mockImplementation(({ queryKey, queryFn }: any) => {
      if (queryKey[0] === 'training-job') {
        return {
          data: mockTrainingJob,
          isLoading: false,
          error: null,
          refetch: vi.fn(),
        };
      }
      if (queryKey[0] === 'chat-bootstrap') {
        return {
          data: mockChatBootstrap,
          isLoading: false,
          error: null,
          refetch: vi.fn(),
        };
      }
      return { data: null, isLoading: false, error: null, refetch: vi.fn() };
    });
  });

  describe('rendering with ready chat', () => {
    it('renders chat interface when bootstrap is ready', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });
    });

    it('displays adapter name in header', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText(/Adapter: My Adapter/)).toBeInTheDocument();
      });
    });

    it('displays adapter information in header', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        // Adapter badge should be visible
        expect(screen.getByText(/Adapter:/)).toBeInTheDocument();
        expect(screen.getByText(/My Adapter/)).toBeInTheDocument();
      });
    });

    it('displays dataset name in header', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText(/Dataset: Training Dataset/)).toBeInTheDocument();
      });
    });

    it('displays dataset version ID (truncated)', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText(/dsv-001/)).toBeInTheDocument();
      });
    });

    it('passes stack ID to ChatInterface', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        const stackIdElement = screen.getByTestId('stack-id');
        expect(stackIdElement).toHaveTextContent('stack-abc');
      });
    });

    it('passes dataset context to ChatInterface when available', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        const datasetContextElement = screen.getByTestId('dataset-context');
        expect(datasetContextElement).toHaveTextContent('Training Dataset');
      });
    });

    it('wraps in DatasetChatProvider when dataset context exists', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        const provider = screen.getByTestId('dataset-chat-provider');
        expect(provider).toBeInTheDocument();
        expect(provider).toHaveAttribute('data-dataset-id', 'dataset-def');
      });
    });

    it('renders View Job Details button', async () => {
      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('View Job Details')).toBeInTheDocument();
      });
    });
  });

  describe('rendering without dataset context', () => {
    it('renders chat without DatasetChatProvider when no dataset', async () => {
      const bootstrapWithoutDataset = {
        ...mockChatBootstrap,
        dataset_id: null,
        dataset_name: null,
        dataset_version_id: null,
      };

      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: bootstrapWithoutDataset,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });

      expect(screen.queryByTestId('dataset-chat-provider')).not.toBeInTheDocument();
    });

    it('does not show dataset badge when no dataset', async () => {
      const bootstrapWithoutDataset = {
        ...mockChatBootstrap,
        dataset_id: undefined,
        dataset_name: undefined,
        dataset_version_id: undefined,
      };

      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: bootstrapWithoutDataset,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });

      // Should not show dataset badge in header
      const header = screen.getByRole('banner');
      const datasetBadges = header.querySelectorAll('[class*="badge"]');
      const hasDatasetBadge = Array.from(datasetBadges).some((badge) =>
        badge.textContent?.includes('Dataset:')
      );
      expect(hasDatasetBadge).toBe(false);
    });
  });

  describe('loading state', () => {
    it('shows loading state while fetching data', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        return { data: null, isLoading: true, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Preparing result chat...')).toBeInTheDocument();
    });

    it('does not render chat interface while loading', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        return { data: null, isLoading: true, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.queryByTestId('chat-interface')).not.toBeInTheDocument();
    });

    it('shows loading when job is loading', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return { data: null, isLoading: true, error: null, refetch: vi.fn() };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrap,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Preparing result chat...')).toBeInTheDocument();
    });

    it('shows loading when bootstrap is loading', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return { data: null, isLoading: true, error: null, refetch: vi.fn() };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Preparing result chat...')).toBeInTheDocument();
    });
  });

  describe('error handling', () => {
    it('shows error state when job fetch fails', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: null,
            isLoading: false,
            error: new Error('Failed to fetch job'),
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Failed to fetch job')).toBeInTheDocument();
    });

    it('shows error state when bootstrap fetch fails', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: null,
            isLoading: false,
            error: new Error('Bootstrap failed'),
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Bootstrap failed')).toBeInTheDocument();
    });

    it('provides retry button on error', () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: null,
            isLoading: false,
            error: new Error('Network error'),
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      expect(screen.getByText('Retry')).toBeInTheDocument();
    });
  });

  describe('chat not ready state', () => {
    it('shows not ready message when bootstrap.ready is false', async () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrapNotReady,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Chat Not Ready')).toBeInTheDocument();
      });
    });

    it('shows not ready message when stack_id is missing', async () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: { ...mockChatBootstrap, stack_id: null },
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('Chat Not Ready')).toBeInTheDocument();
      });
    });

    it('displays current status in not ready state', async () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrapNotReady,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText(/Status: training/)).toBeInTheDocument();
      });
    });

    it('provides link to view job progress when not ready', async () => {
      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrapNotReady,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByText('View Job Progress')).toBeInTheDocument();
      });
    });
  });

  describe('navigation', () => {
    it('navigates back when back button is clicked', async () => {
      const navigate = vi.fn();
      mockUseNavigate.mockReturnValue(navigate);

      render(
        <TestWrapper>
          <ResultChatPage />
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

    it('navigates to job details when button is clicked', async () => {
      const navigate = vi.fn();
      mockUseNavigate.mockReturnValue(navigate);

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByText('View Job Details')).toBeInTheDocument();
      });

      const detailsButton = screen.getByText('View Job Details');
      await user.click(detailsButton);

      expect(navigate).toHaveBeenCalledWith('/training/jobs/job-123');
    });

    it('navigates to job progress from not ready state', async () => {
      const navigate = vi.fn();
      mockUseNavigate.mockReturnValue(navigate);

      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrapNotReady,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      const user = userEvent.setup();
      await waitFor(() => {
        expect(screen.getByText('View Job Progress')).toBeInTheDocument();
      });

      const progressButton = screen.getByText('View Job Progress');
      await user.click(progressButton);

      expect(navigate).toHaveBeenCalledWith('/training/jobs/job-123');
    });
  });

  describe('edge cases', () => {
    it('handles missing adapter name gracefully', async () => {
      const jobWithoutName = {
        ...mockTrainingJob,
        adapter_name: undefined,
      };

      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: jobWithoutName,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: mockChatBootstrap,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });

      // Should not crash without adapter name
      expect(screen.queryByText(/Adapter:/)).not.toBeInTheDocument();
    });

    it('fallsback to adapter name when dataset_name is missing', async () => {
      const bootstrapWithoutDatasetName = {
        ...mockChatBootstrap,
        dataset_name: undefined,
        dataset_id: 'dataset-def', // Still has dataset_id
      };

      mockUseQuery.mockImplementation(({ queryKey }: any) => {
        if (queryKey[0] === 'training-job') {
          return {
            data: mockTrainingJob,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        if (queryKey[0] === 'chat-bootstrap') {
          return {
            data: bootstrapWithoutDatasetName,
            isLoading: false,
            error: null,
            refetch: vi.fn(),
          };
        }
        return { data: null, isLoading: false, error: null, refetch: vi.fn() };
      });

      render(
        <TestWrapper>
          <ResultChatPage />
        </TestWrapper>
      );

      await waitFor(() => {
        expect(screen.getByTestId('chat-interface')).toBeInTheDocument();
      });

      // Should still show dataset context with fallback name
      const datasetContextElement = screen.getByTestId('dataset-context');
      expect(datasetContextElement).toBeInTheDocument();
    });
  });
});
