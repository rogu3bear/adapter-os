/**
 * Export utilities for chat sessions, datasets, and other entities
 *
 * This module provides utilities for exporting AdapterOS data in various formats:
 * - Markdown with citations for human-readable documentation
 * - JSON for machine-readable structured data
 * - PDF for professional document sharing
 *
 * @module export
 */

export {
  renderChatSessionMarkdown,
  renderExtendedChatSessionMarkdown,
  renderSingleAnswerMarkdown,
  downloadTextFile,
  generateExportFilename,
  type ExportMetadata,
} from './renderMarkdown';

export {
  generateChatSessionPdf,
  downloadPdfFile,
} from './generatePdf';

export type {
  ExtendedExportMetadata,
  ExtendedMessageExport,
  ExtendedEvidenceItem,
  RouterDecisionExport,
  EvidenceBundleExport,
  ExportFormat,
  ExportScope,
} from './types';

export {
  generateEvidenceBundle,
  downloadEvidenceBundle,
  type GenerateEvidenceBundleOptions,
} from './generateEvidenceBundle';
