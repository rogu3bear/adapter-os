import type { ChatMessage } from '@/components/chat/ChatMessage';
import type {
  ExtendedExportMetadata,
  ExtendedMessageExport,
} from './types';

export interface ExportMetadata {
  exportId: string;
  exportTimestamp: string;
  entityType: 'chat_session' | 'dataset' | 'adapter';
  entityId: string;
  entityName: string;
}

/**
 * Renders a chat session as formatted Markdown with citations
 *
 * @param sessionName - Name of the chat session
 * @param messages - Array of chat messages
 * @param metadata - Export metadata for tracking
 * @returns Formatted Markdown string
 */
export function renderChatSessionMarkdown(
  sessionName: string,
  messages: ChatMessage[],
  metadata: ExportMetadata
): string {
  let md = `# Chat Session: ${sessionName}\n\n`;
  md += `## Metadata\n`;
  md += `- **Export Date**: ${metadata.exportTimestamp}\n`;
  md += `- **Session ID**: ${metadata.entityId}\n`;
  md += `- **Export ID**: ${metadata.exportId}\n\n`;
  md += `## Conversation\n\n`;

  for (const msg of messages) {
    const role = msg.role === 'user' ? '**You**' : '**Assistant**';
    const time = msg.timestamp ? new Date(msg.timestamp).toLocaleString() : '';
    md += `### ${role} ${time ? `(${time})` : ''}\n\n`;
    md += `${msg.content}\n\n`;

    // Add evidence/sources if available
    if (msg.evidence && msg.evidence.length > 0) {
      md += `**Sources:**\n`;
      for (const ev of msg.evidence) {
        const pageInfo = ev.page_number ? ` (p.${ev.page_number})` : '';
        const score = ev.relevance_score
          ? ` [${(ev.relevance_score * 100).toFixed(1)}% relevance]`
          : '';
        md += `- ${ev.document_name}${pageInfo}${score}\n`;
        if (ev.text_preview) {
          md += `  > "${ev.text_preview}"\n`;
        }
      }
      md += '\n';
    }

    // Add router decision info if available
    if (msg.routerDecision) {
      // Show selected adapters with scores if available
      if (msg.routerDecision.candidates && msg.routerDecision.candidates.length > 0) {
        md += `**Adapters Used:**\n`;
        const selectedCandidates = msg.routerDecision.candidates.filter(c => c.selected);
        for (const candidate of selectedCandidates) {
          const weight = candidate.gate_float
            ? ` (weight: ${(candidate.gate_float * 100).toFixed(1)}%)`
            : '';
          md += `- Adapter ${candidate.adapter_id}${weight}\n`;
        }
        md += '\n';
      } else if (msg.routerDecision.selected_adapters && msg.routerDecision.selected_adapters.length > 0) {
        // Fallback to simple adapter IDs if candidates not available
        md += `**Adapters Used:**\n`;
        for (const adapterId of msg.routerDecision.selected_adapters) {
          const score = msg.routerDecision.scores?.[adapterId];
          const scoreText = score ? ` (score: ${score.toFixed(3)})` : '';
          md += `- ${adapterId}${scoreText}\n`;
        }
        md += '\n';
      }
    }

    // Add verification status if verified
    if (msg.isVerified && msg.verifiedAt) {
      md += `*Verified at ${new Date(msg.verifiedAt).toLocaleString()}*\n\n`;
    }
  }

  md += `---\n\n`;
  md += `*Exported from AdapterOS on ${new Date(metadata.exportTimestamp).toLocaleString()}*\n`;

  return md;
}

/**
 * Creates a download link and triggers download of text content
 *
 * @param content - Content to download
 * @param filename - Name of the file to download
 * @param mimeType - MIME type of the content
 */
export function downloadTextFile(
  content: string,
  filename: string,
  mimeType: string = 'text/plain'
): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Generates a safe filename from a session name and timestamp
 *
 * @param sessionName - Name of the session
 * @param extension - File extension (e.g., 'md', 'json')
 * @returns Safe filename
 */
export function generateExportFilename(
  sessionName: string,
  extension: string
): string {
  const safeName = sessionName
    .replace(/[^a-z0-9]/gi, '-')
    .replace(/-+/g, '-')
    .toLowerCase()
    .substring(0, 50);
  const timestamp = new Date().toISOString().slice(0, 10);
  return `${safeName}-${timestamp}.${extension}`;
}

