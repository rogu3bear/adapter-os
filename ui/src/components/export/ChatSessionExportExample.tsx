/**
 * Example integration of ExportActionButton in a chat session interface
 *
 * This file demonstrates how to integrate export functionality into
 * an existing chat component. Use this as a reference for adding
 * export capabilities to chat sessions, document libraries, or other
 * content types.
 *
 * @example
 * ```tsx
 * // In your chat component:
 * import { useChatExport } from '@/components/export/ChatSessionExportExample';
 *
 * function ChatInterface({ session, messages }) {
 *   const { ExportButton } = useChatExport(session, messages);
 *
 *   return (
 *     <div>
 *       <header>
 *         <h1>{session.name}</h1>
 *         <ExportButton />
 *       </header>
 *       {messages.map(msg => <Message key={msg.id} {...msg} />)}
 *     </div>
 *   );
 * }
 * ```
 */

import { ExportActionButton } from './ExportActionButton';
import {
  renderChatSessionMarkdown,
  downloadTextFile,
  generateExportFilename,
  generateChatSessionPdf,
  downloadPdfFile,
  type ExportMetadata,
} from '@/utils/export';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import type { ChatSession } from '@/types/chat';

interface ChatExportOptions {
  includeMetadata?: boolean;
  includeEvidence?: boolean;
  includeRouterDecisions?: boolean;
}

/**
 * Hook for adding export functionality to chat sessions
 *
 * @param session - Chat session object
 * @param messages - Array of chat messages
 * @param options - Export options
 * @returns Export handlers and button component
 */
export function useChatExport(
  session: ChatSession,
  messages: ChatMessage[],
  options: ChatExportOptions = {}
) {
  const {
    includeMetadata = true,
    includeEvidence = true,
    includeRouterDecisions = true,
  } = options;

  const createExportMetadata = (): ExportMetadata => ({
    exportId: crypto.randomUUID(),
    exportTimestamp: new Date().toISOString(),
    entityType: 'chat_session',
    entityId: session.id,
    entityName: session.name,
  });

  const handleExportMarkdown = async () => {
    const metadata = createExportMetadata();

    // Filter messages based on options
    const messagesToExport = messages.map(msg => ({
      ...msg,
      evidence: includeEvidence ? msg.evidence : undefined,
      routerDecision: includeRouterDecisions ? msg.routerDecision : undefined,
    }));

    const markdown = renderChatSessionMarkdown(
      session.name,
      messagesToExport,
      metadata
    );

    const filename = generateExportFilename(session.name, 'md');
    downloadTextFile(markdown, filename, 'text/markdown');
  };

  const handleExportJson = async () => {
    const metadata = createExportMetadata();

    const exportData = {
      metadata: includeMetadata ? metadata : undefined,
      session: {
        id: session.id,
        name: session.name,
        stack_id: session.stackId,
        stack_name: session.stackName,
        collection_id: session.collectionId,
        created_at: session.createdAt.toISOString(),
        updated_at: session.updatedAt.toISOString(),
        tenant_id: session.tenantId,
      },
      messages: messages.map(msg => ({
        id: msg.id,
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp.toISOString(),
        request_id: msg.requestId,
        evidence: includeEvidence ? msg.evidence : undefined,
        router_decision: includeRouterDecisions ? msg.routerDecision : undefined,
        is_verified: msg.isVerified,
        verified_at: msg.verifiedAt,
      })),
      exported_at: metadata.exportTimestamp,
    };

    const json = JSON.stringify(exportData, null, 2);
    const filename = generateExportFilename(session.name, 'json');
    downloadTextFile(json, filename, 'application/json');
  };

  const handleExportPdf = async () => {
    const metadata = createExportMetadata();

    // Filter messages based on options
    const messagesToExport = messages.map(msg => ({
      ...msg,
      evidence: includeEvidence ? msg.evidence : undefined,
      routerDecision: includeRouterDecisions ? msg.routerDecision : undefined,
    }));

    const pdfBlob = await generateChatSessionPdf(
      session.name,
      messagesToExport,
      metadata
    );

    const filename = generateExportFilename(session.name, 'pdf');
    downloadPdfFile(pdfBlob, filename);
  };

  const ExportButton = () => (
    <ExportActionButton
      onExportMarkdown={handleExportMarkdown}
      onExportJson={handleExportJson}
      onExportPdf={handleExportPdf}
      disabled={messages.length === 0}
    />
  );

  return {
    handleExportMarkdown,
    handleExportJson,
    handleExportPdf,
    ExportButton,
  };
}

/**
 * Example chat header component with export functionality
 */
export function ChatSessionHeaderWithExport({
  session,
  messages,
  onClose,
}: {
  session: ChatSession;
  messages: ChatMessage[];
  onClose?: () => void;
}) {
  const { ExportButton } = useChatExport(session, messages);

  return (
    <div className="flex items-center justify-between p-4 border-b">
      <div className="flex-1">
        <h2 className="text-lg font-semibold">{session.name}</h2>
        <p className="text-sm text-muted-foreground">
          {messages.length} message{messages.length !== 1 ? 's' : ''}
          {session.stackName && ` • Stack: ${session.stackName}`}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <ExportButton />
        {onClose && (
          <button
            onClick={onClose}
            className="px-3 py-1 text-sm hover:bg-accent rounded"
          >
            Close
          </button>
        )}
      </div>
    </div>
  );
}
