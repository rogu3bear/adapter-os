import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  generateEvidenceBundle,
  downloadEvidenceBundle,
  type GenerateEvidenceBundleOptions,
} from '@/utils/export/generateEvidenceBundle';
import type { ExtendedMessageExport } from '@/utils/export/types';

// Mock crypto.subtle for testing
const mockDigest = vi.fn();

// Setup crypto mock if not already present
if (typeof global.crypto === 'undefined') {
  (global as any).crypto = {};
}
if (!global.crypto.subtle) {
  (global.crypto as any).subtle = {};
}
(global.crypto.subtle as any).digest = mockDigest;

describe('generateEvidenceBundle', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Mock SHA-256 hash output
    mockDigest.mockImplementation(async () => {
      // Return a mock ArrayBuffer representing a hash
      const hashArray = new Uint8Array(32);
      for (let i = 0; i < 32; i++) {
        hashArray[i] = i;
      }
      return hashArray.buffer;
    });
  });

  it('generates bundle with schema version and timestamp', async () => {
    const options: GenerateEvidenceBundleOptions = {
      messages: [],
    };

    const bundle = await generateEvidenceBundle(options);

    expect(bundle.schemaVersion).toBe('1.0.0');
    expect(bundle.exportTimestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
    expect(bundle.exportId).toMatch(/^export-/);
  });

  it('uses provided export ID if given', async () => {
    const options: GenerateEvidenceBundleOptions = {
      messages: [],
      exportId: 'custom-export-123',
    };

    const bundle = await generateEvidenceBundle(options);

    expect(bundle.exportId).toBe('custom-export-123');
  });

  it('collects evidence from messages', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        evidence: [
          {
            documentId: 'doc-1',
            documentName: 'Guide',
            chunkId: 'chunk-1',
            pageNumber: 5,
            textPreview: 'Preview text',
            relevanceScore: 0.95,
            rank: 1,
          },
          {
            documentId: 'doc-2',
            documentName: 'Manual',
            chunkId: 'chunk-2',
            pageNumber: 10,
            textPreview: 'More text',
            relevanceScore: 0.87,
            rank: 2,
          },
        ],
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.evidence).toHaveLength(2);
    expect(bundle.evidence[0].documentId).toBe('doc-1');
    expect(bundle.evidence[0].documentName).toBe('Guide');
    expect(bundle.evidence[1].documentId).toBe('doc-2');
  });

  it('deduplicates evidence by chunk ID', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer 1',
        timestamp: '2025-12-12T09:00:00.000Z',
        evidence: [
          {
            documentId: 'doc-1',
            documentName: 'Guide',
            chunkId: 'chunk-1',
            pageNumber: 5,
            textPreview: 'Preview text',
            relevanceScore: 0.95,
            rank: 1,
          },
        ],
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Answer 2',
        timestamp: '2025-12-12T09:01:00.000Z',
        evidence: [
          {
            documentId: 'doc-1',
            documentName: 'Guide',
            chunkId: 'chunk-1', // Same chunk ID
            pageNumber: 5,
            textPreview: 'Preview text',
            relevanceScore: 0.95,
            rank: 1,
          },
        ],
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.evidence).toHaveLength(1);
    expect(bundle.evidence[0].chunkId).toBe('chunk-1');
  });

  it('collects trace information from messages', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        traceId: 'trace-123',
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Answer 2',
        timestamp: '2025-12-12T09:01:00.000Z',
        traceId: 'trace-456',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages, backendId: 'test-backend' });

    expect(bundle.traces).toHaveLength(2);
    expect(bundle.traces[0]).toEqual({
      traceId: 'trace-123',
      backendId: 'test-backend',
    });
    expect(bundle.traces[1]).toEqual({
      traceId: 'trace-456',
      backendId: 'test-backend',
    });
  });

  it('uses default backend ID if not provided', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        traceId: 'trace-123',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.traces[0].backendId).toBe('aos-worker');
  });

  it('includes signatures for verified messages', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        traceId: 'trace-123',
        isVerified: true,
        verifiedAt: '2025-12-12T09:00:05.000Z',
        proofDigest: '0xdeadbeef',
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Answer 2',
        timestamp: '2025-12-12T09:01:00.000Z',
        traceId: 'trace-456',
        isVerified: false,
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.signatures).toHaveLength(1);
    expect(bundle.signatures[0]).toEqual({
      traceId: 'trace-123',
      signature: '0xdeadbeef',
      signedAt: '2025-12-12T09:00:05.000Z',
    });
  });

  it('generates placeholder signature if no proof digest', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        traceId: 'trace-123',
        isVerified: true,
        verifiedAt: '2025-12-12T09:00:05.000Z',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.signatures).toHaveLength(1);
    expect(bundle.signatures[0].signature).toMatch(/^sig-/);
  });

  it('computes bundle hash for integrity', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.checksums.bundleHash).toMatch(/^0x[0-9a-f]+$/);
    expect(mockDigest).toHaveBeenCalled();
    expect(mockDigest.mock.calls[0][0]).toBe('SHA-256');
    // Check that the second arg has the Uint8Array properties
    const digestArg = mockDigest.mock.calls[0][1];
    expect(digestArg).toHaveProperty('buffer');
    expect(digestArg).toHaveProperty('byteLength');
  });

  it('handles messages without evidence', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.evidence).toHaveLength(0);
  });

  it('handles messages without trace ID', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.traces).toHaveLength(0);
  });

  it('preserves evidence metadata including bbox and char_range', async () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        evidence: [
          {
            documentId: 'doc-1',
            documentName: 'Guide',
            chunkId: 'chunk-1',
            pageNumber: 5,
            textPreview: 'Preview text',
            relevanceScore: 0.95,
            rank: 1,
            charRange: { start: 100, end: 200 },
            bbox: { x: 10, y: 20, width: 100, height: 50 },
            citationId: 'cite-1',
          },
        ],
      },
    ];

    const bundle = await generateEvidenceBundle({ messages });

    expect(bundle.evidence[0].charRange).toEqual({ start: 100, end: 200 });
    expect(bundle.evidence[0].bbox).toEqual({ x: 10, y: 20, width: 100, height: 50 });
    expect(bundle.evidence[0].citationId).toBe('cite-1');
  });

  it('generates different export IDs on repeated calls', async () => {
    const options: GenerateEvidenceBundleOptions = {
      messages: [],
    };

    const bundle1 = await generateEvidenceBundle(options);
    // Wait a tiny bit to ensure timestamp changes
    await new Promise((resolve) => setTimeout(resolve, 1));
    const bundle2 = await generateEvidenceBundle(options);

    expect(bundle1.exportId).not.toBe(bundle2.exportId);
  });
});

