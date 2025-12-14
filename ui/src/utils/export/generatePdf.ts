import jsPDF from 'jspdf';
import autoTable from 'jspdf-autotable';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import type { ExportMetadata } from './renderMarkdown';

/**
 * Generates a PDF document for a chat session
 *
 * @param sessionName - Name of the chat session
 * @param messages - Array of chat messages
 * @param metadata - Export metadata for tracking
 * @returns Blob containing the PDF document
 */
export async function generateChatSessionPdf(
  sessionName: string,
  messages: ChatMessage[],
  metadata: ExportMetadata
): Promise<Blob> {
  const doc = new jsPDF();

  // Title
  doc.setFontSize(20);
  doc.text(`Chat Session: ${sessionName}`, 14, 22);

  // Metadata table
  doc.setFontSize(12);
  autoTable(doc, {
    startY: 30,
    head: [['Field', 'Value']],
    body: [
      ['Export Date', metadata.exportTimestamp],
      ['Session ID', metadata.entityId],
      ['Export ID', metadata.exportId],
    ],
    theme: 'striped',
  });

  // Messages table
  const finalY = (doc as any).lastAutoTable?.finalY || 60;
  autoTable(doc, {
    startY: finalY + 10,
    head: [['Time', 'Role', 'Message']],
    body: messages.map(msg => [
      msg.timestamp ? new Date(msg.timestamp).toLocaleString() : '',
      msg.role,
      msg.content.slice(0, 200) + (msg.content.length > 200 ? '...' : ''),
    ]),
    theme: 'striped',
    styles: { cellWidth: 'wrap', fontSize: 9 },
    columnStyles: {
      0: { cellWidth: 35 },
      1: { cellWidth: 20 },
      2: { cellWidth: 'auto' },
    },
  });

  return doc.output('blob');
}

/**
 * Downloads a PDF file by creating a temporary link and triggering download
 *
 * @param blob - Blob containing the PDF data
 * @param filename - Name of the file to download
 */
export function downloadPdfFile(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename.endsWith('.pdf') ? filename : `${filename}.pdf`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}
