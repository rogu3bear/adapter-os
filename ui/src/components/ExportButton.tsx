import React from 'react';
import { Button } from './ui/button';
import { Download, Loader2 } from 'lucide-react';
import { useAsyncAction } from '@/hooks/async/useAsyncAction';

interface ExportButtonProps {
  data?: Record<string, unknown>[];
  format: 'csv' | 'json';
  filename?: string;
  className?: string;
  // API-based export: function that returns a Blob
  onExport?: (format: 'csv' | 'json') => Promise<Blob>;
  // For API exports, provide the mime type
  mimeType?: string;
}

export const ExportButton: React.FC<ExportButtonProps> = ({
  data,
  format,
  filename,
  className,
  onExport,
  mimeType,
}) => {
  const convertToCSV = (data: Record<string, unknown>[]): string => {
    if (data.length === 0) return '';

    const headers = Object.keys(data[0]);
    const csvHeaders = headers.join(',');

    const csvRows = data.map(row =>
      headers.map(header => {
        const value = row[header];
        // Escape commas and quotes in CSV
        if (typeof value === 'string' && (value.includes(',') || value.includes('"'))) {
          return `"${value.replace(/"/g, '""')}"`;
        }
        return value;
      }).join(',')
    );

    return [csvHeaders, ...csvRows].join('\n');
  };

  const downloadBlob = (blob: Blob, filename: string) => {
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  const downloadFile = (content: string, filename: string, mimeType: string) => {
    const blob = new Blob([content], { type: mimeType });
    downloadBlob(blob, filename);
  };

  const { execute: handleExport, isLoading } = useAsyncAction(
    async () => {
      const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
      const baseFilename = filename || `export-${timestamp}`;

      if (onExport) {
        // API-based export
        const blob = await onExport(format);
        const extension = format === 'csv' ? 'csv' : 'json';
        downloadBlob(blob, `${baseFilename}.${extension}`);
      } else if (data) {
        // Local data export
        if (format === 'csv') {
          const csv = convertToCSV(data);
          downloadFile(csv, `${baseFilename}.csv`, 'text/csv');
        } else {
          const json = JSON.stringify(data, null, 2);
          downloadFile(json, `${baseFilename}.json`, 'application/json');
        }
      }
    },
    { operationName: 'export_data' }
  );

  return (
    <Button
      onClick={() => handleExport()}
      variant="outline"
      size="sm"
      className={className}
      disabled={isLoading || (!data && !onExport)}
    >
      {isLoading ? (
        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
      ) : (
        <Download className="h-4 w-4 mr-2" />
      )}
      {isLoading ? 'Exporting...' : `Export ${format.toUpperCase()}`}
    </Button>
  );
};

// Specialized export buttons for common data types
export const ExportRoutingDecisions: React.FC<{ data: Record<string, unknown>[]; className?: string }> = ({
  data,
  className,
}) => {
  return (
    <div className="flex gap-2">
      <ExportButton
        data={data}
        format="csv"
        filename="routing-decisions"
        className={className}
      />
      <ExportButton
        data={data}
        format="json"
        filename="routing-decisions"
        className={className}
      />
    </div>
  );
};

export const ExportMetrics: React.FC<{ data: Record<string, unknown>[]; className?: string }> = ({
  data,
  className,
}) => {
  return (
    <div className="flex gap-2">
      <ExportButton
        data={data}
        format="csv"
        filename="metrics"
        className={className}
      />
      <ExportButton
        data={data}
        format="json"
        filename="metrics"
        className={className}
      />
    </div>
  );
};

