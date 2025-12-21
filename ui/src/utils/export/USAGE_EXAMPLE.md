# Export Types and Functions - Usage Examples

This document provides usage examples for the extended export types and markdown rendering functions.

## Extended Export Types

### ExtendedExportMetadata

Includes additional fields for determinism tracking, dataset versioning, and adapter stack information:

```typescript
import type { ExtendedExportMetadata } from '@/utils/export';

const metadata: ExtendedExportMetadata = {
  exportId: 'export-abc123',
  exportTimestamp: '2025-12-12T10:00:00Z',
  entityType: 'chat_session',
  entityId: 'session-xyz789',
  entityName: 'Customer Support Chat',

  // Extended fields
  determinismMode: 'deterministic',
  determinismState: 'verified',
  datasetVersionId: 'dataset-v1.2.3',
  tenantId: 'tenant-001',
  collectionId: 'collection-support',

  adapterStack: {
    stackId: 'stack-customer-support',
    stackName: 'Customer Support Stack',
    adapters: [
      {
        adapterId: 'adapter-sentiment',
        version: 'v2.1.0',
        gate: 0.6523,
      },
      {
        adapterId: 'adapter-product-knowledge',
        version: 'v1.5.2',
        gate: 0.3477,
      },
    ],
  },
};
```

### ExtendedMessageExport

Includes trace IDs, proof digests, and detailed evidence with bounding boxes:

```typescript
import type { ExtendedMessageExport } from '@/utils/export';

const message: ExtendedMessageExport = {
  id: 'msg-001',
  role: 'assistant',
  content: 'Based on the documentation, the return policy is 30 days.',
  timestamp: '2025-12-12T10:05:00Z',

  // Extended trace metadata
  requestId: 'req-2025-12-12-001',
  traceId: 'trace-blake3-abcd1234',
  proofDigest: 'blake3:5f3a2b1c...',

  evidence: [
    {
      documentId: 'doc-return-policy',
      documentName: 'Return Policy 2025',
      chunkId: 'chunk-rp-003',
      pageNumber: 2,
      textPreview: 'All items may be returned within 30 days of purchase...',
      relevanceScore: 0.95,
      rank: 1,

      // Extended evidence metadata
      charRange: { start: 145, end: 287 },
      bbox: { x: 72.5, y: 356.2, width: 450.8, height: 48.3 },
      citationId: '[1]',
    },
  ],

  routerDecision: {
    requestId: 'req-2025-12-12-001',
    selectedAdapters: ['adapter-policy-expert'],
    candidates: [
      {
        adapterId: 'adapter-policy-expert',
        gateQ15: 21299,  // Q15 quantized value
        gateFloat: 0.6500,
        selected: true,
      },
      {
        adapterId: 'adapter-general',
        gateQ15: 11468,
        gateFloat: 0.3500,
        selected: false,
      },
    ],
    entropy: 0.0823,
  },

  isVerified: true,
  verifiedAt: '2025-12-12T10:05:01Z',
};
```

## Markdown Rendering Functions

### renderExtendedChatSessionMarkdown

Exports a full chat session with extended metadata:

```typescript
import {
  renderExtendedChatSessionMarkdown,
  downloadTextFile,
  generateExportFilename,
} from '@/utils/export';

// Export a session with full audit trail
const markdown = renderExtendedChatSessionMarkdown(
  'Customer Support - Order #12345',
  messages,  // Array of ExtendedMessageExport
  metadata   // ExtendedExportMetadata
);

// Download the file
const filename = generateExportFilename('customer-support-12345', 'md');
downloadTextFile(markdown, filename, 'text/markdown');
```

**Output includes:**
- Session metadata with determinism mode/state
- Dataset version ID
- Adapter stack table with gates
- Per-message trace IDs and proof digests
- Evidence with character ranges and bounding boxes
- Router decisions with Q15 gates

### renderSingleAnswerMarkdown

Exports a single answer with complete context:

```typescript
import {
  renderSingleAnswerMarkdown,
  downloadTextFile,
  generateExportFilename,
} from '@/utils/export';

// Export a single answer
const markdown = renderSingleAnswerMarkdown(
  message,   // ExtendedMessageExport
  metadata   // ExtendedExportMetadata
);

// Download the file
const filename = generateExportFilename('answer-export', 'md');
downloadTextFile(markdown, filename, 'text/markdown');
```

