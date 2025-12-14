import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TraceTab } from '@/components/chat/drawer/TraceTab';
import type { TraceResponseV1 } from '@/api/types';

// Mock useTrace hook
const mockUseTrace = vi.fn();
vi.mock('@/hooks/useTrace', () => ({
  useTrace: (...args: unknown[]) => mockUseTrace(...args),
}));

// Mock useTenant hook
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: 'test-tenant' }),
}));

// Mock clipboard - need to define it on navigator properly
const mockWriteText = vi.fn(() => Promise.resolve());
Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: mockWriteText },
  writable: true,
  configurable: true,
});

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = createQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>
  );
}

const mockTrace: TraceResponseV1 = {
  trace_id: 'trace-abc123def456',
  context_digest: 'ctx-digest-0123456789abcdef',
  policy_digest: 'pol-digest-fedcba9876543210',
  backend_id: 'coreml',
  kernel_version_id: 'v2.1.0',
  tokens: [
    {
      token_index: 0,
      token_id: 'tok-0',
      selected_adapter_ids: ['adapter-finance', 'adapter-legal'],
      gates_q15: [24576, 8192],
      decision_hash: 'dec-hash-0',
      policy_mask_digest: 'mask-0',
    },
    {
      token_index: 1,
      token_id: 'tok-1',
      selected_adapter_ids: ['adapter-finance'],
      gates_q15: [32767],
      decision_hash: 'dec-hash-1',
      policy_mask_digest: 'mask-1',
    },
    {
      token_index: 2,
      token_id: 'tok-2',
      selected_adapter_ids: [],
      gates_q15: [],
      decision_hash: 'dec-hash-2',
      policy_mask_digest: 'mask-2',
    },
  ],
};