describe('downloadEvidenceBundle', () => {
  let mockLink: HTMLAnchorElement;
  let mockCreateObjectURL: ReturnType<typeof vi.fn>;
  let mockRevokeObjectURL: ReturnType<typeof vi.fn>;
  let mockAppendChild: ReturnType<typeof vi.fn>;
  let mockRemoveChild: ReturnType<typeof vi.fn>;
  let mockClick: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockClick = vi.fn();
    mockLink = {
      href: '',
      download: '',
      click: mockClick,
    } as unknown as HTMLAnchorElement;

    vi.spyOn(document, 'createElement').mockReturnValue(mockLink);

    mockCreateObjectURL = vi.fn().mockReturnValue('blob:mock-url');
    mockRevokeObjectURL = vi.fn();
    global.URL.createObjectURL = mockCreateObjectURL;
    global.URL.revokeObjectURL = mockRevokeObjectURL;

    mockAppendChild = vi.spyOn(document.body, 'appendChild').mockImplementation(() => mockLink);
    mockRemoveChild = vi.spyOn(document.body, 'removeChild').mockImplementation(() => mockLink);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('downloads bundle as JSON file', () => {
    const bundle = {
      schemaVersion: '1.0.0',
      exportTimestamp: '2025-12-12T10:00:00.000Z',
      exportId: 'export-123',
      traces: [],
      evidence: [],
      signatures: [],
      checksums: {
        bundleHash: '0xabcdef',
      },
    };

    downloadEvidenceBundle(bundle);

    expect(mockLink.download).toBe('evidence-bundle-export-123.json');
  });

  it('uses custom filename if provided', () => {
    const bundle = {
      schemaVersion: '1.0.0',
      exportTimestamp: '2025-12-12T10:00:00.000Z',
      exportId: 'export-123',
      traces: [],
      evidence: [],
      signatures: [],
      checksums: {
        bundleHash: '0xabcdef',
      },
    };

    downloadEvidenceBundle(bundle, 'custom-bundle.json');

    expect(mockLink.download).toBe('custom-bundle.json');
  });

  it('creates blob with formatted JSON content', () => {
    const bundle = {
      schemaVersion: '1.0.0',
      exportTimestamp: '2025-12-12T10:00:00.000Z',
      exportId: 'export-123',
      traces: [],
      evidence: [],
      signatures: [],
      checksums: {
        bundleHash: '0xabcdef',
      },
    };

    downloadEvidenceBundle(bundle);

    expect(mockCreateObjectURL).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'application/json',
      })
    );
  });

  it('triggers download flow correctly', () => {
    const bundle = {
      schemaVersion: '1.0.0',
      exportTimestamp: '2025-12-12T10:00:00.000Z',
      exportId: 'export-123',
      traces: [],
      evidence: [],
      signatures: [],
      checksums: {
        bundleHash: '0xabcdef',
      },
    };

    downloadEvidenceBundle(bundle);

    expect(mockAppendChild).toHaveBeenCalledWith(mockLink);
    expect(mockClick).toHaveBeenCalled();
    expect(mockRemoveChild).toHaveBeenCalledWith(mockLink);
    expect(mockRevokeObjectURL).toHaveBeenCalledWith('blob:mock-url');
  });
});
