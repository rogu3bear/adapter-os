// DatasetFiles - Files tab for dataset detail page

import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { LoadingState } from '@/components/ui/loading-state';
import apiClient from '@/api/client';

interface DatasetFilesProps {
  datasetId: string;
  isLoading: boolean;
}

interface DatasetFile {
  file_id: string;
  file_name: string;
  file_path: string;
  size_bytes: number;
  hash: string;
  mime_type?: string;
  created_at: string;
}

export default function DatasetFiles({ datasetId, isLoading }: DatasetFilesProps) {
  const { data: files, isLoading: isLoadingFiles } = useQuery<DatasetFile[]>({
    queryKey: ['dataset', datasetId, 'files'],
    queryFn: async () => {
      return apiClient.request<DatasetFile[]>(`/v1/datasets/${datasetId}/files`);
    },
    enabled: !!datasetId,
  });

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  };

  const formatDate = (dateString: string): string => {
    try {
      return new Date(dateString).toLocaleString();
    } catch {
      return dateString;
    }
  };

  if (isLoading || isLoadingFiles) {
    return <LoadingState message="Loading files..." />;
  }

  if (!files || files.length === 0) {
    return (
      <Card>
        <CardContent className="pt-6">
          <p className="text-center text-muted-foreground">No files found in this dataset</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Files ({files.length})</CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>File Name</TableHead>
              <TableHead>Size</TableHead>
              <TableHead>Hash</TableHead>
              <TableHead>MIME Type</TableHead>
              <TableHead>Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {files.map((file) => (
              <TableRow key={file.file_id}>
                <TableCell className="font-medium">{file.file_name}</TableCell>
                <TableCell>{formatBytes(file.size_bytes)}</TableCell>
                <TableCell className="font-mono text-xs">{file.hash.slice(0, 16)}...</TableCell>
                <TableCell>{file.mime_type || '-'}</TableCell>
                <TableCell className="text-sm">{formatDate(file.created_at)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

