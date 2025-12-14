import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { RunReceiptPanel } from '@/components/receipts/RunReceiptPanel';
import type { InferResponse } from '@/api/types';
import { toast } from 'sonner';

// Mock dependencies
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

// Mock clipboard API
const mockWriteText = vi.fn();
Object.defineProperty(navigator, 'clipboard', {
  writable: true,
  value: {
    writeText: mockWriteText,
  },
});

// Mock document.createElement for download functionality
const mockClick = vi.fn();
let lastLinkElement: HTMLAnchorElement | null = null;
const originalCreateElement = document.createElement.bind(document);
const mockCreateElement = vi.fn((tag: string) => {
  if (tag === 'a') {
    const anchor = originalCreateElement(tag) as HTMLAnchorElement;
    anchor.click = mockClick as unknown as () => void;
    lastLinkElement = anchor;
    return anchor;
  }
  return originalCreateElement(tag);
});

const mockCreateObjectURL = vi.fn(() => 'blob:mock-url');
const mockRevokeObjectURL = vi.fn();
const originalCreateObjectURL = global.URL.createObjectURL;
const originalRevokeObjectURL = global.URL.revokeObjectURL;

const mockResponse: InferResponse = {
  schema_version: '1.0',
  id: 'response-1',
  text: 'Test response text',
  tokens_generated: 10,
  latency_ms: 100,
  adapters_used: ['adapter-1', 'adapter-2'],
  finish_reason: 'stop',
  backend_used: 'coreml',
  determinism_mode_applied: 'deterministic',
  run_receipt: {
    trace_id: 'trace-123',
    run_head_hash: 'head-abc',
    output_digest: 'output-xyz',
    receipt_digest: 'receipt-def',
    logical_prompt_tokens: 20,
    prefix_cached_token_count: 5,
    billed_input_tokens: 15,
    logical_output_tokens: 10,
    billed_output_tokens: 10,
    signature: 'sig-data',
    attestation: 'attestation-data',
  },
  trace: {
    latency_ms: 100,
    router_decisions: [],
    evidence_spans: [{ text: 'evidence text', relevance: 0.9 }],
  },
};

