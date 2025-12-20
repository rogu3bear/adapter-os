/**
 * ExportDialog - Dialog for selecting export format and options
 *
 * Shows format selection (Markdown, JSON, PDF), export preview,
 * and determinism verification state.
 */

import { useState } from 'react';
import { Download, FileText, FileJson, FileType, Check, AlertCircle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';
import type { ExportFormat } from '@/utils/export/types';

interface ExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onExport: (format: ExportFormat) => Promise<void>;
  title?: string;
  /** Number of messages to be exported */
  messageCount?: number;
  /** Number of evidence citations */
  evidenceCount?: number;
  /** Number of traces */
  traceCount?: number;
  /** Determinism state of the session */
  determinismState?: 'verified' | 'unverified' | 'approximate';
  /** Available formats (default: markdown, json, pdf) */
  availableFormats?: ExportFormat[];
  /** Include evidence bundle option */
  includeEvidenceBundle?: boolean;
}

const formatLabels: Record<ExportFormat, { label: string; icon: typeof FileText; extension: string }> = {
  markdown: { label: 'Markdown', icon: FileText, extension: '.md' },
  json: { label: 'JSON', icon: FileJson, extension: '.json' },
  pdf: { label: 'PDF', icon: FileType, extension: '.pdf' },
  'evidence-bundle': { label: 'Evidence Bundle', icon: FileJson, extension: '.json' },
};

export function ExportDialog({
  open,
  onOpenChange,
  onExport,
  title = 'Export Chat Session',
  messageCount = 0,
  evidenceCount = 0,
  traceCount = 0,
  determinismState,
  availableFormats = ['markdown', 'json', 'pdf'],
  includeEvidenceBundle = false,
}: ExportDialogProps) {
  const [selectedFormat, setSelectedFormat] = useState<ExportFormat>('markdown');
  const [isExporting, setIsExporting] = useState(false);

  const formats = includeEvidenceBundle
    ? [...availableFormats, 'evidence-bundle' as ExportFormat]
    : availableFormats;

  const handleExport = async () => {
    setIsExporting(true);
    try {
      await onExport(selectedFormat);
      onOpenChange(false);
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" data-testid="export-dialog">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            Select a format and export your conversation
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* Format Selection */}
          <div className="space-y-3">
            <Label className="text-sm font-medium">Format</Label>
            <RadioGroup
              value={selectedFormat}
              onValueChange={(value) => setSelectedFormat(value as ExportFormat)}
              className="space-y-2"
            >
              {formats.map((format) => {
                const { label, icon: Icon, extension } = formatLabels[format];
                return (
                  <div
                    key={format}
                    className={cn(
                      'flex items-center space-x-3 rounded-md border p-3 cursor-pointer transition-colors',
                      selectedFormat === format
                        ? 'border-primary bg-primary/5'
                        : 'border-border hover:bg-muted/50'
                    )}
                    onClick={() => setSelectedFormat(format)}
                  >
                    <RadioGroupItem value={format} id={format} />
                    <Label
                      htmlFor={format}
                      className="flex items-center gap-2 cursor-pointer flex-1"
                    >
                      <Icon className="h-4 w-4 text-muted-foreground" />
                      <span>{label}</span>
                      <span className="text-xs text-muted-foreground">({extension})</span>
                    </Label>
                  </div>
                );
              })}
            </RadioGroup>
          </div>

          {/* Export Preview */}
          <div className="space-y-2">
            <Label className="text-sm font-medium">Includes</Label>
            <div className="rounded-md border p-3 space-y-1 text-sm">
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">Messages</span>
                <span>{messageCount}</span>
              </div>
              {evidenceCount > 0 && (
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">
                    Evidence citations{selectedFormat === 'markdown' ? ' (with bbox)' : ''}
                  </span>
                  <span>{evidenceCount}</span>
                </div>
              )}
              {traceCount > 0 && (
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Trace records</span>
                  <span>{traceCount}</span>
                </div>
              )}
            </div>
          </div>

          {/* Determinism State */}
          {determinismState && (
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Determinism:</span>
              <DeterminismBadge state={determinismState} />
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isExporting}
          >
            Cancel
          </Button>
          <Button
            onClick={handleExport}
            disabled={isExporting}
            data-testid="export-confirm"
          >
            {isExporting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Exporting...
              </>
            ) : (
              <>
                <Download className="h-4 w-4 mr-2" />
                Export Download
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DeterminismBadge({ state }: { state: 'verified' | 'unverified' | 'approximate' }) {
  const configs = {
    verified: {
      icon: Check,
      label: 'VERIFIED',
      className: 'bg-green-100 text-green-800 border-green-200',
    },
    unverified: {
      icon: AlertCircle,
      label: 'UNVERIFIED',
      className: 'bg-yellow-100 text-yellow-800 border-yellow-200',
    },
    approximate: {
      icon: AlertCircle,
      label: 'APPROXIMATE',
      className: 'bg-orange-100 text-orange-800 border-orange-200',
    },
  };

  const config = configs[state];
  const Icon = config.icon;

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium border',
        config.className
      )}
    >
      <Icon className="h-3 w-3" />
      {config.label}
    </span>
  );
}

export default ExportDialog;
