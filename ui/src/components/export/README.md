# Export Components

This directory contains reusable export components for AdapterOS UI.

## Components

### ExportActionButton

A dropdown button component for exporting content in multiple formats.

**Features:**
- Export as Markdown with citations
- Export as JSON for machine-readable data
- Export as PDF for professional document sharing
- Loading states and error handling
- Toast notifications for user feedback

**Usage Example:**

```tsx
import { ExportActionButton } from '@/components/export/ExportActionButton';
import {
  renderChatSessionMarkdown,
  downloadTextFile,
  generateExportFilename,
  generateChatSessionPdf,
  downloadPdfFile,
  type ExportMetadata,
} from '@/utils/export';
import type { ChatMessage } from '@/components/chat/ChatMessage';

function ChatSessionHeader({ session, messages }: {
  session: { id: string; name: string };
  messages: ChatMessage[]
}) {
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
      session: {
        id: session.id,
        name: session.name,
        exported_at: new Date().toISOString(),
      },
      messages: messages.map(msg => ({
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp.toISOString(),
        evidence: msg.evidence,
        router_decision: msg.routerDecision,
      })),
    };

    const json = JSON.stringify(exportData, null, 2);
    const filename = generateExportFilename(session.name, 'json');
    downloadTextFile(json, filename, 'application/json');
  };

  const handleExportPdf = async () => {
    const metadata: ExportMetadata = {
      exportId: crypto.randomUUID(),
      exportTimestamp: new Date().toISOString(),
      entityType: 'chat_session',
      entityId: session.id,
      entityName: session.name,
    };

    const pdfBlob = await generateChatSessionPdf(
      session.name,
      messages,
      metadata
    );

    const filename = generateExportFilename(session.name, 'pdf');
    downloadPdfFile(pdfBlob, filename);
  };

  return (
    <div className="flex items-center justify-between">
      <h1>{session.name}</h1>
      <ExportActionButton
        onExportMarkdown={handleExportMarkdown}
        onExportJson={handleExportJson}
        onExportPdf={handleExportPdf}
        disabled={messages.length === 0}
      />
    </div>
  );
}
```

## Utilities

See `/src/utils/export/` for export utility functions:

- `renderChatSessionMarkdown()` - Convert chat to Markdown with citations
- `generateChatSessionPdf()` - Convert chat to PDF with formatted tables
- `downloadTextFile()` - Trigger browser download for text files
- `downloadPdfFile()` - Trigger browser download for PDF files
- `generateExportFilename()` - Create safe, timestamped filenames

## Export Formats

### Markdown Export
- Human-readable format with full conversation history
- Includes evidence/sources with relevance scores
- Router decision information with adapter weights
- Verification timestamps
- Perfect for documentation and archiving

### JSON Export
- Machine-readable structured data
- Complete session metadata
- Full message history with all fields
- Evidence and router decisions preserved
- Ideal for data processing and migration

### PDF Export
- Professional document format
- Formatted tables for metadata and messages
- Truncated message preview (first 200 characters)
- Timestamps and role information
- Great for sharing and reporting

## Implementation Details

The PDF export uses jsPDF with jspdf-autotable for table generation:
- Session title and metadata table
- Messages displayed in a formatted table
- Automatic page breaks and text wrapping
- Responsive column widths
- Timestamped exports with unique IDs