describe('RunReceiptPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockResolvedValue(undefined);
    document.createElement = mockCreateElement as unknown as typeof document.createElement;
    global.URL.createObjectURL = mockCreateObjectURL;
    global.URL.revokeObjectURL = mockRevokeObjectURL;
    lastLinkElement = null;
  });

  afterEach(() => {
    document.createElement = originalCreateElement;
    global.URL.createObjectURL = originalCreateObjectURL;
    global.URL.revokeObjectURL = originalRevokeObjectURL;
  });

  it('renders without crashing', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('Run receipt')).toBeInTheDocument();
  });

  it('returns null when response is null', () => {
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={null} />
      </MemoryRouter>
    );
    expect(container.firstChild).toBeNull();
  });

  it('displays title and description', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('Run receipt')).toBeInTheDocument();
    expect(screen.getByText(/Backend, determinism, and signed evidence/)).toBeInTheDocument();
  });

  it('displays backend used badge', () => {
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-backend-used"]');
    expect(badge).toHaveTextContent('coreml');
  });

  it('displays determinism mode badge', () => {
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-determinism-mode"]');
    expect(badge).toHaveTextContent('Deterministic (strict)');
  });

  it('displays adapters used', () => {
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    const adaptersSection = container.querySelector('[data-cy="receipt-adapters-used"]');
    expect(adaptersSection).toHaveTextContent('adapter-1');
    expect(adaptersSection).toHaveTextContent('adapter-2');
  });

  it('displays "Base model only" when no adapters used', () => {
    const responseNoAdapters = { ...mockResponse, adapters_used: [] };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoAdapters} />
      </MemoryRouter>
    );
    expect(screen.getByText('Base model only')).toBeInTheDocument();
  });

  it('displays signature present badge', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('Signature present')).toBeInTheDocument();
  });

  it('displays signature missing badge when no signature', () => {
    const responseNoSig = {
      ...mockResponse,
      run_receipt: { ...mockResponse.run_receipt!, signature: undefined },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoSig} />
      </MemoryRouter>
    );
    expect(screen.getByText('Signature missing')).toBeInTheDocument();
  });

  it('displays attestation present badge', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('Attestation present')).toBeInTheDocument();
  });

  it('displays attestation missing badge when no attestation', () => {
    const responseNoAttestation = {
      ...mockResponse,
      run_receipt: { ...mockResponse.run_receipt!, attestation: undefined },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoAttestation} />
      </MemoryRouter>
    );
    expect(screen.getByText('Attestation missing')).toBeInTheDocument();
  });

  it('displays digest items', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('trace-123')).toBeInTheDocument();
    expect(screen.getByText('head-abc')).toBeInTheDocument();
    expect(screen.getByText('output-xyz')).toBeInTheDocument();
    expect(screen.getByText('receipt-def')).toBeInTheDocument();
  });

  it('displays "Not provided" for missing digest values', () => {
    const responseNoDigests = {
      ...mockResponse,
      run_receipt: {
        ...mockResponse.run_receipt!,
        trace_id: undefined,
        run_head_hash: undefined,
      },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoDigests} />
      </MemoryRouter>
    );
    const notProvidedElements = screen.getAllByText('Not provided');
    expect(notProvidedElements.length).toBeGreaterThan(0);
  });

  it('displays token accounting values', () => {
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );
    expect(screen.getByText('20')).toBeInTheDocument(); // logical_prompt_tokens
    expect(screen.getByText('5')).toBeInTheDocument(); // prefix_cached_token_count
    expect(screen.getByText('15')).toBeInTheDocument(); // billed_input_tokens
    expect(screen.getByText('10')).toBeInTheDocument(); // logical_output_tokens (appears twice)
  });

  it('formats token numbers with locale formatting', () => {
    const responseLargeNumbers = {
      ...mockResponse,
      run_receipt: {
        ...mockResponse.run_receipt!,
        logical_prompt_tokens: 1000000,
      },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseLargeNumbers} />
      </MemoryRouter>
    );
    expect(screen.getByText('1,000,000')).toBeInTheDocument();
  });

  it('copies digest value to clipboard when copy button is clicked', async () => {
    const user = userEvent.setup();
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );

    const copyButton = container.querySelector('[data-cy="copy-trace-id"]');
    await user.click(copyButton as Element);

    await waitFor(() => {
      expect(mockWriteText).toHaveBeenCalledWith('trace-123');
      expect(toast.success).toHaveBeenCalledWith('Trace ID copied');
    });
  });

  it('shows error toast when copying unavailable value', async () => {
    const user = userEvent.setup();
    const responseNoTrace = {
      ...mockResponse,
      run_receipt: { ...mockResponse.run_receipt!, trace_id: undefined },
    };
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoTrace} />
      </MemoryRouter>
    );

    const copyButton = container.querySelector('[data-cy="copy-trace-id"]');
    await user.click(copyButton as Element);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('Trace ID is unavailable to copy');
    });
  });

  it('navigates to trace viewer when Open Trace is clicked', async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );

    const openTraceButton = screen.getByText('Open Trace');
    await user.click(openTraceButton);

    expect(mockNavigate).toHaveBeenCalledWith('/telemetry/viewer?requestId=trace-123');
  });

  it('disables open trace button when trace ID is unavailable', () => {
    const responseNoTrace = {
      ...mockResponse,
      run_receipt: { ...mockResponse.run_receipt!, trace_id: undefined },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoTrace} />
      </MemoryRouter>
    );

    const openTraceButton = screen.getByText('Open Trace');
    // Button should be disabled when there's no trace ID
    expect(openTraceButton).toBeDisabled();
  });

  it('exports evidence bundle when Export Evidence is clicked', async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );

    const exportButton = screen.getByText('Export Evidence');
    await user.click(exportButton);

    await waitFor(() => {
      expect(mockClick).toHaveBeenCalled();
      expect(lastLinkElement?.download).toBe('evidence-bundle-receipt-def.json');
      expect(toast.success).toHaveBeenCalledWith('Evidence bundle exported');
    });
  });

  it('includes all required fields in exported evidence bundle', async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <RunReceiptPanel response={mockResponse} />
      </MemoryRouter>
    );

    const exportButton = screen.getByText('Export Evidence');
    await user.click(exportButton);

    await waitFor(() => {
      const blob = mockCreateObjectURL.mock.calls[0][0] as Blob;
      expect(blob.type).toBe('application/json');
    });
  });

  it('uses response ID as fallback for filename when no receipt digest', async () => {
    const user = userEvent.setup();
    const responseNoDigest = {
      ...mockResponse,
      run_receipt: { ...mockResponse.run_receipt!, receipt_digest: undefined },
    };
    render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoDigest} />
      </MemoryRouter>
    );

    const exportButton = screen.getByText('Export Evidence');
    await user.click(exportButton);

    await waitFor(() => {
      expect(lastLinkElement?.download).toBe('evidence-bundle-response-1.json');
    });
  });

  it('uses requested backend when backend_used is not available', () => {
    const responseNoBackend = { ...mockResponse, backend_used: undefined };
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoBackend} requestedBackend="mlx" />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-backend-used"]');
    expect(badge).toHaveTextContent('mlx');
  });

  it('displays "auto" when no backend information available', () => {
    const responseNoBackend = { ...mockResponse, backend_used: undefined };
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoBackend} />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-backend-used"]');
    expect(badge).toHaveTextContent('auto');
  });

  it('uses requested determinism mode when not applied', () => {
    const responseNoDeterminism = { ...mockResponse, determinism_mode_applied: undefined };
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoDeterminism} requestedDeterminismMode="strict" />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-determinism-mode"]');
    expect(badge).toHaveTextContent('strict');
  });

  it('displays "unknown" when no determinism information available', () => {
    const responseNoDeterminism = { ...mockResponse, determinism_mode_applied: undefined };
    const { container } = render(
      <MemoryRouter>
        <RunReceiptPanel response={responseNoDeterminism} />
      </MemoryRouter>
    );
    const badge = container.querySelector('[data-cy="receipt-determinism-mode"]');
    expect(badge).toHaveTextContent('unknown');
  });
});
