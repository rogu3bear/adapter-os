// DatasetOverview - Overview tab for dataset detail page

import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { LoadingState } from '@/components/ui/loading-state';
import { apiClient } from '@/api/services';
import type { Dataset, DatasetVersionSummary } from '@/api/training-types';
import { formatBytes, formatTimestamp, formatNumber } from '@/lib/formatters';

interface DatasetOverviewProps {
  dataset: Dataset;
  isLoading: boolean;
  versions?: DatasetVersionSummary[];
  isLoadingVersions?: boolean;
  latestVersionId?: string;
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

export default function DatasetOverview({
  dataset,
  isLoading,
  versions = [],
  isLoadingVersions = false,
  latestVersionId,
}: DatasetOverviewProps) {
  const { data: statistics, isLoading: isLoadingStats } = useQuery({
    queryKey: ['dataset', dataset.id, 'statistics'],
    queryFn: async (): Promise<DatasetStatistics> => {
      return apiClient.request<DatasetStatistics>(`/v1/datasets/${dataset.id}/statistics`);
    },
    enabled: !!dataset.id,
    retry: false, // Don't retry if stats don't exist
  });

  if (isLoading) {
    return <LoadingState message="Loading dataset..." />;
  }

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
              <Label className="text-muted-foreground">Version</Label>
              <p className="font-mono text-sm">{dataset.dataset_version_id || '-'}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Source Type</Label>
              <Badge variant="outline" className="mt-1">
                {sourceTypeLabel}
              </Badge>
            </div>
            <div>
              <Label className="text-muted-foreground">Trust</Label>
              <Badge variant="outline" className="mt-1">
                {dataset.trust_state ?? 'unknown'}
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
              <p className="text-sm">{formatTimestamp(dataset.created_at, 'long')}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Updated At</Label>
              <p className="text-sm">{formatTimestamp(dataset.updated_at, 'long')}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Versions */}
      <Card>
        <CardHeader>
          <CardTitle>Dataset Versions</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoadingVersions ? (
            <LoadingState message="Loading versions..." />
          ) : versions.length > 0 ? (
            <div className="space-y-2">
              {versions.map((v) => (
                <div key={v.dataset_version_id} className="flex items-start justify-between rounded border p-3">
                  <div className="space-y-1">
                    <p className="font-mono text-xs break-all">{v.dataset_version_id}</p>
                    <p className="text-xs text-muted-foreground">
                      v{v.version_number}
                      {v.version_label ? ` • ${v.version_label}` : ''} • {formatTimestamp(v.created_at, 'short')}
                    </p>
                  </div>
                  <Badge variant="outline">{v.trust_state ?? 'unknown'}</Badge>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No versions available yet.</p>
          )}
          {latestVersionId && (
            <p className="mt-3 text-xs text-muted-foreground">
              Latest trusted version used for training: <span className="font-mono">{latestVersionId}</span>
            </p>
          )}
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
              <p className="text-2xl font-bold">{formatNumber(dataset.file_count || 0)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Total Size</Label>
              <p className="text-2xl font-bold">{formatBytes(dataset.total_size_bytes || 0)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Total Tokens</Label>
              <p className="text-2xl font-bold">{formatNumber(dataset.total_tokens || 0)}</p>
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
                  <p className="text-sm">{formatTimestamp(statistics.computed_at, 'long')}</p>
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

