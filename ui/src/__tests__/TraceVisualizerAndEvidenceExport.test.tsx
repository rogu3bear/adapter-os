import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { TraceVisualizer } from '@/components/TraceVisualizer';
import { RunReceiptPanel } from '@/components/receipts/RunReceiptPanel';
import type { InferResponse } from '@/api/types';

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

// Helper to read blob content (jsdom doesn't have Blob.text())
async function readBlobAsText(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsText(blob);
  });
}

describe('Trace viewer and evidence export', () => {
  beforeEach(() => {
    document.createElement = mockCreateElement as unknown as typeof document.createElement;
    global.URL.createObjectURL = mockCreateObjectURL;
    global.URL.revokeObjectURL = mockRevokeObjectURL;
    mockClick.mockReset();
    lastLinkElement = null;
    mockCreateObjectURL.mockClear();
    mockRevokeObjectURL.mockClear();
  });

  afterEach(() => {
    document.createElement = originalCreateElement;
    global.URL.createObjectURL = originalCreateObjectURL;
    global.URL.revokeObjectURL = originalRevokeObjectURL;
  });

  it('renders router trace with token counts and gates for large traces', () => {
    const trace = {
      latency_ms: 120,
      router_decisions: Array.from({ length: 12 }, (_, idx) => ({
        step: idx,
        adapters: [`adapter-${idx}`],
        gates: [0.42],
        stack_hash: `stack-${idx}`,
      })),
      evidence_spans: [],
    };

    render(<TraceVisualizer trace={trace as any} />);

    expect(screen.getByText('12 routing decisions')).toBeInTheDocument();
    expect(screen.getByText('+ 2 more decisions')).toBeInTheDocument();
    expect(screen.getAllByText(/Gate: 0\.42/)[0]).toBeInTheDocument();
  });

  it('exports evidence bundle with receipt digest in filename', async () => {
    const user = userEvent.setup();
    const response: InferResponse & { token_count?: number } = {
      schema_version: '1.0',
      id: 'run-1',
      text: 'Answer text',
      tokens_generated: 6,
      token_count: 6,
      latency_ms: 10,
      adapters_used: ['adapter-A'],
      finish_reason: 'stop',
      run_receipt: {
        trace_id: 'trace-123',
        run_head_hash: 'head-abc',
        output_digest: 'out-xyz',
        receipt_digest: 'b3abc123',
        logical_prompt_tokens: 12,
        prefix_cached_token_count: 4,
        billed_input_tokens: 8,
        logical_output_tokens: 6,
        billed_output_tokens: 6,
      },
      trace: {
        latency_ms: 5,
        router_decisions: [{ adapters: ['adapter-A'], gates: [0.42] }],
        evidence_spans: [{ text: 'snippet', relevance: 0.9 }],
      },
    };

    render(
      <MemoryRouter>
        <RunReceiptPanel response={response} />
      </MemoryRouter>
    );

    await user.click(screen.getByRole('button', { name: /export evidence/i }));

    expect(mockClick).toHaveBeenCalled();
    expect(lastLinkElement?.download).toBe('evidence-bundle-b3abc123.json');
    const blob = mockCreateObjectURL.mock.calls[0][0] as Blob;
    const content = await readBlobAsText(blob);
    expect(content).toContain(response.run_receipt.receipt_digest);
    expect(content).toContain(response.text);
  });
});
