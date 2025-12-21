import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { EvidencePanel } from '@/components/evidence/EvidencePanel';
import type { Evidence } from '@/api/document-types';

// Mock useEvidenceApi hook
const mockCreateEvidence = vi.fn();
const mockDownloadEvidence = vi.fn();
const mockInvalidateEvidence = vi.fn();

vi.mock('@/hooks/api/useEvidenceApi', () => ({
  useEvidenceApi: () => ({
    evidence: {
      data: mockEvidenceData,
      isLoading: mockIsLoading,
      isFetching: mockIsFetching,
    },
    createEvidence: mockCreateEvidence,
    isCreating: mockIsCreating,
    downloadEvidence: mockDownloadEvidence,
    isDownloading: mockIsDownloading,
    invalidateEvidence: mockInvalidateEvidence,
  }),
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Hoisted mock variables
let mockEvidenceData: Evidence[] | undefined = [];
let mockIsLoading = false;
let mockIsFetching = false;
let mockIsCreating = false;
let mockIsDownloading = false;

const mockEvidence: Evidence = {
  id: 'evidence-1',
  dataset_id: null,
  adapter_id: null,
  tenant_id: 'test-tenant',
  evidence_type: 'audit',
  reference: 'trace-123',
  description: 'Test evidence',
  confidence: 'high',
  created_by: 'user-1',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T01:00:00Z',
  metadata_json: null,
  trace_id: 'trace-123',
  status: 'ready',
  bundle_size_bytes: 1024,
  file_name: 'evidence-1.json',
};

describe('EvidencePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockEvidenceData = [];
    mockIsLoading = false;
    mockIsFetching = false;
    mockIsCreating = false;
    mockIsDownloading = false;
  });

  it('renders without crashing', () => {
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByTestId('evidence-panel')).toBeInTheDocument();
  });

  it('displays title and description', () => {
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('Evidence')).toBeInTheDocument();
    expect(screen.getByText(/Compliance-ready bundles/)).toBeInTheDocument();
  });

  it('shows loading state when isLoading is true', () => {
    mockIsLoading = true;
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('Loading evidence...')).toBeInTheDocument();
  });

  it('shows empty state when no evidence exists', () => {
    mockEvidenceData = [];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('No evidence found for this trace yet.')).toBeInTheDocument();
  });

  it('displays evidence entries when data exists', () => {
    mockEvidenceData = [mockEvidence];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('trace-123')).toBeInTheDocument();
    expect(screen.getByText('Test evidence')).toBeInTheDocument();
    expect(screen.getByText('Ready')).toBeInTheDocument();
  });

  it('displays evidence type badge', () => {
    mockEvidenceData = [mockEvidence];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('audit')).toBeInTheDocument();
  });

  it('displays bundle size in KB', () => {
    mockEvidenceData = [mockEvidence];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('1.0 KB')).toBeInTheDocument();
  });

  it('displays error code badge when present', () => {
    mockEvidenceData = [{ ...mockEvidence, error_code: 'BUILD_FAILED', status: 'failed' }];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('BUILD_FAILED')).toBeInTheDocument();
  });

  it('displays created and updated timestamps', () => {
    mockEvidenceData = [mockEvidence];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText(/Created 12\/31\/2024/)).toBeInTheDocument();
    expect(screen.getByText(/Updated 12\/31\/2024/)).toBeInTheDocument();
  });

  it('calls createEvidence when Create evidence button is clicked', async () => {
    const user = userEvent.setup();
    mockCreateEvidence.mockResolvedValue({});
    render(<EvidencePanel traceId="trace-123" tenantId="test-tenant" receiptDigest="receipt-abc" />);

    const createButton = screen.getByRole('button', { name: /Create evidence/i });
    await user.click(createButton);

    await waitFor(() => {
      expect(mockCreateEvidence).toHaveBeenCalledWith({
        evidence_type: 'audit',
        reference: 'receipt-abc',
        description: 'Inference evidence bundle',
        metadata_json: JSON.stringify({
          trace_id: 'trace-123',
          receipt_digest: 'receipt-abc',
          source: 'inference_playground',
        }),
      });
    });
  });

  it('disables Create evidence button when no traceId', () => {
    render(<EvidencePanel traceId="" />);
    const createButton = screen.getByRole('button', { name: /Create evidence/i });
    expect(createButton).toBeDisabled();
  });

  it('disables Create evidence button when isCreating', () => {
    mockIsCreating = true;
    render(<EvidencePanel traceId="trace-123" />);
    const createButton = screen.getByRole('button', { name: /Create evidence/i });
    expect(createButton).toBeDisabled();
  });

  it('calls downloadEvidence when Download button is clicked', async () => {
    const user = userEvent.setup();
    mockEvidenceData = [mockEvidence];
    mockDownloadEvidence.mockResolvedValue({});
    render(<EvidencePanel traceId="trace-123" />);

    const downloadButton = screen.getByRole('button', { name: /Download/i });
    await user.click(downloadButton);

    await waitFor(() => {
      expect(mockDownloadEvidence).toHaveBeenCalledWith({
        evidenceId: 'evidence-1',
        filename: 'evidence-1.json',
      });
    });
  });

  it('disables Download button when isDownloading', () => {
    mockEvidenceData = [mockEvidence];
    mockIsDownloading = true;
    render(<EvidencePanel traceId="trace-123" />);
    const downloadButton = screen.getByRole('button', { name: /Download/i });
    expect(downloadButton).toBeDisabled();
  });

  it('calls invalidateEvidence when Refresh button is clicked', async () => {
    const user = userEvent.setup();
    render(<EvidencePanel traceId="trace-123" />);

    const refreshButton = screen.getByRole('button', { name: /Refresh/i });
    await user.click(refreshButton);

    expect(mockInvalidateEvidence).toHaveBeenCalled();
  });

  it('disables Refresh button when isFetching', () => {
    mockIsFetching = true;
    render(<EvidencePanel traceId="trace-123" />);
    const refreshButton = screen.getByRole('button', { name: /Refresh/i });
    expect(refreshButton).toBeDisabled();
  });

  it('shows spinning icon when fetching', () => {
    mockIsFetching = true;
    render(<EvidencePanel traceId="trace-123" />);
    const refreshButton = screen.getByRole('button', { name: /Refresh/i });
    const icon = refreshButton.querySelector('.animate-spin');
    expect(icon).toBeInTheDocument();
  });

  it('shows spinning icon when creating', () => {
    mockIsCreating = true;
    render(<EvidencePanel traceId="trace-123" />);
    const createButton = screen.getByRole('button', { name: /Create evidence/i });
    const icon = createButton.querySelector('.animate-spin');
    expect(icon).toBeInTheDocument();
  });

  it('displays "No description" when description is null', () => {
    mockEvidenceData = [{ ...mockEvidence, description: null }];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('No description')).toBeInTheDocument();
  });

  it('uses default filename when file_name is null', async () => {
    const user = userEvent.setup();
    mockEvidenceData = [{ ...mockEvidence, file_name: null }];
    mockDownloadEvidence.mockResolvedValue({});
    render(<EvidencePanel traceId="trace-123" />);

    const downloadButton = screen.getByRole('button', { name: /Download/i });
    await user.click(downloadButton);

    await waitFor(() => {
      expect(mockDownloadEvidence).toHaveBeenCalledWith({
        evidenceId: 'evidence-1',
        filename: 'evidence-evidence-1.json',
      });
    });
  });

  it('renders multiple evidence entries', () => {
    mockEvidenceData = [
      mockEvidence,
      { ...mockEvidence, id: 'evidence-2', reference: 'trace-456', status: 'building' },
    ];
    render(<EvidencePanel traceId="trace-123" />);
    expect(screen.getByText('trace-123')).toBeInTheDocument();
    expect(screen.getByText('trace-456')).toBeInTheDocument();
    expect(screen.getByText('Ready')).toBeInTheDocument();
    expect(screen.getByText('Building')).toBeInTheDocument();
  });
});
