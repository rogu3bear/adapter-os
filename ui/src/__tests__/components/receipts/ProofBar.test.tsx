import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ProofBar } from '@/components/receipts/ProofBar';
import { toast } from 'sonner';

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const mockWriteText = vi.fn();

function setupUser() {
  const user = userEvent.setup();
  navigator.clipboard.writeText = mockWriteText as unknown as Clipboard['writeText'];
  return user;
}

describe('ProofBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockResolvedValue(undefined);
  });

  it('renders without crashing', () => {
    render(<ProofBar />);
    expect(screen.getByText(/Receipt digest/i)).toBeInTheDocument();
  });

  it('displays receipt digest when provided', () => {
    const { container } = render(<ProofBar receiptDigest="abc123" />);
    const receiptValue = container.querySelector('[data-cy="proofbar-receipt-digest-value"]');
    expect(receiptValue).toHaveTextContent('abc123');
  });

  it('displays "Not available" when receipt digest is null', () => {
    const { container } = render(<ProofBar receiptDigest={null} />);
    const receiptValue = container.querySelector('[data-cy="proofbar-receipt-digest-value"]');
    expect(receiptValue).toHaveTextContent('Not available');
  });

  it('displays "Not available" when receipt digest is empty string', () => {
    const { container } = render(<ProofBar receiptDigest="" />);
    const receiptValue = container.querySelector('[data-cy="proofbar-receipt-digest-value"]');
    expect(receiptValue).toHaveTextContent('Not available');
  });

  it('displays "Not available" when receipt digest is whitespace', () => {
    const { container } = render(<ProofBar receiptDigest="   " />);
    const receiptValue = container.querySelector('[data-cy="proofbar-receipt-digest-value"]');
    expect(receiptValue).toHaveTextContent('Not available');
  });

  it('displays trace ID when provided', () => {
    const { container } = render(<ProofBar traceId="trace-xyz" />);
    const traceValue = container.querySelector('[data-cy="proofbar-trace-id-value"]');
    expect(traceValue).toHaveTextContent('trace-xyz');
  });

  it('displays "Not available" when trace ID is null', () => {
    const { container } = render(<ProofBar traceId={null} />);
    const traceValue = container.querySelector('[data-cy="proofbar-trace-id-value"]');
    expect(traceValue).toHaveTextContent('Not available');
  });

  it('displays backend when provided', () => {
    render(<ProofBar backendUsed="coreml" />);
    expect(screen.getByText('coreml')).toBeInTheDocument();
  });

  it('displays "Not available" when backend is null', () => {
    render(<ProofBar backendUsed={null} />);
    const backendRow = screen.getByText('Backend:').closest('div');
    expect(backendRow).not.toBeNull();
    expect(within(backendRow as HTMLElement).getByText('Not available')).toBeInTheDocument();
  });

  it('displays determinism mode badge', () => {
    render(<ProofBar determinismMode="deterministic" />);
    expect(screen.getByText('deterministic')).toBeInTheDocument();
  });

  it('displays "unknown" when determinism mode is null', () => {
    render(<ProofBar determinismMode={null} />);
    expect(screen.getByText('unknown')).toBeInTheDocument();
  });

  it('applies default variant badge for deterministic mode', () => {
    render(<ProofBar determinismMode="deterministic" />);
    const badge = screen.getByText('deterministic');
    expect(badge.className).toContain('bg-primary');
  });

  it('applies secondary variant badge for non-deterministic mode', () => {
    render(<ProofBar determinismMode="non-deterministic" />);
    const badge = screen.getByText('non-deterministic');
    expect(badge.className).toContain('bg-secondary');
  });

  it('copies receipt digest to clipboard when copy button is clicked', async () => {
    const user = setupUser();
    render(<ProofBar receiptDigest="abc123" />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const receiptCopyButton = copyButtons[0]; // First copy button is for receipt digest
    await user.click(receiptCopyButton);

    await waitFor(() => {
      expect(mockWriteText).toHaveBeenCalledWith('abc123');
      expect(toast.success).toHaveBeenCalledWith('Receipt digest copied');
    });
  });

  it('copies trace ID to clipboard when copy button is clicked', async () => {
    const user = setupUser();
    render(<ProofBar traceId="trace-xyz" />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const traceCopyButton = copyButtons[1]; // Second copy button is for trace ID
    await user.click(traceCopyButton);

    await waitFor(() => {
      expect(mockWriteText).toHaveBeenCalledWith('trace-xyz');
      expect(toast.success).toHaveBeenCalledWith('Trace ID copied');
    });
  });

  it('shows error toast when copying null value', async () => {
    const user = setupUser();
    render(<ProofBar receiptDigest={null} />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const receiptCopyButton = copyButtons[0];
    await user.click(receiptCopyButton);

    await waitFor(() => {
      expect(mockWriteText).not.toHaveBeenCalled();
      expect(toast.error).toHaveBeenCalledWith('Receipt digest is not available to copy');
    });
  });

  it('shows error toast when clipboard API fails', async () => {
    const user = setupUser();
    mockWriteText.mockRejectedValue(new Error('Clipboard error'));
    render(<ProofBar receiptDigest="abc123" />);

    const copyButtons = screen.getAllByLabelText(/Copy/i);
    const receiptCopyButton = copyButtons[0];
    await user.click(receiptCopyButton);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('Unable to copy Receipt digest');
    });
  });

  it('calls onOpenTrace when Open Trace button is clicked', async () => {
    const user = setupUser();
    const mockOnOpenTrace = vi.fn();
    const { container } = render(<ProofBar traceId="trace-xyz" onOpenTrace={mockOnOpenTrace} />);

    const openTraceButton = container.querySelector('[data-cy="proofbar-open-trace"]');
    await user.click(openTraceButton as Element);

    expect(mockOnOpenTrace).toHaveBeenCalled();
  });

  it('disables Open Trace button when no trace ID', () => {
    const mockOnOpenTrace = vi.fn();
    const { container } = render(<ProofBar traceId={null} onOpenTrace={mockOnOpenTrace} />);

    const openTraceButton = container.querySelector('[data-cy="proofbar-open-trace"]');
    expect(openTraceButton).toBeDisabled();
  });

  it('disables Open Trace button when no onOpenTrace callback', () => {
    const { container } = render(<ProofBar traceId="trace-xyz" />);

    const openTraceButton = container.querySelector('[data-cy="proofbar-open-trace"]');
    expect(openTraceButton).toBeDisabled();
  });

  it('shows Export Evidence button when evidenceAvailable is true', () => {
    const mockOnExportEvidence = vi.fn();
    const { container } = render(<ProofBar evidenceAvailable={true} onExportEvidence={mockOnExportEvidence} />);

    expect(container.querySelector('[data-cy="export-evidence"]')).toBeInTheDocument();
  });

  it('hides Export Evidence button when evidenceAvailable is false', () => {
    const mockOnExportEvidence = vi.fn();
    const { container } = render(<ProofBar evidenceAvailable={false} onExportEvidence={mockOnExportEvidence} />);

    expect(container.querySelector('[data-cy="export-evidence"]')).not.toBeInTheDocument();
  });

  it('hides Export Evidence button when onExportEvidence is not provided', () => {
    const { container } = render(<ProofBar evidenceAvailable={true} />);

    expect(container.querySelector('[data-cy="export-evidence"]')).not.toBeInTheDocument();
  });

  it('calls onExportEvidence when Export Evidence button is clicked', async () => {
    const user = setupUser();
    const mockOnExportEvidence = vi.fn();
    const { container } = render(<ProofBar evidenceAvailable={true} onExportEvidence={mockOnExportEvidence} />);

    const exportButton = container.querySelector('[data-cy="export-evidence"]');
    await user.click(exportButton as Element);

    expect(mockOnExportEvidence).toHaveBeenCalled();
  });

  it('applies custom className when provided', () => {
    const { container } = render(<ProofBar className="custom-class" />);
    const proofBar = container.querySelector('[data-cy="proof-bar"]');
    expect(proofBar?.className).toContain('custom-class');
  });

  it('normalizes and trims whitespace from values', () => {
    const { container } = render(<ProofBar receiptDigest="  abc123  " traceId="  trace-xyz  " />);
    const receiptValue = container.querySelector('[data-cy="proofbar-receipt-digest-value"]');
    const traceValue = container.querySelector('[data-cy="proofbar-trace-id-value"]');
    expect(receiptValue).toHaveTextContent('abc123');
    expect(traceValue).toHaveTextContent('trace-xyz');
  });
});
