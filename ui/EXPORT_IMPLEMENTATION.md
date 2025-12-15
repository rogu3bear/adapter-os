# Export to PDF/Markdown with Citations - Implementation Guide

## Overview

This document describes the implementation of export functionality for chat sessions with citation support in the AdapterOS UI, including the **Evidence + Export Pack v1** enhancements.

## Evidence + Export Pack v1 (Latest)

### Evidence Drawer

A collapsible right-side drawer with two tabs for inspecting inference evidence and calculations:

| Tab | Icon | Purpose |
|-----|------|---------|
| **Rulebook** | 📜 | Citations, document highlights, page jumps |
| **Calculation** | 🧮 | Proof summary, routing decisions, token accounting |

**Key Features:**
- Sheet-based drawer (Radix UI)
- Keyboard navigation: `Esc` closes drawer, `Arrow Left/Right` switches tabs
- Optional context pattern for graceful degradation when drawer not available
- Per-message triggers for quick access

### Extended Export Types

Exports now include full provenance metadata:

```typescript
interface ExtendedEvidenceItem {
  documentId: string;
  documentName: string;
  chunkId: string;
  pageNumber: number | null;
  textPreview: string;
  relevanceScore: number;
  rank: number;
  charRange?: { start: number; end: number };  // NEW: Character range for highlighting
  bbox?: { x: number; y: number; width: number; height: number };  // NEW: PDF bounding box
  citationId?: string;  // NEW: Cross-reference identifier
}

interface ExtendedMessageExport {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  requestId?: string;
  traceId?: string;        // NEW: Telemetry trace ID
  proofDigest?: string;    // NEW: Cryptographic proof
  isVerified?: boolean;
  verifiedAt?: string;
  evidence?: ExtendedEvidenceItem[];
  routerDecision?: {
    requestId: string;
    selectedAdapters: string[];
    candidates?: RouterCandidateInfo[];
  };
}
```

### Evidence Bundle Export

New export format with cryptographic integrity verification:

```typescript
interface EvidenceBundleExport {
  schemaVersion: '1.0.0';
  exportTimestamp: string;
  exportId: string;
  traces: Array<{ traceId: string; backendId: string }>;
  evidence: ExtendedEvidenceItem[];
  signatures: Array<{ traceId: string; signature: string; signedAt: string }>;
  checksums: {
    bundleHash: string;  // SHA-256 via Web Crypto API
  };
}
```

### Export Surfaces

| Location | Route | Export Capabilities |
|----------|-------|---------------------|
| Chat Interface | `/chat` | Session export, per-message export, evidence bundle |
| Dataset Chat | `/training/datasets/:id/chat` | Session with dataset_version_id metadata |
| Adapter Version | `/repos/:repoId/versions/:versionId` | Version metadata, lineage, provenance |

---

## What Was Implemented

### 1. Core Export Utilities (`ui/src/utils/export/`)

#### `renderMarkdown.ts`
Provides functions for rendering chat sessions as Markdown with citations:

- **`renderChatSessionMarkdown()`** - Converts chat messages to formatted Markdown
  - Includes metadata (export date, session ID, export ID)
  - Formats conversation with role labels and timestamps
  - Adds evidence/sources with relevance scores
  - Includes router decisions showing adapter weights
  - Adds verification status for verified messages

- **`downloadTextFile()`** - Triggers browser file download
  - Creates blob with appropriate MIME type
  - Handles cleanup of object URLs

- **`generateExportFilename()`** - Creates safe, timestamped filenames
  - Sanitizes special characters
  - Adds timestamp for uniqueness
  - Limits length to prevent filesystem issues

- **`ExportMetadata`** interface - Structured export metadata
  - Tracks export ID, timestamp, entity type/id/name

### 2. Export Components (`ui/src/components/export/`)

#### `ExportActionButton.tsx`
Reusable dropdown button for exporting in multiple formats:

- Dropdown menu with Markdown and JSON export options
- Loading state management during export
- Error handling with toast notifications
- Success feedback via toast messages
- Customizable variant and size props
- Data-testid attributes for E2E testing

#### `ChatSessionExportExample.tsx`
Example integration showing how to use export functionality:

- **`useChatExport()`** hook - Provides export handlers
  - Options for including/excluding metadata, evidence, router decisions
  - Returns handlers for Markdown and JSON export
  - Returns ready-to-use ExportButton component