/**
 * Renders a chat session with extended metadata as formatted Markdown
 *
 * @param sessionName - Name of the chat session
 * @param messages - Array of extended chat messages with full metadata
 * @param metadata - Extended export metadata
 * @returns Formatted Markdown string with full audit trail
 */
export function renderExtendedChatSessionMarkdown(
  sessionName: string,
  messages: ExtendedMessageExport[],
  metadata: ExtendedExportMetadata
): string {
  let md = `# Chat Session: ${sessionName}\n\n`;

  md += `## Metadata\n`;
  md += `- **Export Date**: ${metadata.exportTimestamp}\n`;
  md += `- **Session ID**: ${metadata.entityId}\n`;
  md += `- **Export ID**: ${metadata.exportId}\n`;

  if (metadata.determinismMode) {
    md += `- **Determinism Mode**: ${metadata.determinismMode}\n`;
  }
  if (metadata.determinismState) {
    md += `- **Determinism State**: ${metadata.determinismState}\n`;
  }
  if (metadata.datasetVersionId) {
    md += `- **Dataset Version ID**: ${metadata.datasetVersionId}\n`;
  }
  if (metadata.tenantId) {
    md += `- **Workspace ID**: ${metadata.tenantId}\n`;
  }
  if (metadata.collectionId) {
    md += `- **Collection ID**: ${metadata.collectionId}\n`;
  }

  // Add adapter stack table if available
  if (metadata.adapterStack && metadata.adapterStack.adapters.length > 0) {
    md += `\n### Adapter Stack\n`;
    md += `- **Stack ID**: ${metadata.adapterStack.stackId}\n`;
    if (metadata.adapterStack.stackName) {
      md += `- **Stack Name**: ${metadata.adapterStack.stackName}\n`;
    }
    md += `\n| Adapter ID | Version | Gate |\n`;
    md += `|------------|---------|------|\n`;
    for (const adapter of metadata.adapterStack.adapters) {
      const version = adapter.version || 'N/A';
      const gate = adapter.gate !== undefined
        ? adapter.gate.toFixed(4)
        : 'N/A';
      md += `| ${adapter.adapterId} | ${version} | ${gate} |\n`;
    }
  }

  md += `\n## Conversation\n\n`;

  for (const msg of messages) {
    const role = msg.role === 'user' ? '**You**' : '**Assistant**';
    const time = msg.timestamp ? new Date(msg.timestamp).toLocaleString() : '';
    md += `### ${role} ${time ? `(${time})` : ''}\n\n`;

    // Add trace metadata
    if (msg.requestId) {
      md += `*Request ID: ${msg.requestId}*\n`;
    }
    if (msg.traceId) {
      md += `*Trace ID: ${msg.traceId}*\n`;
    }
    if (msg.proofDigest) {
      md += `*Proof Digest: ${msg.proofDigest}*\n`;
    }
    if (msg.requestId || msg.traceId || msg.proofDigest) {
      md += `\n`;
    }

    md += `${msg.content}\n\n`;

    // Add per-turn adapter stack if available
    if (msg.adapterStackSnapshot) {
      md += `**Adapter Stack:**\n`;
      md += `| Adapter ID | Version | Gate |\n`;
      md += `|------------|---------|------|\n`;
      for (const adapter of msg.adapterStackSnapshot.adapters) {
        const version = adapter.version || 'N/A';
        const gate = adapter.gate !== undefined ? adapter.gate.toFixed(4) : 'N/A';
        md += `| ${adapter.adapterId} | ${version} | ${gate} |\n`;
      }
      md += `\n`;
    }

    // Add per-turn dataset version if available
    if (msg.datasetVersionId) {
      md += `*Dataset Version: ${msg.datasetVersionId}*\n\n`;
    }

    // Add evidence/sources with extended metadata
    if (msg.evidence && msg.evidence.length > 0) {
      md += `**Sources:**\n`;
      for (const ev of msg.evidence) {
        const pageInfo = ev.pageNumber ? ` (p.${ev.pageNumber})` : '';
        const score = ev.relevanceScore
          ? ` [${(ev.relevanceScore * 100).toFixed(1)}% relevance]`
          : '';
        const citation = ev.citationId ? ` [${ev.citationId}]` : '';
        md += `- ${ev.documentName}${pageInfo}${score}${citation}\n`;

        if (ev.textPreview) {
          md += `  > "${ev.textPreview}"\n`;
        }

        // Add character range if available
        if (ev.charRange) {
          md += `  - Character range: ${ev.charRange.start}-${ev.charRange.end}\n`;
        }

        // Add bounding box if available
        if (ev.bbox) {
          md += `  - Position: (x: ${ev.bbox.x.toFixed(1)}, y: ${ev.bbox.y.toFixed(1)}, w: ${ev.bbox.width.toFixed(1)}, h: ${ev.bbox.height.toFixed(1)})\n`;
        }
      }
      md += '\n';
    }

    // Add router decision info with Q15 gates
    if (msg.routerDecision) {
      if (msg.routerDecision.candidates && msg.routerDecision.candidates.length > 0) {
        md += `**Router Decision:**\n`;
        if (msg.routerDecision.entropy !== undefined) {
          md += `- Entropy: ${msg.routerDecision.entropy.toFixed(4)}\n`;
        }
        md += `\n| Adapter ID | Gate (Q15) | Gate (Float) | Selected |\n`;
        md += `|------------|------------|--------------|----------|\n`;
        for (const candidate of msg.routerDecision.candidates) {
          const selected = candidate.selected ? '✓' : '✗';
          md += `| ${candidate.adapterId} | ${candidate.gateQ15} | ${candidate.gateFloat.toFixed(4)} | ${selected} |\n`;
        }
        md += '\n';
      } else if (msg.routerDecision.selectedAdapters && msg.routerDecision.selectedAdapters.length > 0) {
        md += `**Adapters Used:**\n`;
        for (const adapterId of msg.routerDecision.selectedAdapters) {
          md += `- ${adapterId}\n`;
        }
        md += '\n';
      }
    }

    // Add verification status if verified
    if (msg.isVerified && msg.verifiedAt) {
      md += `*Verified at ${new Date(msg.verifiedAt).toLocaleString()}*\n\n`;
    }
  }

  md += `---\n\n`;
  md += `*Exported from AdapterOS on ${new Date(metadata.exportTimestamp).toLocaleString()}*\n`;

  return md;
}