describe('TraceTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockClear();
  });

  describe('Empty state', () => {
    it('shows empty message when no traceId provided', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId={null} />);

      expect(screen.getByText('No trace available')).toBeInTheDocument();
      expect(screen.getByText(/Select a message with trace data/)).toBeInTheDocument();
    });

    it('shows empty message when traceId is undefined', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId={undefined} />);

      expect(screen.getByText('No trace available')).toBeInTheDocument();
    });
  });

  describe('Loading state', () => {
    it('shows loading skeleton when fetching trace', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: true,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId="trace-123" />);

      // Should show skeleton elements (Skeleton component renders with specific classes)
      const skeletons = document.querySelectorAll('[data-slot="skeleton"]');
      expect(skeletons.length).toBeGreaterThan(0);
    });
  });

  describe('Error state', () => {
    it('shows error alert when trace fetch fails', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: false,
        isError: true,
        error: new Error('Network error'),
      });

      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText(/Failed to load trace/)).toBeInTheDocument();
      expect(screen.getByText(/Network error/)).toBeInTheDocument();
    });

    it('shows not found alert when trace returns null', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText(/Trace data not found/)).toBeInTheDocument();
    });
  });

  describe('Loaded state', () => {
    beforeEach(() => {
      mockUseTrace.mockReturnValue({
        data: mockTrace,
        isLoading: false,
        isError: false,
      });
    });

    it('displays trace summary with all digests', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText('Trace Summary')).toBeInTheDocument();
      expect(screen.getByText('Trace ID')).toBeInTheDocument();
      expect(screen.getByText('Context digest')).toBeInTheDocument();
      expect(screen.getByText('Policy digest')).toBeInTheDocument();
    });

    it('displays backend and kernel badges', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText(/Backend: coreml/)).toBeInTheDocument();
      expect(screen.getByText(/Kernel: v2.1.0/)).toBeInTheDocument();
    });

    it('displays token count badge', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText('3 tokens')).toBeInTheDocument();
    });

    it('displays token decisions table', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText('Token Decisions')).toBeInTheDocument();
      // Check header - actual header uses "Adapter : Gate (Q15)"
      expect(screen.getByText('#')).toBeInTheDocument();
      expect(screen.getByText('Adapter : Gate (Q15)')).toBeInTheDocument();
    });

    it('shows adapter badges for tokens with adapters', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      // Token 0 has adapter-finance (truncated: first 6 + ... + last 4)
      // "adapter-finance" -> "adapte...ance" - appears multiple times
      const adapterBadges = screen.getAllByText(/adapte\.\.\.ance/);
      expect(adapterBadges.length).toBeGreaterThan(0);
    });

    it('shows "no adapters" text for tokens without adapters', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      // Token 2 has no adapters - should show "no adapters" text
      expect(screen.getByText('no adapters')).toBeInTheDocument();
    });

    it('displays gate Q15 values', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      // Token 0 has gate 24576 - displayed with toLocaleString()
      expect(screen.getByText('24,576')).toBeInTheDocument();
    });
  });

  describe('Copy functionality', () => {
    beforeEach(() => {
      mockUseTrace.mockReturnValue({
        data: mockTrace,
        isLoading: false,
        isError: false,
      });
    });

    it('renders copy buttons for all digests', () => {
      renderWithProviders(<TraceTab traceId="trace-123" />);

      // Should have 3 copy buttons: Trace ID, Context digest, Policy digest
      const copyButtons = screen.getAllByRole('button', { name: /copy/i });
      expect(copyButtons.length).toBe(3);
    });
  });

  describe('Navigation', () => {
    beforeEach(() => {
      mockUseTrace.mockReturnValue({
        data: mockTrace,
        isLoading: false,
        isError: false,
      });
    });

    it('navigates to full trace viewer when button clicked', async () => {
      const user = userEvent.setup();
      renderWithProviders(<TraceTab traceId="trace-123" />);

      const openButton = screen.getByRole('button', { name: /open full trace viewer/i });
      await user.click(openButton);

      expect(mockNavigate).toHaveBeenCalledWith('/telemetry/viewer?requestId=trace-123');
    });

    it('calls onOpenFullViewer callback if provided', async () => {
      const onOpenFullViewer = vi.fn();
      const user = userEvent.setup();
      renderWithProviders(<TraceTab traceId="trace-123" onOpenFullViewer={onOpenFullViewer} />);

      const openButton = screen.getByRole('button', { name: /open full trace viewer/i });
      await user.click(openButton);

      expect(onOpenFullViewer).toHaveBeenCalled();
      expect(mockNavigate).not.toHaveBeenCalled();
    });
  });

  describe('Token limit', () => {
    it('shows "show all" link when more than 10 tokens', () => {
      const manyTokensTrace: TraceResponseV1 = {
        ...mockTrace,
        tokens: Array.from({ length: 15 }, (_, i) => ({
          token_index: i,
          selected_adapter_ids: ['adapter-a'],
          gates_q15: [16384],
          decision_hash: `hash-${i}`,
          policy_mask_digest: `mask-${i}`,
        })),
      };

      mockUseTrace.mockReturnValue({
        data: manyTokensTrace,
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.getByText('Showing 10 of 15')).toBeInTheDocument();
      expect(screen.getByText(/Show all 15 tokens/)).toBeInTheDocument();
    });

    it('does not show "show all" link when 10 or fewer tokens', () => {
      mockUseTrace.mockReturnValue({
        data: mockTrace, // has 3 tokens
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId="trace-123" />);

      expect(screen.queryByText(/Show all/)).not.toBeInTheDocument();
    });
  });

  describe('Hook parameters', () => {
    it('passes traceId and tenantId to useTrace hook', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: true,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId="my-trace-id" />);

      expect(mockUseTrace).toHaveBeenCalledWith('my-trace-id', 'test-tenant');
    });

    it('passes undefined traceId when null', () => {
      mockUseTrace.mockReturnValue({
        data: null,
        isLoading: false,
        isError: false,
      });

      renderWithProviders(<TraceTab traceId={null} />);

      expect(mockUseTrace).toHaveBeenCalledWith(undefined, 'test-tenant');
    });
  });
});