- **`ChatSessionHeaderWithExport`** component - Example header with export
  - Shows session name and message count
  - Integrates ExportButton seamlessly
  - Demonstrates real-world usage pattern

### 3. Tests (`ui/src/__tests__/export-renderMarkdown.test.ts`)

Comprehensive test suite covering:

- Basic chat session rendering with metadata
- Messages with evidence citations
- Messages with router decisions
- Verified messages with verification timestamps
- Edge cases (missing timestamps, etc.)
- Filename generation and sanitization
- Filename length limits

**Test Results:** 9/9 tests passing ✓

### 4. Documentation

- **`ui/src/components/export/README.md`** - Component usage guide
  - Usage examples for ExportActionButton
  - Integration patterns
  - Instructions for adding PDF export (requires jsPDF)

- **`ui/src/utils/export/index.ts`** - Re-exports with module documentation
  - Centralized export point for all utilities
  - Includes note about PDF support

## File Structure

```
ui/
├── src/
│   ├── components/
│   │   ├── chat/
│   │   │   ├── ChatMessage.tsx                # Per-message export dropdown
│   │   │   ├── EvidenceDrawer.tsx             # Main evidence drawer (Sheet + Tabs)
│   │   │   ├── EvidenceDrawerTrigger.tsx      # Evidence/Proof triggers per message
│   │   │   ├── InlineEvidencePreview.tsx      # 1-3 top citations inline
│   │   │   └── drawer/
│   │   │       ├── RulebookTab.tsx            # Citations, highlights, document jump
│   │   │       └── CalculationTab.tsx         # Receipt, routing, token accounting
│   │   └── export/
│   │       ├── ExportActionButton.tsx         # Main export button component
│   │       ├── ExportDialog.tsx               # Format selection with preview
│   │       ├── ChatSessionExportExample.tsx   # Usage example and hook
│   │       ├── index.ts                       # Component exports
│   │       └── README.md                      # Component documentation
│   ├── contexts/
│   │   └── EvidenceDrawerContext.tsx          # Drawer state management
│   ├── hooks/
│   │   └── useEvidenceDrawer.ts               # Hook for drawer access
│   ├── utils/
│   │   └── export/
│   │       ├── renderMarkdown.ts              # Markdown rendering utilities
│   │       ├── generateEvidenceBundle.ts      # Evidence bundle with SHA-256 checksums
│   │       ├── types.ts                       # Extended export types
│   │       └── index.ts                       # Utility exports
│   ├── pages/
│   │   ├── Training/
│   │   │   └── DatasetChatPage.tsx            # Export button in header
│   │   └── Repositories/
│   │       └── RepoVersionPage.tsx            # Version export button
│   └── __tests__/
│       └── export-renderMarkdown.test.ts      # Test suite
└── EXPORT_IMPLEMENTATION.md                   # This file
```

## How to Use

### Basic Integration Example

```tsx
import { ExportActionButton } from '@/components/export/ExportActionButton';
import {
  renderChatSessionMarkdown,
  downloadTextFile,
  generateExportFilename,
  type ExportMetadata,
} from '@/utils/export';

function ChatHeader({ session, messages }) {
  const handleExportMarkdown = async () => {
    const metadata: ExportMetadata = {
      exportId: crypto.randomUUID(),
      exportTimestamp: new Date().toISOString(),
      entityType: 'chat_session',
      entityId: session.id,
      entityName: session.name,
    };

    const markdown = renderChatSessionMarkdown(
      session.name,
      messages,
      metadata
    );

    const filename = generateExportFilename(session.name, 'md');
    downloadTextFile(markdown, filename, 'text/markdown');
  };

  const handleExportJson = async () => {
    const exportData = {
      session: { id: session.id, name: session.name },
      messages: messages.map(msg => ({
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp.toISOString(),
        evidence: msg.evidence,
      })),
      exported_at: new Date().toISOString(),
    };

    const json = JSON.stringify(exportData, null, 2);
    const filename = generateExportFilename(session.name, 'json');
    downloadTextFile(json, filename, 'application/json');
  };

  return (
    <div className="header">
      <h1>{session.name}</h1>
      <ExportActionButton
        onExportMarkdown={handleExportMarkdown}
        onExportJson={handleExportJson}
        disabled={messages.length === 0}
      />
    </div>
  );
}
```