**Output includes:**
- Full export metadata
- Adapter stack configuration
- Message details with trace metadata
- Detailed evidence breakdown per source
- Router decision table
- Verification status

## Evidence Bundle Export

For audit and compliance purposes:

```typescript
import type { EvidenceBundleExport } from '@/utils/export';

const bundle: EvidenceBundleExport = {
  schemaVersion: '1.0.0',
  exportTimestamp: '2025-12-12T10:00:00Z',
  exportId: 'bundle-compliance-2025-q4',

  traces: [
    {
      traceId: 'trace-blake3-001',
      backendId: 'backend-coreml-01',
    },
  ],

  evidence: [
    // Array of ExtendedEvidenceItem
  ],

  signatures: [
    {
      traceId: 'trace-blake3-001',
      signature: 'ed25519:abc123...',
      signedAt: '2025-12-12T10:00:01Z',
    },
  ],

  checksums: {
    bundleHash: 'blake3:5f3a2b1c...',
  },
};

// Export as JSON
const json = JSON.stringify(bundle, null, 2);
downloadTextFile(json, 'evidence-bundle.json', 'application/json');
```

## Example Markdown Output

### Extended Chat Session

```markdown
# Chat Session: Customer Support - Order #12345

## Metadata
- **Export Date**: 12/12/2025, 10:00:00 AM
- **Session ID**: session-xyz789
- **Export ID**: export-abc123
- **Determinism Mode**: deterministic
- **Determinism State**: verified
- **Dataset Version ID**: dataset-v1.2.3
- **Tenant ID**: tenant-001
- **Collection ID**: collection-support

### Adapter Stack
- **Stack ID**: stack-customer-support
- **Stack Name**: Customer Support Stack

| Adapter ID | Version | Gate |
|------------|---------|------|
| adapter-sentiment | v2.1.0 | 0.6523 |
| adapter-product-knowledge | v1.5.2 | 0.3477 |

## Conversation

### **Assistant** (12/12/2025, 10:05:00 AM)

*Request ID: req-2025-12-12-001*
*Trace ID: trace-blake3-abcd1234*
*Proof Digest: blake3:5f3a2b1c...*

Based on the documentation, the return policy is 30 days.

**Sources:**
- Return Policy 2025 (p.2) [95.0% relevance] [1]
  > "All items may be returned within 30 days of purchase..."
  - Character range: 145-287
  - Position: (x: 72.5, y: 356.2, w: 450.8, h: 48.3)

**Router Decision:**
- Entropy: 0.0823

| Adapter ID | Gate (Q15) | Gate (Float) | Selected |
|------------|------------|--------------|----------|
| adapter-policy-expert | 21299 | 0.6500 | ✓ |
| adapter-general | 11468 | 0.3500 | ✗ |

*Verified at 12/12/2025, 10:05:01 AM*

---

*Exported from AdapterOS on 12/12/2025, 10:00:00 AM*
```

## Integration with Components

```typescript
// In a React component
import { renderExtendedChatSessionMarkdown } from '@/utils/export';

function ExportButton({ session, messages }) {
  const handleExport = () => {
    const metadata: ExtendedExportMetadata = {
      exportId: crypto.randomUUID(),
      exportTimestamp: new Date().toISOString(),
      entityType: 'chat_session',
      entityId: session.id,
      entityName: session.name,
      determinismMode: session.determinismMode,
      determinismState: session.determinismState,
      // ... other fields
    };

    const markdown = renderExtendedChatSessionMarkdown(
      session.name,
      messages,
      metadata
    );

    const filename = generateExportFilename(session.name, 'md');
    downloadTextFile(markdown, filename, 'text/markdown');
  };

  return <button onClick={handleExport}>Export Session</button>;
}
```

## Notes

- The original `renderChatSessionMarkdown` function remains unchanged for backwards compatibility
- All new types are optional, so existing code continues to work
- Q15 gates are displayed alongside float values for audit purposes
- Character ranges and bounding boxes enable precise evidence verification
- Proof digests use BLAKE3 hashing for cryptographic verification
