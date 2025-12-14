import { useState } from 'react';
import { Download, FileText, FileJson, FileType } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { toast } from 'sonner';

interface ExportActionButtonProps {
  onExportMarkdown: () => Promise<void>;
  onExportJson: () => Promise<void>;
  onExportPdf?: () => Promise<void>;
  disabled?: boolean;
  variant?: 'default' | 'outline' | 'ghost' | 'destructive' | 'secondary' | 'link' | null;
  size?: 'default' | 'sm' | 'lg' | 'icon' | null;
}

/**
 * Export action button with dropdown menu for different export formats
 *
 * Provides options to export content as:
 * - Markdown (.md) - Human-readable format with citations
 * - JSON (.json) - Machine-readable structured data
 * - PDF (.pdf) - Professional document format (optional)
 *
 * @example
 * ```tsx
 * <ExportActionButton
 *   onExportMarkdown={async () => {
 *     const markdown = renderChatSessionMarkdown(...);
 *     downloadTextFile(markdown, 'session.md', 'text/markdown');
 *   }}
 *   onExportJson={async () => {
 *     const json = JSON.stringify(data, null, 2);
 *     downloadTextFile(json, 'session.json', 'application/json');
 *   }}
 *   onExportPdf={async () => {
 *     const pdf = await generateChatSessionPdf(...);
 *     downloadPdfFile(pdf, 'session.pdf');
 *   }}
 * />
 * ```
 */
export function ExportActionButton({
  onExportMarkdown,
  onExportJson,
  onExportPdf,
  disabled,
  variant = 'outline',
  size = 'sm',
}: ExportActionButtonProps) {
  const [isExporting, setIsExporting] = useState(false);

  const handleExport = async (fn: () => Promise<void>, format: string) => {
    setIsExporting(true);
    try {
      await fn();
      toast.success(`Exported as ${format}`);
    } catch (error) {
      toast.error(`Export failed: ${(error as Error).message}`);
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant={variant}
          size={size}
          disabled={disabled || isExporting}
          data-testid="export-button"
        >
          <Download className="h-4 w-4 mr-2" />
          {isExporting ? 'Exporting...' : 'Export'}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem
          onClick={() => handleExport(onExportMarkdown, 'Markdown')}
          data-testid="export-markdown"
        >
          <FileText className="h-4 w-4 mr-2" />
          Export as Markdown
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => handleExport(onExportJson, 'JSON')}
          data-testid="export-json"
        >
          <FileJson className="h-4 w-4 mr-2" />
          Export as JSON
        </DropdownMenuItem>
        {onExportPdf && (
          <DropdownMenuItem
            onClick={() => handleExport(onExportPdf, 'PDF')}
            data-testid="export-pdf"
          >
            <FileType className="h-4 w-4 mr-2" />
            Export as PDF
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
