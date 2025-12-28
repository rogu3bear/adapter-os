import { describe, it, expect } from 'vitest';
import {
  renderExtendedChatSessionMarkdown,
  renderSingleAnswerMarkdown,
} from '@/utils/export/renderMarkdown';
import type { ExtendedExportMetadata, ExtendedMessageExport } from '@/utils/export/types';

describe('renderExtendedChatSessionMarkdown', () => {
  const baseMetadata: ExtendedExportMetadata = {
    exportId: 'export-123',
    exportTimestamp: '2025-12-12T10:00:00.000Z',
    entityType: 'chat_session',
    entityId: 'session-456',
    entityName: 'Extended Test Session',
  };

  it('renders basic session with extended metadata', () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Question',
        timestamp: '2025-12-12T09:00:00.000Z',
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test Session', messages, baseMetadata);

    expect(markdown).toContain('# Chat Session: Test Session');
    expect(markdown).toContain('## Metadata');
    expect(markdown).toContain('- **Export ID**: export-123');
    expect(markdown).toContain('## Conversation');
  });

  it('includes determinism metadata when provided', () => {
    const metadata: ExtendedExportMetadata = {
      ...baseMetadata,
      determinismMode: 'deterministic',
      determinismState: 'verified',
    };

    const markdown = renderExtendedChatSessionMarkdown('Test', [], metadata);

    expect(markdown).toContain('- **Determinism Mode**: deterministic');
    expect(markdown).toContain('- **Determinism State**: verified');
  });

  it('includes dataset version and tenant metadata', () => {
    const metadata: ExtendedExportMetadata = {
      ...baseMetadata,
      datasetVersionId: 'dataset-v1',
      tenantId: 'tenant-123',
      collectionId: 'collection-456',
    };

    const markdown = renderExtendedChatSessionMarkdown('Test', [], metadata);

    expect(markdown).toContain('- **Dataset Version ID**: dataset-v1');
    expect(markdown).toContain('- **Workspace ID**: tenant-123');
    expect(markdown).toContain('- **Collection ID**: collection-456');
  });

  it('renders adapter stack table with gates', () => {
    const metadata: ExtendedExportMetadata = {
      ...baseMetadata,
      adapterStack: {
        stackId: 'stack-789',
        stackName: 'Production Stack',
        adapters: [
          {
            adapterId: 'adapter-1',
            version: 'v1.0',
            gate: 0.8,
          },
          {
            adapterId: 'adapter-2',
            version: 'v2.1',
            gate: 0.6,
          },
        ],
      },
    };

    const markdown = renderExtendedChatSessionMarkdown('Test', [], metadata);

    expect(markdown).toContain('### Adapter Stack');
    expect(markdown).toContain('- **Stack ID**: stack-789');
    expect(markdown).toContain('- **Stack Name**: Production Stack');
    expect(markdown).toContain('| Adapter ID | Version | Gate |');
    expect(markdown).toContain('| adapter-1 | v1.0 | 0.8000 |');
    expect(markdown).toContain('| adapter-2 | v2.1 | 0.6000 |');
  });

  it('handles adapter stack without version or gate', () => {
    const metadata: ExtendedExportMetadata = {
      ...baseMetadata,
      adapterStack: {
        stackId: 'stack-789',
        adapters: [
          {
            adapterId: 'adapter-1',
          },
        ],
      },
    };

    const markdown = renderExtendedChatSessionMarkdown('Test', [], metadata);

    expect(markdown).toContain('| adapter-1 | N/A | N/A |');
  });

  it('renders messages with trace metadata', () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        requestId: 'req-123',
        traceId: 'trace-456',
        proofDigest: '0xdeadbeef',
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test', messages, baseMetadata);

    expect(markdown).toContain('*Request ID: req-123*');
    expect(markdown).toContain('*Trace ID: trace-456*');
    expect(markdown).toContain('*Proof Digest: 0xdeadbeef*');
  });

  it('renders evidence with extended metadata', () => {
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
            bbox: { x: 10.5, y: 20.7, width: 100.2, height: 50.8 },
            citationId: 'cite-1',
          },
        ],
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test', messages, baseMetadata);

    expect(markdown).toContain('- Guide (p.5) [95.0% relevance] [cite-1]');
    expect(markdown).toContain('  > "Preview text"');
    expect(markdown).toContain('  - Character range: 100-200');
    expect(markdown).toContain('  - Position: (x: 10.5, y: 20.7, w: 100.2, h: 50.8)');
  });

  it('renders router decision with Q15 gates and entropy', () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        routerDecision: {
          requestId: 'req-123',
          selectedAdapters: ['adapter-1', 'adapter-2'],
          candidates: [
            {
              adapterId: 'adapter-1',
              gateQ15: 26214,
              gateFloat: 0.8,
              selected: true,
            },
            {
              adapterId: 'adapter-2',
              gateQ15: 19661,
              gateFloat: 0.6,
              selected: true,
            },
            {
              adapterId: 'adapter-3',
              gateQ15: 6554,
              gateFloat: 0.2,
              selected: false,
            },
          ],
          entropy: 1.2345,
        },
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test', messages, baseMetadata);

    expect(markdown).toContain('**Router Decision:**');
    expect(markdown).toContain('- Entropy: 1.2345');
    expect(markdown).toContain('| Adapter ID | Gate (Q15) | Gate (Float) | Selected |');
    expect(markdown).toContain('| adapter-1 | 26214 | 0.8000 | ✓ |');
    expect(markdown).toContain('| adapter-2 | 19661 | 0.6000 | ✓ |');
    expect(markdown).toContain('| adapter-3 | 6554 | 0.2000 | ✗ |');
  });

  it('renders verification status', () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: '2025-12-12T09:00:00.000Z',
        isVerified: true,
        verifiedAt: '2025-12-12T09:00:05.000Z',
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test', messages, baseMetadata);

    expect(markdown).toContain('*Verified at');
  });

  it('handles messages without optional fields gracefully', () => {
    const messages: ExtendedMessageExport[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: 'Simple answer',
        timestamp: '2025-12-12T09:00:00.000Z',
      },
    ];

    const markdown = renderExtendedChatSessionMarkdown('Test', messages, baseMetadata);

    expect(markdown).toContain('Simple answer');
    expect(markdown).not.toContain('Request ID:');
    expect(markdown).not.toContain('**Sources:**');
    expect(markdown).not.toContain('**Router Decision:**');
  });
});

