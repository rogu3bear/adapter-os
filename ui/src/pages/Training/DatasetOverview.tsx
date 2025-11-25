// DatasetOverview - Overview tab for dataset detail page

import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { LoadingState } from '@/components/ui/loading-state';
import apiClient from '@/api/client';
import type { Dataset } from '@/api/training-types';

interface DatasetOverviewProps {
  dataset: Dataset;
  isLoading: boolean;
}

interface DatasetStatistics {
  schema_version: string;
  dataset_id: string;
  num_examples: number;
  avg_input_length: number;
  avg_target_length: number;
  language_distribution?: Record<string, number>;
  file_type_distribution?: Record<string, number>;
  total_tokens: number;
  computed_at: string;
}

const SOURCE_TYPE_LABELS: Record<string, string> = {
  uploaded_files: 'Uploaded',
  code_repo: 'Code Repository',
  generated: 'Generated',
};

export default function DatasetOverview({ dataset, isLoading }: DatasetOverviewProps) {
  const { data: statistics, isLoading: isLoadingStats } = useQuery<DatasetStatistics>({
    queryKey: ['dataset', dataset.id, 'statistics'],
    queryFn: async () => {
      return apiClient.request<DatasetStatistics>(`/v1/datasets/${dataset.id}/statistics`);
    },
    enabled: !!dataset.id,
    retry: false, // Don't retry if stats don't exist
  });

  if (isLoading) {
    return <LoadingState message="Loading dataset..." />;
  }

  const formatDate = (dateString?: string): string => {
    if (!dateString) return '-';
    try {
      return new Date(dateString).toLocaleString();
    } catch {
      return dateString;
    }
  };

  const formatNumber = (num?: number): string => {
    if (num === undefined || num === null) return '-';
    return num.toLocaleString();
  };

  const formatBytes = (bytes?: number): string => {
    if (bytes === undefined || bytes === null) return '-';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  };

  const sourceTypeLabel = SOURCE_TYPE_LABELS[dataset.source_type] || dataset.source_type;

  return (
    <div className="space-y-6">
      {/* Basic Information */}
      <Card>
        <CardHeader>
          <CardTitle>Basic Information</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label className="text-muted-foreground">Name</Label>
              <p className="font-medium">{dataset.name}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">ID</Label>
              <p className="font-mono text-sm">{dataset.id}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Source Type</Label>
              <Badge variant="outline" className="mt-1">
                {sourceTypeLabel}
              </Badge>
            </div>
            <div>
              <Label className="text-muted-foreground">Language</Label>
              <p className="font-medium">{dataset.language || '-'}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Framework</Label>
              <p className="font-medium">{dataset.framework || '-'}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Created At</Label>
              <p className="text-sm">{formatDate(dataset.created_at)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Updated At</Label>
              <p className="text-sm">{formatDate(dataset.updated_at)}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Statistics */}
      <Card>
        <CardHeader>
          <CardTitle>Statistics</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-3 gap-4">
            <div>
              <Label className="text-muted-foreground">File Count</Label>
              <p className="text-2xl font-bold">{formatNumber(dataset.file_count)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Total Size</Label>
              <p className="text-2xl font-bold">{formatBytes(dataset.total_size_bytes)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Total Tokens</Label>
              <p className="text-2xl font-bold">{formatNumber(dataset.total_tokens)}</p>
            </div>
          </div>
          {statistics && (
            <div className="mt-6 pt-6 border-t">
              <h4 className="font-semibold mb-4">Computed Statistics</h4>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label className="text-muted-foreground">Number of Examples</Label>
                  <p className="text-lg font-medium">{formatNumber(statistics.num_examples)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Avg Input Length</Label>
                  <p className="text-lg font-medium">{statistics.avg_input_length.toFixed(1)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Avg Target Length</Label>
                  <p className="text-lg font-medium">{statistics.avg_target_length.toFixed(1)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Computed At</Label>
                  <p className="text-sm">{formatDate(statistics.computed_at)}</p>
                </div>
              </div>
              {statistics.language_distribution && Object.keys(statistics.language_distribution).length > 0 && (
                <div className="mt-4">
                  <Label className="text-muted-foreground">Language Distribution</Label>
                  <div className="flex flex-wrap gap-2 mt-2">
                    {Object.entries(statistics.language_distribution).map(([lang, count]) => (
                      <Badge key={lang} variant="outline">
                        {lang}: {count}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}
              {statistics.file_type_distribution && Object.keys(statistics.file_type_distribution).length > 0 && (
                <div className="mt-4">
                  <Label className="text-muted-foreground">File Type Distribution</Label>
                  <div className="flex flex-wrap gap-2 mt-2">
                    {Object.entries(statistics.file_type_distribution).map(([type, count]) => (
                      <Badge key={type} variant="outline">
                        {type}: {count}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
          {!statistics && !isLoadingStats && (
            <div className="mt-4 text-sm text-muted-foreground">
              Statistics not yet computed for this dataset
            </div>
          )}
        </CardContent>
      </Card>

      {/* Hash */}
      <Card>
        <CardHeader>
          <CardTitle>Hash (BLAKE3)</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono text-xs break-all">{dataset.hash_b3}</p>
        </CardContent>
      </Card>
    </div>
  );
}

