import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { CalculationTab } from '@/components/chat/drawer/CalculationTab';
import type { ExtendedRouterDecision } from '@/api/types';
import { toast } from 'sonner';

// Mock clipboard
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

const mockRouterDecision: ExtendedRouterDecision = {
  selected_adapters: ['adapter-finance', 'adapter-legal', 'adapter-compliance'],
  candidates: [
    {
      adapter_id: 'adapter-finance',
      gate_float: 0.8,
      selected: true,
    },
    {
      adapter_id: 'adapter-legal',
      gate_float: 0.6,
      selected: true,
    },
    {
      adapter_id: 'adapter-compliance',
      gate_float: 0.4,
      selected: true,
    },
    {
      adapter_id: 'adapter-unused',
      gate_float: 0.1,
      selected: false,
    },
  ],
  k_value: 3,
  entropy: 0.8654,
  tau: 1.2,
  latency_ms: 15.7,
};

function TestWrapper({ children }: { children: React.ReactNode }) {
  return <MemoryRouter>{children}</MemoryRouter>;
}

describe('CalculationTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockClear();
    mockNavigate.mockClear();
  });

  describe('Empty state', () => {
    it('shows empty message when no data provided', () => {
      render(
        <TestWrapper>
          <CalculationTab />
        </TestWrapper>
      );

      expect(screen.getByText('No calculation data available')).toBeInTheDocument();
      expect(screen.getByText(/Inference metadata will appear here/)).toBeInTheDocument();
    });

    it('does not show empty message when any data is provided', () => {
      render(
        <TestWrapper>
          <CalculationTab requestId="req-123" />
        </TestWrapper>
      );

      expect(screen.queryByText('No calculation data available')).not.toBeInTheDocument();
    });
  });

  describe('Proof Summary section', () => {
    it('displays trace ID when provided', () => {
      render(
        <TestWrapper>
          <CalculationTab traceId="trace-abc123def456789" />
        </TestWrapper>
      );

      expect(screen.getByText('Trace ID')).toBeInTheDocument();
      expect(screen.getByText(/trace-abc12\.\.\.56789/)).toBeInTheDocument();
    });

    it('displays proof digest when provided', () => {
      render(
        <TestWrapper>
          <CalculationTab proofDigest="proof-xyz9876543210abc" />
        </TestWrapper>
      );

      expect(screen.getByText('Proof Digest')).toBeInTheDocument();
      expect(screen.getByText(/proof-xyz9\.\.\.210abc/)).toBeInTheDocument();
    });

    it('shows verified badge when isVerified is true', () => {
      render(
        <TestWrapper>
          <CalculationTab requestId="req-1" isVerified={true} />
        </TestWrapper>
      );

      expect(screen.getByText('Verified')).toBeInTheDocument();
    });

    it('shows pending badge when isVerified is false', () => {
      render(
        <TestWrapper>
          <CalculationTab requestId="req-1" isVerified={false} />
        </TestWrapper>
      );

      expect(screen.getByText('Pending')).toBeInTheDocument();
    });

    it('displays verifiedAt timestamp when provided', () => {
      const verifiedAt = '2025-01-15T14:30:00Z';
      render(
        <TestWrapper>
          <CalculationTab requestId="req-1" verifiedAt={verifiedAt} />
        </TestWrapper>
      );

      expect(screen.getByText('Verified At')).toBeInTheDocument();
      // Should display formatted date
      const dateDisplay = screen.getByText(/1\/15\/2025|15\/1\/2025/); // Handles different locales
      expect(dateDisplay).toBeInTheDocument();
    });
  });

  describe('Routing Decision section', () => {
    it('displays selected adapters', () => {
      render(
        <TestWrapper>
          <CalculationTab routerDecision={mockRouterDecision} />
        </TestWrapper>
      );

      expect(screen.getByText('Selected Adapters')).toBeInTheDocument();
      expect(screen.getByText('adapter-finance')).toBeInTheDocument();
      expect(screen.getByText('adapter-legal')).toBeInTheDocument();
      expect(screen.getByText('adapter-compliance')).toBeInTheDocument();

      // Unselected adapter should not appear
      expect(screen.queryByText('adapter-unused')).not.toBeInTheDocument();
    });

    it('displays gate weights with percentages', () => {
      render(
        <TestWrapper>
          <CalculationTab routerDecision={mockRouterDecision} />
        </TestWrapper>
      );

      expect(screen.getByText('Gate Weights')).toBeInTheDocument();

      // Total = 0.8 + 0.6 + 0.4 = 1.8
      // Percentages: 44.4%, 33.3%, 22.2%
      expect(screen.getByText('44.4%')).toBeInTheDocument(); // 0.8 / 1.8
      expect(screen.getByText('33.3%')).toBeInTheDocument(); // 0.6 / 1.8
      expect(screen.getByText('22.2%')).toBeInTheDocument(); // 0.4 / 1.8
    });

    it('displays router parameters when available', () => {
      render(
        <TestWrapper>
          <CalculationTab routerDecision={mockRouterDecision} />
        </TestWrapper>
      );

      expect(screen.getByText('K-value')).toBeInTheDocument();
      expect(screen.getByText('3')).toBeInTheDocument();

      expect(screen.getByText('Entropy')).toBeInTheDocument();
      expect(screen.getByText('0.8654')).toBeInTheDocument();

      expect(screen.getByText('Tau')).toBeInTheDocument();
      expect(screen.getByText('1.2000')).toBeInTheDocument();

      expect(screen.getByText('Latency')).toBeInTheDocument();
      expect(screen.getByText('15.70ms')).toBeInTheDocument();
    });

    it('does not show gate weights when no selected candidates', () => {
      const decisionNoSelected: ExtendedRouterDecision = {
        selected_adapters: [],
        candidates: [
          {
            adapter_id: 'adapter-1',
            gate_float: 0.5,
            selected: false,
          },
        ],
      };

      render(
        <TestWrapper>
          <CalculationTab routerDecision={decisionNoSelected} />
        </TestWrapper>
      );

      expect(screen.queryByText('Gate Weights')).not.toBeInTheDocument();
    });

    it('does not show gate weights when total is zero', () => {
      const decisionZeroGates: ExtendedRouterDecision = {
        selected_adapters: ['adapter-1'],
        candidates: [
          {
            adapter_id: 'adapter-1',
            gate_float: 0,
            selected: true,
          },
        ],
      };

      render(
        <TestWrapper>
          <CalculationTab routerDecision={decisionZeroGates} />
        </TestWrapper>
      );

      expect(screen.queryByText('Gate Weights')).not.toBeInTheDocument();
    });
  });

  describe('Copy functionality', () => {
    it('copies trace ID to clipboard', async () => {
      const user = userEvent.setup();
      render(
        <TestWrapper>
          <CalculationTab traceId="trace-abc123" />
        </TestWrapper>
      );

      const copyButton = screen.getByLabelText('Copy Trace ID');
      await user.click(copyButton);

      expect(mockWriteText).toHaveBeenCalledWith('trace-abc123');
      expect(toast.success).toHaveBeenCalledWith('Trace ID copied');
    });

    it('copies proof digest to clipboard', async () => {
      const user = userEvent.setup();
      render(
        <TestWrapper>
          <CalculationTab proofDigest="proof-xyz789" />
        </TestWrapper>
      );

      const copyButton = screen.getByLabelText('Copy Proof Digest');
      await user.click(copyButton);

      expect(mockWriteText).toHaveBeenCalledWith('proof-xyz789');
      expect(toast.success).toHaveBeenCalledWith('Proof Digest copied');
    });

    it('shows error toast when copy fails', async () => {
      mockWriteText.mockRejectedValueOnce(new Error('Copy failed'));
      const user = userEvent.setup();

      render(
        <TestWrapper>
          <CalculationTab traceId="trace-123" />
        </TestWrapper>
      );

      const copyButton = screen.getByLabelText('Copy Trace ID');
      await user.click(copyButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Failed to copy Trace ID');
      });
    });
  });

  describe('Actions section', () => {
    it('navigates to trace viewer when button clicked', async () => {
      const user = userEvent.setup();
      render(
        <TestWrapper>
          <CalculationTab traceId="trace-abc123" />
        </TestWrapper>
      );

      const openButton = screen.getByRole('button', { name: /open trace in telemetry viewer/i });
      await user.click(openButton);

      expect(mockNavigate).toHaveBeenCalledWith('/telemetry/viewer?requestId=trace-abc123');
    });

    it('disables navigation button when no traceId', () => {
      render(
        <TestWrapper>
          <CalculationTab requestId="req-123" />
        </TestWrapper>
      );

      const openButton = screen.getByRole('button', { name: /open trace in telemetry viewer/i });
      expect(openButton).toBeDisabled();
    });
  });

  describe('Combined data rendering', () => {
    it('renders all sections when all data provided', () => {
      render(
        <TestWrapper>
          <CalculationTab
            requestId="req-123"
            routerDecision={mockRouterDecision}
            traceId="trace-abc"
            proofDigest="proof-xyz"
            isVerified={true}
            verifiedAt="2025-01-15T12:00:00Z"
          />
        </TestWrapper>
      );

      expect(screen.getByText('Proof Summary')).toBeInTheDocument();
      expect(screen.getByText('Routing Decision')).toBeInTheDocument();
      expect(screen.getByText('Actions')).toBeInTheDocument();
      expect(screen.queryByText('No calculation data available')).not.toBeInTheDocument();
    });
  });

  describe('Text truncation', () => {
    it('truncates long trace IDs', () => {
      const longTraceId = 'trace-' + 'a'.repeat(50);
      render(
        <TestWrapper>
          <CalculationTab traceId={longTraceId} />
        </TestWrapper>
      );

      // Should show truncated version with ellipsis
      const truncatedText = screen.getByText(/trace-aaaa\.\.\.aaaa/);
      expect(truncatedText).toBeInTheDocument();
    });

    it('does not truncate short trace IDs', () => {
      render(
        <TestWrapper>
          <CalculationTab traceId="trace-123" />
        </TestWrapper>
      );

      expect(screen.getByText('trace-123')).toBeInTheDocument();
    });
  });
});