describe('renderSingleAnswerMarkdown', () => {
  const baseMetadata: ExtendedExportMetadata = {
    exportId: 'export-123',
    exportTimestamp: '2025-12-12T10:00:00.000Z',
    entityType: 'chat_session',
    entityId: 'msg-456',
    entityName: 'Single Answer',
  };

  it('renders single answer with metadata', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'This is the answer',
      timestamp: '2025-12-12T09:00:00.000Z',
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('# Answer Export');
    expect(markdown).toContain('## Metadata');
    expect(markdown).toContain('- **Export ID**: export-123');
    expect(markdown).toContain('- **Entity Type**: chat_session');
    expect(markdown).toContain('## Answer');
    expect(markdown).toContain('### Content');
    expect(markdown).toContain('This is the answer');
  });

  it('includes message details section', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
      requestId: 'req-123',
      traceId: 'trace-456',
      proofDigest: '0xabcdef',
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('### Message Details');
    expect(markdown).toContain('- **Role**: assistant');
    expect(markdown).toContain('- **Timestamp**:');
    expect(markdown).toContain('- **Request ID**: req-123');
    expect(markdown).toContain('- **Trace ID**: trace-456');
    expect(markdown).toContain('- **Proof Digest**: 0xabcdef');
  });

  it('renders sources with full metadata', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
      evidence: [
        {
          documentId: 'doc-1',
          documentName: 'User Guide',
          chunkId: 'chunk-1',
          pageNumber: 10,
          textPreview: 'This is the relevant text',
          relevanceScore: 0.92,
          rank: 1,
          charRange: { start: 500, end: 600 },
          bbox: { x: 50, y: 100, width: 200, height: 100 },
          citationId: 'cite-1',
        },
      ],
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('### Sources');
    expect(markdown).toContain('#### User Guide (p.10) [92.0% relevance] [cite-1]');
    expect(markdown).toContain('- **Document ID**: doc-1');
    expect(markdown).toContain('- **Chunk ID**: chunk-1');
    expect(markdown).toContain('- **Rank**: 1');
    expect(markdown).toContain('**Preview:**');
    expect(markdown).toContain('> "This is the relevant text"');
    expect(markdown).toContain('- **Character range**: 500-600');
    expect(markdown).toContain('- **Position**: (x: 50.0, y: 100.0, w: 200.0, h: 100.0)');
  });

  it('renders router decision section', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
      routerDecision: {
        requestId: 'req-123',
        selectedAdapters: ['adapter-1'],
        candidates: [
          {
            adapterId: 'adapter-1',
            gateQ15: 32767,
            gateFloat: 1.0,
            selected: true,
          },
        ],
        entropy: 0.5,
      },
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('### Router Decision');
    expect(markdown).toContain('- **Request ID**: req-123');
    expect(markdown).toContain('- **Entropy**: 0.5000');
    expect(markdown).toContain('| Adapter ID | Gate (Q15) | Gate (Float) | Selected |');
    expect(markdown).toContain('| adapter-1 | 32767 | 1.0000 | ✓ |');
  });

  it('renders verification section for verified messages', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
      isVerified: true,
      verifiedAt: '2025-12-12T09:00:10.000Z',
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('### Verification');
    expect(markdown).toContain('- **Verified**: Yes');
    expect(markdown).toContain('- **Verified At**:');
  });

  it('includes adapter stack in metadata', () => {
    const metadata: ExtendedExportMetadata = {
      ...baseMetadata,
      adapterStack: {
        stackId: 'stack-123',
        stackName: 'My Stack',
        adapters: [
          {
            adapterId: 'adapter-1',
            version: 'v1.0',
            gate: 0.75,
          },
        ],
      },
    };

    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
    };

    const markdown = renderSingleAnswerMarkdown(message, metadata);

    expect(markdown).toContain('### Adapter Stack');
    expect(markdown).toContain('- **Stack ID**: stack-123');
    expect(markdown).toContain('- **Stack Name**: My Stack');
    expect(markdown).toContain('| adapter-1 | v1.0 | 0.7500 |');
  });

  it('handles router decision without candidates', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
      routerDecision: {
        requestId: 'req-123',
        selectedAdapters: ['adapter-1', 'adapter-2'],
      },
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('### Router Decision');
    expect(markdown).toContain('**Selected Adapters:**');
    expect(markdown).toContain('- adapter-1');
    expect(markdown).toContain('- adapter-2');
  });

  it('includes export footer with timestamp', () => {
    const message: ExtendedMessageExport = {
      id: 'msg-1',
      role: 'assistant',
      content: 'Answer',
      timestamp: '2025-12-12T09:00:00.000Z',
    };

    const markdown = renderSingleAnswerMarkdown(message, baseMetadata);

    expect(markdown).toContain('---');
    expect(markdown).toContain('*Exported from AdapterOS on');
  });
});