### Using the Hook (Simpler)

```tsx
import { useChatExport } from '@/components/export';

function ChatInterface({ session, messages }) {
  const { ExportButton } = useChatExport(session, messages);

  return (
    <div>
      <header>
        <h1>{session.name}</h1>
        <ExportButton />
      </header>
      {/* Rest of chat interface */}
    </div>
  );
}
```

### Evidence Drawer Integration

```tsx
import { EvidenceDrawerProvider } from '@/contexts/EvidenceDrawerContext';
import { EvidenceDrawer } from '@/components/chat/EvidenceDrawer';
import { useEvidenceDrawer } from '@/hooks/useEvidenceDrawer';

function ChatInterface({ onViewDocument }) {
  return (
    <EvidenceDrawerProvider>
      <div className="flex flex-col h-full">
        {/* Chat messages */}
        <MessagesArea />

        {/* Evidence drawer - slides in from right */}
        <EvidenceDrawer onViewDocument={onViewDocument} />
      </div>
    </EvidenceDrawerProvider>
  );
}

// In message component - trigger drawer from message
function MessageComponent({ message }) {
  const { openDrawer } = useEvidenceDrawer();

  return (
    <div>
      <p>{message.content}</p>
      <EvidenceDrawerTrigger
        messageId={message.id}
        evidence={message.evidence}
        routerDecision={message.routerDecision}
        traceId={message.traceId}
        proofDigest={message.proofDigest}
      />
    </div>
  );
}
```

### Per-Message Export

Each assistant message has an export dropdown:

```tsx
<DropdownMenu>
  <DropdownMenuTrigger asChild>
    <Button variant="ghost" size="sm">
      <Download className="h-4 w-4" />
    </Button>
  </DropdownMenuTrigger>
  <DropdownMenuContent>
    <DropdownMenuItem onClick={handleExportMarkdown}>
      Export as Markdown
    </DropdownMenuItem>
    <DropdownMenuItem onClick={handleExportJson}>
      Export as JSON
    </DropdownMenuItem>
    <DropdownMenuSeparator />
    <DropdownMenuItem onClick={handleExportEvidenceBundle}>
      Export Evidence Bundle
    </DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

### Evidence Bundle Generation

```tsx
import { generateEvidenceBundle, downloadEvidenceBundle } from '@/utils/export';

async function handleExportEvidenceBundle() {
  const exportMessage = toExportFormat(message);
  const bundle = await generateEvidenceBundle({
    messages: [exportMessage],
    backendId: 'aos-worker',
  });
  downloadEvidenceBundle(bundle, `evidence-${message.id.slice(0, 8)}.json`);
}
```

## Markdown Export Format

The exported Markdown includes extended provenance metadata:

```markdown
# Chat Session: [Session Name]

## Metadata
- **Export Date**: [ISO timestamp]
- **Session ID**: [Session UUID]
- **Export ID**: [Export UUID]
- **Determinism Mode**: deterministic
- **Dataset Version**: [dataset_version_id]  # NEW

## Adapter Stack
| Adapter ID | Version | Gate Weight |
|------------|---------|-------------|
| adapter-1  | v2.1    | 45.2%       |

## Conversation

### **You** ([timestamp])

[User message content]

### **Assistant** ([timestamp])
**Trace ID**: `trace-abc123`           # NEW
**Proof Digest**: `0x1234...5678`       # NEW

[Assistant response content]

**Sources:**
- [Document Name] (p.[page]) [relevance%]
  > "[text preview]"
  - Char Range: 1200-1450              # NEW
  - BBox: x:100, y:200, w:300, h:50    # NEW

**Adapters Used:**
- [Adapter Name] (weight: [percentage]%, Q15: [gate_q15])

*Verified at [timestamp]*

---