/**
 * Renders a single answer with full metadata as formatted Markdown
 *
 * @param message - Extended message export with full metadata
 * @param metadata - Extended export metadata
 * @returns Formatted Markdown string for single answer
 */
export function renderSingleAnswerMarkdown(
  message: ExtendedMessageExport,
  metadata: ExtendedExportMetadata
): string {
  let md = `# Answer Export\n\n`;

  md += `## Metadata\n`;
  md += `- **Export Date**: ${metadata.exportTimestamp}\n`;
  md += `- **Export ID**: ${metadata.exportId}\n`;
  md += `- **Entity Type**: ${metadata.entityType}\n`;
  md += `- **Entity ID**: ${metadata.entityId}\n`;
  md += `- **Entity Name**: ${metadata.entityName}\n`;

  if (metadata.determinismMode) {
    md += `- **Determinism Mode**: ${metadata.determinismMode}\n`;
  }
  if (metadata.determinismState) {
    md += `- **Determinism State**: ${metadata.determinismState}\n`;
  }
  if (metadata.datasetVersionId) {
    md += `- **Dataset Version ID**: ${metadata.datasetVersionId}\n`;
  }
  if (metadata.tenantId) {
    md += `- **Workspace ID**: ${metadata.tenantId}\n`;
  }
  if (metadata.collectionId) {
    md += `- **Collection ID**: ${metadata.collectionId}\n`;
  }

  // Add adapter stack table if available
  if (metadata.adapterStack && metadata.adapterStack.adapters.length > 0) {
    md += `\n### Adapter Stack\n`;
    md += `- **Stack ID**: ${metadata.adapterStack.stackId}\n`;
    if (metadata.adapterStack.stackName) {
      md += `- **Stack Name**: ${metadata.adapterStack.stackName}\n`;
    }
    md += `\n| Adapter ID | Version | Gate |\n`;
    md += `|------------|---------|------|\n`;
    for (const adapter of metadata.adapterStack.adapters) {
      const version = adapter.version || 'N/A';
      const gate = adapter.gate !== undefined
        ? adapter.gate.toFixed(4)
        : 'N/A';
      md += `| ${adapter.adapterId} | ${version} | ${gate} |\n`;
    }
  }

  md += `\n## Answer\n\n`;

  // Add message metadata
  md += `### Message Details\n`;
  md += `- **Role**: ${message.role}\n`;
  md += `- **Timestamp**: ${new Date(message.timestamp).toLocaleString()}\n`;

  if (message.requestId) {
    md += `- **Request ID**: ${message.requestId}\n`;
  }
  if (message.traceId) {
    md += `- **Trace ID**: ${message.traceId}\n`;
  }
  if (message.proofDigest) {
    md += `- **Proof Digest**: ${message.proofDigest}\n`;
  }

  md += `\n### Content\n\n`;
  md += `${message.content}\n\n`;

  // Add per-turn adapter stack if available
  if (message.adapterStackSnapshot) {
    md += `### Adapter Stack (This Turn)\n\n`;
    md += `| Adapter ID | Version | Gate |\n`;
    md += `|------------|---------|------|\n`;
    for (const adapter of message.adapterStackSnapshot.adapters) {
      const version = adapter.version || 'N/A';
      const gate = adapter.gate !== undefined ? adapter.gate.toFixed(4) : 'N/A';
      md += `| ${adapter.adapterId} | ${version} | ${gate} |\n`;
    }
    md += `\n`;
  }

  // Add per-turn dataset version if available
  if (message.datasetVersionId) {
    md += `**Dataset Version (This Turn):** ${message.datasetVersionId}\n\n`;
  }

  // Add evidence/sources with extended metadata
  if (message.evidence && message.evidence.length > 0) {
    md += `### Sources\n\n`;
    for (const ev of message.evidence) {
      const pageInfo = ev.pageNumber ? ` (p.${ev.pageNumber})` : '';
      const score = ev.relevanceScore
        ? ` [${(ev.relevanceScore * 100).toFixed(1)}% relevance]`
        : '';
      const citation = ev.citationId ? ` [${ev.citationId}]` : '';
      md += `#### ${ev.documentName}${pageInfo}${score}${citation}\n\n`;

      md += `- **Document ID**: ${ev.documentId}\n`;
      md += `- **Chunk ID**: ${ev.chunkId}\n`;
      md += `- **Rank**: ${ev.rank}\n`;

      if (ev.textPreview) {
        md += `\n**Preview:**\n> "${ev.textPreview}"\n\n`;
      }

      // Add character range if available
      if (ev.charRange) {
        md += `- **Character range**: ${ev.charRange.start}-${ev.charRange.end}\n`;
      }

      // Add bounding box if available
      if (ev.bbox) {
        md += `- **Position**: (x: ${ev.bbox.x.toFixed(1)}, y: ${ev.bbox.y.toFixed(1)}, w: ${ev.bbox.width.toFixed(1)}, h: ${ev.bbox.height.toFixed(1)})\n`;
      }

      md += `\n`;
    }
  }

  // Add router decision info with Q15 gates
  if (message.routerDecision) {
    md += `### Router Decision\n\n`;
    md += `- **Request ID**: ${message.routerDecision.requestId}\n`;

    if (message.routerDecision.entropy !== undefined) {
      md += `- **Entropy**: ${message.routerDecision.entropy.toFixed(4)}\n`;
    }

    if (message.routerDecision.candidates && message.routerDecision.candidates.length > 0) {
      md += `\n| Adapter ID | Gate (Q15) | Gate (Float) | Selected |\n`;
      md += `|------------|------------|--------------|----------|\n`;
      for (const candidate of message.routerDecision.candidates) {
        const selected = candidate.selected ? '✓' : '✗';
        md += `| ${candidate.adapterId} | ${candidate.gateQ15} | ${candidate.gateFloat.toFixed(4)} | ${selected} |\n`;
      }
    } else if (message.routerDecision.selectedAdapters && message.routerDecision.selectedAdapters.length > 0) {
      md += `\n**Selected Adapters:**\n`;
      for (const adapterId of message.routerDecision.selectedAdapters) {
        md += `- ${adapterId}\n`;
      }
    }
    md += '\n';
  }

  // Add verification status if verified
  if (message.isVerified && message.verifiedAt) {
    md += `### Verification\n\n`;
    md += `- **Verified**: Yes\n`;
    md += `- **Verified At**: ${new Date(message.verifiedAt).toLocaleString()}\n\n`;
  }

  md += `---\n\n`;
  md += `*Exported from AdapterOS on ${new Date(metadata.exportTimestamp).toLocaleString()}*\n`;

  return md;
}
