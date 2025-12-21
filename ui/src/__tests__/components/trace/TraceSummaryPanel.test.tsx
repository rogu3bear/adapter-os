import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TraceSummaryPanel } from '@/components/trace/TraceSummaryPanel';
import type { TraceResponseV1 } from '@/api/types';

const mockWriteText = vi.fn();

function setupUser() {
  const user = userEvent.setup();
  navigator.clipboard.writeText = mockWriteText as unknown as Clipboard['writeText'];
  return user;
}

const mockTrace: TraceResponseV1 = {
  trace_id: 'trace-123',
  context_digest: 'context-abc',
  policy_digest: 'policy-xyz',
  backend_id: 'coreml',
  kernel_version_id: 'v1.0.0',
  tokens: [
    {
      token_index: 0,
      token_id: 'tok-1',
      selected_adapter_ids: ['adapter-1'],
      gates_q15: [16384],
      decision_hash: 'decision-abc',
      policy_mask_digest: 'mask-xyz',
    },
    {
      token_index: 1,
      token_id: 'tok-2',
      selected_adapter_ids: ['adapter-1', 'adapter-2'],
      gates_q15: [16384, 8192],
      decision_hash: 'decision-def',
      policy_mask_digest: 'mask-123',
      fusion_interval_id: 'fusion-1',
      fused_weight_hash: 'fused-abc',
    },
  ],
};

describe('TraceSummaryPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockClear();
    mockWriteText.mockImplementation(() => {});
  });

  it('renders without crashing', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('Trace Summary')).toBeInTheDocument();
  });

  it('displays title and description', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('Trace Summary')).toBeInTheDocument();
    expect(screen.getByText(/Inspect digests and runtime metadata/)).toBeInTheDocument();
  });

  it('displays trace ID', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('trace-123')).toBeInTheDocument();
  });

  it('displays backend badge', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('Backend: coreml')).toBeInTheDocument();
  });

  it('displays kernel version badge', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('Kernel: v1.0.0')).toBeInTheDocument();
  });

  it('displays token count badge', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('Tokens: 2')).toBeInTheDocument();
  });

  it('displays context digest', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('context-abc')).toBeInTheDocument();
  });

  it('displays policy digest', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    expect(screen.getByText('policy-xyz')).toBeInTheDocument();
  });

  it('copies context digest to clipboard when copy button is clicked', async () => {
    const user = setupUser();
    render(<TraceSummaryPanel trace={mockTrace} />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const contextCopyButton = copyButtons[0]; // First copy button is for context digest
    await user.click(contextCopyButton);

    expect(mockWriteText).toHaveBeenCalledWith('context-abc');
  });

  it('copies policy digest to clipboard when copy button is clicked', async () => {
    const user = setupUser();
    render(<TraceSummaryPanel trace={mockTrace} />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const policyCopyButton = copyButtons[1]; // Second copy button is for policy digest
    await user.click(policyCopyButton);

    expect(mockWriteText).toHaveBeenCalledWith('policy-xyz');
  });

  it('shows Export Evidence Bundle button when onExport is provided', () => {
    const mockOnExport = vi.fn();
    render(<TraceSummaryPanel trace={mockTrace} onExport={mockOnExport} />);
    expect(screen.getByText('Export Evidence Bundle')).toBeInTheDocument();
  });

  it('disables Export Evidence Bundle button when onExport is not provided', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);
    const exportButton = screen.getByText('Export Evidence Bundle');
    expect(exportButton).toBeDisabled();
  });

  it('calls onExport when Export Evidence Bundle button is clicked', async () => {
    const user = setupUser();
    const mockOnExport = vi.fn();
    render(<TraceSummaryPanel trace={mockTrace} onExport={mockOnExport} />);

    const exportButton = screen.getByText('Export Evidence Bundle');
    await user.click(exportButton);

    expect(mockOnExport).toHaveBeenCalled();
  });

  it('displays correct token count for empty tokens array', () => {
    const emptyTrace = { ...mockTrace, tokens: [] };
    render(<TraceSummaryPanel trace={emptyTrace} />);
    expect(screen.getByText('Tokens: 0')).toBeInTheDocument();
  });

  it('displays correct token count for large number of tokens', () => {
    const largeTrace = {
      ...mockTrace,
      tokens: Array.from({ length: 100 }, (_, i) => ({
        token_index: i,
        selected_adapter_ids: [],
        gates_q15: [],
        decision_hash: `hash-${i}`,
        policy_mask_digest: `mask-${i}`,
      })),
    };
    render(<TraceSummaryPanel trace={largeTrace} />);
    expect(screen.getByText('Tokens: 100')).toBeInTheDocument();
  });

  it('renders all digest items in grid layout', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);

    // Check for digest labels
    expect(screen.getByText('Context digest')).toBeInTheDocument();
    expect(screen.getByText('Policy digest')).toBeInTheDocument();
  });

  it('has correct aria labels for copy buttons', () => {
    render(<TraceSummaryPanel trace={mockTrace} />);

    expect(screen.getByLabelText('Copy Context digest')).toBeInTheDocument();
    expect(screen.getByLabelText('Copy Policy digest')).toBeInTheDocument();
  });
});
