// DatasetPreview - Preview tab for dataset detail page

import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { LoadingState } from '@/components/ui/loading-state';
import { apiClient } from '@/api/services';

interface DatasetPreviewProps {
  datasetId: string;
  isLoading: boolean;
}

interface PreviewResponse {
  dataset_id: string;
  format: string;
  total_examples: number;
  examples: Array<Record<string, unknown>>;
}

export default function DatasetPreview({ datasetId, isLoading }: DatasetPreviewProps) {
  const { data: preview, isLoading: isLoadingPreview } = useQuery({
    queryKey: ['dataset', datasetId, 'preview'],
    queryFn: async (): Promise<PreviewResponse> => {
      return apiClient.request<PreviewResponse>(`/v1/datasets/${datasetId}/preview?limit=20`);
    },
    enabled: !!datasetId,
  });

  if (isLoading || isLoadingPreview) {
    return <LoadingState message="Loading preview..." />;
  }

  if (!preview || preview.examples.length === 0) {
    return (
      <Card>
        <CardContent className="pt-6">
          <p className="text-center text-muted-foreground">No preview available for this dataset</p>
        </CardContent>
      </Card>
    );
  }

  const formatExample = (example: Record<string, unknown>, format: string): React.ReactNode => {
    // Format-specific rendering
    if (format === 'jsonl' || format === 'json') {
      return (
        <pre className="text-xs bg-muted p-3 rounded overflow-auto max-h-48 font-mono">
          {JSON.stringify(example, null, 2)}
        </pre>
      );
    }
    
    // For text/patches format, try to extract text content
    if (format === 'txt' || format === 'patches') {
      const textContent = typeof example === 'string' 
        ? example 
        : (example.text || example.content || example.patch || JSON.stringify(example, null, 2));
      return (
        <pre className="text-xs bg-muted p-3 rounded overflow-auto max-h-48 whitespace-pre-wrap">
          {String(textContent)}
        </pre>
      );
    }
    
    // Default: JSON formatting
    return (
      <pre className="text-xs bg-muted p-3 rounded overflow-auto max-h-48 font-mono">
        {JSON.stringify(example, null, 2)}
      </pre>
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Preview ({preview.total_examples} examples)</CardTitle>
        <p className="text-sm text-muted-foreground mt-1">
          Format: {preview.format || 'unknown'}
        </p>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {preview.examples.slice(0, 20).map((example, idx) => (
            <div key={idx} className="border rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <span className="text-sm font-medium text-muted-foreground">Example {idx + 1}</span>
                {preview.format && (
                  <span className="text-xs px-2 py-0.5 bg-muted rounded">
                    {preview.format}
                  </span>
                )}
              </div>
              {formatExample(example, preview.format || 'json')}
            </div>
          ))}
          {preview.examples.length > 20 && (
            <p className="text-sm text-muted-foreground text-center">
              Showing first 20 of {preview.total_examples} examples
            </p>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

