import React from 'react';
import { Button } from './ui/button';
import { Download } from 'lucide-react';

interface ExportButtonProps {
  data: any[];
  format: 'csv' | 'json';
  filename?: string;
  className?: string;
}

export const ExportButton: React.FC<ExportButtonProps> = ({
  data,
  format,
  filename,
  className,
}) => {
  const convertToCSV = (data: any[]): string => {
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

  const downloadFile = (content: string, filename: string, mimeType: string) => {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    
    URL.revokeObjectURL(url);
  };

  const handleExport = () => {
    const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
    const baseFilename = filename || `export-${timestamp}`;
    
    if (format === 'csv') {
      const csv = convertToCSV(data);
      downloadFile(csv, `${baseFilename}.csv`, 'text/csv');
    } else {
      const json = JSON.stringify(data, null, 2);
      downloadFile(json, `${baseFilename}.json`, 'application/json');
    }
  };

  return (
    <Button
      onClick={handleExport}
      variant="outline"
      size="sm"
      className={className}
    >
      <Download className="h-4 w-4 mr-2" />
      Export {format.toUpperCase()}
    </Button>
  );
};

// Specialized export buttons for common data types
export const ExportRoutingDecisions: React.FC<{ data: any[]; className?: string }> = ({
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

export const ExportMetrics: React.FC<{ data: any[]; className?: string }> = ({
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