*Exported from AdapterOS on [date/time]*
```

## JSON Export Format

The JSON export includes extended provenance metadata:

```json
{
  "metadata": {
    "exportId": "export-abc123",
    "exportTimestamp": "2025-01-15T10:15:30Z",
    "entityType": "chat_session",
    "entityId": "session-xyz",
    "entityName": "Research Session"
  },
  "session": {
    "id": "session-xyz",
    "name": "Research Session",
    "stack_id": "stack-001",
    "dataset_version_id": "dv-123",
    "created_at": "2025-01-15T10:00:00Z",
    "tenant_id": "tenant-abc"
  },
  "messages": [
    {
      "id": "msg-001",
      "role": "assistant",
      "content": "Response content...",
      "timestamp": "2025-01-15T10:15:28Z",
      "traceId": "trace-abc123",
      "proofDigest": "0x1234abcd...",
      "isVerified": true,
      "verifiedAt": "2025-01-15T10:15:30Z",
      "evidence": [
        {
          "documentId": "doc-001",
          "documentName": "Report.pdf",
          "chunkId": "chunk-42",
          "pageNumber": 15,
          "textPreview": "Relevant text...",
          "relevanceScore": 0.95,
          "rank": 1,
          "charRange": { "start": 1200, "end": 1450 },
          "bbox": { "x": 100, "y": 200, "width": 300, "height": 50 },
          "citationId": "cite-1"
        }
      ],
      "routerDecision": {
        "requestId": "req-001",
        "selectedAdapters": ["adapter-1", "adapter-2"],
        "candidates": [
          {
            "adapterId": "adapter-1",
            "gateQ15": 21348,
            "gateFloat": 0.652,
            "selected": true
          }
        ]
      }
    }
  ]
}
```

## Evidence Bundle Export Format

```json
{
  "schemaVersion": "1.0.0",
  "exportTimestamp": "2025-01-15T10:15:30Z",
  "exportId": "export-abc123-xyz789",
  "traces": [
    { "traceId": "trace-abc123", "backendId": "aos-worker" }
  ],
  "evidence": [
    {
      "documentId": "doc-001",
      "documentName": "Report.pdf",
      "chunkId": "chunk-42",
      "pageNumber": 15,
      "textPreview": "Relevant text...",
      "relevanceScore": 0.95,
      "rank": 1,
      "charRange": { "start": 1200, "end": 1450 },
      "bbox": { "x": 100, "y": 200, "width": 300, "height": 50 }
    }
  ],
  "signatures": [
    {
      "traceId": "trace-abc123",
      "signature": "0x1234abcd...",
      "signedAt": "2025-01-15T10:15:30Z"
    }
  ],
  "checksums": {
    "bundleHash": "0xabcdef1234567890..."
  }
}
```

**Security Note**: The `bundleHash` is computed client-side using SHA-256 via Web Crypto API for export integrity verification only. Real cryptographic proofs come from the backend's BLAKE3 + Ed25519 signing.

## Dependencies Check

### Currently Installed ✓
- `lucide-react` - Icons (Download, FileText, FileJson)
- `sonner` - Toast notifications
- `@radix-ui/react-dropdown-menu` - Dropdown menu component
- All core React dependencies

### Not Installed (Optional)
- `jspdf` - Required only if PDF export is needed
  - Installation: `pnpm add jspdf`
  - See `ui/src/components/export/README.md` for PDF implementation guide

## Testing

Run tests:
```bash
cd ui
pnpm vitest run src/__tests__/export-renderMarkdown.test.ts
```

All 9 tests passing:
- ✓ Basic chat session rendering
- ✓ Messages with evidence citations
- ✓ Messages with router decisions
- ✓ Verified messages
- ✓ Edge cases
- ✓ Filename generation
- ✓ Filename sanitization
- ✓ Filename length limits
- ✓ Multiple consecutive special characters

## Where to Add Export Functionality

### 1. Chat Interface (`ui/src/components/ChatInterface.tsx`)
Add export button to chat session header for exporting entire conversations.

### 2. Document Library (`ui/src/pages/DocumentLibrary/`)
Export document-based chat sessions with citations.

### 3. Telemetry Viewer (`ui/src/pages/TelemetryPage.tsx`)
Export inference traces and chat sessions for audit purposes.

### 4. Replay Interface (`ui/src/pages/Replay/ReplayShell.tsx`)
Export replay results with determinism evidence.

## Completed Features (Evidence + Export Pack v1)

### Evidence Drawer ✓
- [x] EvidenceDrawerContext with state management
- [x] Sheet-based drawer with Rulebook/Calculation tabs
- [x] RulebookTab with citations, page numbers, relevance scores
- [x] CalculationTab with proof summary, routing decisions, gate weights
- [x] Keyboard navigation (Esc closes, Arrow Left/Right switches tabs)
- [x] EvidenceDrawerTrigger for per-message access
- [x] InlineEvidencePreview showing 1-3 top citations
- [x] Optional context pattern for graceful degradation

### Extended Export Types ✓
- [x] traceId and proofDigest in message exports
- [x] charRange for text highlighting
- [x] bbox for PDF coordinate highlighting
- [x] citationId for cross-references
- [x] RouterCandidateInfo with gate_q15 and gate_float

### Export Surfaces ✓
- [x] Per-message export dropdown in ChatMessage.tsx
- [x] Export button in DatasetChatPage header
- [x] Export button in RepoVersionPage
- [x] ExportDialog with format selection

### Evidence Bundle ✓
- [x] generateEvidenceBundle with SHA-256 checksums (Web Crypto API)
- [x] downloadEvidenceBundle helper
- [x] Trace collection with backend IDs
- [x] Signature placeholders for verified messages

---

## Next Steps (Optional Enhancements)

### 1. Add PDF Export
- Install jsPDF: `pnpm add jspdf`
- Create `ui/src/utils/export/renderPdf.ts`
- Follow guide in `ui/src/components/export/README.md`

### 2. Add CSV Export
For tabular data (evidence lists, adapter weights):
```typescript
export function renderEvidenceCsv(evidence: EvidenceItem[]): string {
  let csv = 'Document,Page,Relevance,Preview\n';
  for (const ev of evidence) {
    csv += `"${ev.document_name}",${ev.page_number || ''},${ev.relevance_score},"${ev.text_preview}"\n`;
  }
  return csv;
}
```

### 3. Add HTML Export
For rich formatting with embedded styles:
```typescript
export function renderChatSessionHtml(
  sessionName: string,
  messages: ChatMessage[],
  metadata: ExportMetadata
): string {
  // Convert Markdown to HTML with styles
  // Add citation links, syntax highlighting, etc.
}
```

### 4. Add Export Settings
Allow users to customize what's included:
- Include/exclude evidence
- Include/exclude router decisions
- Include/exclude metadata
- Date range filtering

### 5. Add Batch Export
Export multiple sessions or datasets at once:
- ZIP archive creation
- Progress indicators
- Cancellation support

## Integration with Existing Features

The export functionality integrates seamlessly with:

- **Evidence Panel** (`ui/src/components/chat/EvidencePanel.tsx`)
  - Similar export pattern used for evidence sources
  - Consistent user experience

- **Chat Sessions** (`ui/src/types/chat.ts`)
  - Uses existing ChatMessage and ChatSession types
  - No type changes required

- **API Client** (`ui/src/api/client.ts`)
  - Export uses client-side data only
  - No new API endpoints needed

- **E2E Testing**
  - Data-testid attributes added for Cypress tests
  - Ready for E2E test coverage

## Security Considerations

1. **Client-Side Only**: All export operations happen in browser
2. **No Server Upload**: Exported data never leaves user's machine
3. **Sanitization**: Filenames are sanitized to prevent path traversal
4. **MIME Types**: Proper MIME types prevent browser security warnings
5. **Blob Cleanup**: Object URLs are properly revoked after use

## Performance Notes

- **Efficient**: Markdown rendering is O(n) with message count
- **Memory**: Large sessions are handled via streaming download
- **No Blocking**: Export operations are async
- **Cancel Support**: Easy to add cancellation for large exports

## Maintenance

All code follows AdapterOS conventions:
- TypeScript with strict typing
- JSDoc comments for public APIs
- Comprehensive test coverage
- Consistent error handling
- Accessible UI components

## Support

For questions or issues:
1. Check `ui/src/components/export/README.md` for usage examples
2. Review test file for expected behavior
3. See `ChatSessionExportExample.tsx` for integration patterns

---

**Original Implementation Date**: 2025-12-12
**Evidence + Export Pack v1**: 2025-12-12
**Status**: Complete and tested ✓
**Test Coverage**: 9/9 core tests passing

### Key Files Modified in v1
| File | Change |
|------|--------|
| `ChatMessage.tsx` | Extended interfaces, per-message export dropdown |
| `ChatInterface.tsx` | EvidenceDrawerProvider wrapper |
| `generateEvidenceBundle.ts` | SHA-256 via Web Crypto API |
| `DatasetChatPage.tsx` | Header export button |
| `RepoVersionPage.tsx` | Version export button |
| `CalculationTab.tsx` | Fixed query param for telemetry navigation |
