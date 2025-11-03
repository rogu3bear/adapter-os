//! Export Menu Component
//!
//! Provides a dropdown menu for exporting data with format options and filters.
//!
//! Citations:
//! - ui/src/components/ExportButton.tsx - Export button patterns
//! - ui/src/components/AuditDashboard.tsx L224-L264 - API export patterns

import React, { useState } from 'react';
import { Button } from './button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from './dropdown-menu';
import { Download, FileText, FileJson, Loader2 } from 'lucide-react';
import { Progress } from './progress';

export type ExportFormat = 'csv' | 'json';

export interface ExportMenuProps {
  onExport: (format: ExportFormat) => Promise<void>;
  filename?: string;
  disabled?: boolean;
  isLoading?: boolean;
  formats?: ExportFormat[];
}

export function ExportMenu({
  onExport,
  filename,
  disabled = false,
  isLoading = false,
  formats = ['csv', 'json'],
}: ExportMenuProps) {
  const [exporting, setExporting] = useState<ExportFormat | null>(null);

  const handleExport = async (format: ExportFormat) => {
    if (disabled || isLoading) return;
    
    setExporting(format);
    try {
      await onExport(format);
    } catch (error) {
      // Error handling should be done by parent component
    } finally {
      setExporting(null);
    }
  };

  const getFileIcon = (format: ExportFormat) => {
    return format === 'csv' ? <FileText className="h-4 w-4" /> : <FileJson className="h-4 w-4" />;
  };

  const getFormatLabel = (format: ExportFormat) => {
    return format.toUpperCase();
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          disabled={disabled || isLoading}
          className="gap-2"
        >
          <Download className="h-4 w-4" />
          {isLoading || exporting ? 'Exporting...' : 'Export'}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel>Export Format</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {formats.map((format) => (
          <DropdownMenuItem
            key={format}
            onClick={() => handleExport(format)}
            disabled={disabled || isLoading || exporting !== null}
            className="flex items-center gap-2"
          >
            {exporting === format ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              getFileIcon(format)
            )}
            <span>Export as {getFormatLabel(format)}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export default ExportMenu;
