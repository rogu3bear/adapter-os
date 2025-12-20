import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { apiClient } from '@/api/services';
import { Dataset, DatasetValidationStatus, DatasetSourceType } from '@/api/training-types';
import { usePolling } from '@/hooks/realtime/usePolling';
import { Upload, Database, RefreshCw, CheckCircle, XCircle, Clock, AlertTriangle, FileText } from 'lucide-react';
import { logger } from '@/utils/logger';

export default function DataScientistDatasetManager() {
  const [isUploadOpen, setIsUploadOpen] = useState(false);
  const [newDatasetName, setNewDatasetName] = useState('');
  const [newDatasetSource, setNewDatasetSource] = useState<DatasetSourceType>('uploaded_files');
  const [selectedFiles, setSelectedFiles] = useState<FileList | null>(null);
  const [isUploading, setIsUploading] = useState(false);

  // Poll for datasets
  const { data: datasets, isLoading, error, refetch } = usePolling<Dataset[]>(
    async () => {
      // API client doesn't have listDatasets yet, mock for now
      // In production: return apiClient.listDatasets();
      return Promise.resolve([
        {
          id: 'ds-001',
          name: 'Python Code Samples',
          hash_b3: 'abc123def456',
          source_type: 'code_repo' as DatasetSourceType,
          language: 'python',
          framework: 'pytorch',
          file_count: 1250,
          total_size_bytes: 52428800, // 50MB
          total_tokens: 2500000,
          validation_status: 'valid' as DatasetValidationStatus,
          created_at: '2025-01-15T10:30:00Z',
          updated_at: '2025-01-15T10:45:00Z',
        },
        {
          id: 'ds-002',
          name: 'TypeScript Components',
          hash_b3: 'def789ghi012',
          source_type: 'uploaded_files' as DatasetSourceType,
          language: 'typescript',
          framework: 'react',
          file_count: 340,
          total_size_bytes: 18874368, // 18MB
          total_tokens: 890000,
          validation_status: 'valid' as DatasetValidationStatus,
          created_at: '2025-01-14T08:00:00Z',
          updated_at: '2025-01-14T09:15:00Z',
        },
        {
          id: 'ds-003',
          name: 'Rust Systems Code',
          hash_b3: 'ghi345jkl678',
          source_type: 'code_repo' as DatasetSourceType,
          language: 'rust',
          file_count: 89,
          total_size_bytes: 9437184, // 9MB
          total_tokens: 450000,
          validation_status: 'draft' as DatasetValidationStatus,
          created_at: '2025-01-16T14:00:00Z',
          updated_at: '2025-01-16T14:00:00Z',
        },
        {
          id: 'ds-004',
          name: 'Generated Samples',
          hash_b3: 'jkl901mno234',
          source_type: 'generated' as DatasetSourceType,
          file_count: 500,
          total_size_bytes: 26214400, // 25MB
          total_tokens: 1200000,
          validation_status: 'invalid' as DatasetValidationStatus,
          created_at: '2025-01-13T16:30:00Z',
          updated_at: '2025-01-13T17:00:00Z',
        },
      ]);
    },
    'slow',
    {
      onError: (err) => {
        logger.error('Failed to fetch datasets', { component: 'DataScientistDatasetManager' }, err);
      },
    }
  );

  const getValidationStatusBadge = (status: DatasetValidationStatus | string) => {
    switch (status) {
      case 'valid':
        return (
          <Badge variant="outline" className="bg-green-50 text-green-700 border-green-200">
            <CheckCircle className="h-3 w-3 mr-1" />
            Valid
          </Badge>
        );
      case 'invalid':
        return (
          <Badge variant="outline" className="bg-red-50 text-red-700 border-red-200">
            <XCircle className="h-3 w-3 mr-1" />
            Invalid
          </Badge>
        );
      case 'draft':
        return (
          <Badge variant="outline" className="bg-gray-50 text-gray-700 border-gray-200">
            <Clock className="h-3 w-3 mr-1" />
            Draft
          </Badge>
        );
      case 'validating':
        return (
          <Badge variant="outline" className="bg-blue-50 text-blue-700 border-blue-200">
            <RefreshCw className="h-3 w-3 mr-1 animate-spin" />
            Validating
          </Badge>
        );
      case 'pending':
        return (
          <Badge variant="outline" className="bg-yellow-50 text-yellow-700 border-yellow-200">
            <Clock className="h-3 w-3 mr-1" />
            Pending
          </Badge>
        );
      case 'skipped':
        return (
          <Badge variant="outline" className="bg-gray-50 text-gray-700 border-gray-200">
            <XCircle className="h-3 w-3 mr-1" />
            Skipped
          </Badge>
        );
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const getSourceTypeBadge = (sourceType: DatasetSourceType) => {
    switch (sourceType) {
      case 'code_repo':
        return <Badge variant="secondary">Repository</Badge>;
      case 'uploaded_files':
        return <Badge variant="secondary">Uploaded</Badge>;
      case 'generated':
        return <Badge variant="secondary">Generated</Badge>;
      default:
        return <Badge variant="outline">{sourceType}</Badge>;
    }
  };

  const formatTokenCount = (tokens: number) => {
    if (tokens >= 1000000) {
      return `${(tokens / 1000000).toFixed(1)}M`;
    }
    if (tokens >= 1000) {
      return `${(tokens / 1000).toFixed(1)}K`;
    }
    return tokens.toString();
  };

  const handleUpload = async () => {
    if (!newDatasetName || !selectedFiles?.length) return;

    setIsUploading(true);
    try {
      // In production: await apiClient.createDataset({...});
      logger.info('Dataset upload started', {
        component: 'DataScientistDatasetManager',
        name: newDatasetName,
        fileCount: selectedFiles.length,
      });

      // Simulate upload delay
      await new Promise((resolve) => setTimeout(resolve, 1500));

      setIsUploadOpen(false);
      setNewDatasetName('');
      setSelectedFiles(null);
      refetch();
    } catch (err) {
      logger.error('Failed to upload dataset', { component: 'DataScientistDatasetManager' }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setIsUploading(false);
    }
  };

  if (isLoading && !datasets) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <RefreshCw className="h-8 w-8 mx-auto mb-4 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">Loading datasets...</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <AlertTriangle className="h-8 w-8 mx-auto mb-4 text-red-500" />
            <p className="text-sm text-red-600">Failed to load datasets</p>
            <Button variant="outline" size="sm" className="mt-4" onClick={() => refetch()}>
              Retry
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6 p-4">
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <Database className="h-5 w-5" />
              Dataset Manager
            </CardTitle>
            <p className="text-sm text-muted-foreground mt-1">
              Manage training datasets for adapter fine-tuning
            </p>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => refetch()}>
              <RefreshCw className="h-4 w-4 mr-2" />
              Refresh
            </Button>
            <Dialog open={isUploadOpen} onOpenChange={setIsUploadOpen}>
              <DialogTrigger asChild>
                <Button size="sm">
                  <Upload className="h-4 w-4 mr-2" />
                  Upload Dataset
                </Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Upload New Dataset</DialogTitle>
                </DialogHeader>
                <div className="space-y-4 pt-4">
                  <div className="space-y-2">
                    <Label htmlFor="dataset-name">Dataset Name</Label>
                    <Input
                      id="dataset-name"
                      placeholder="e.g., Python Training Data v2"
                      value={newDatasetName}
                      onChange={(e) => setNewDatasetName(e.target.value)}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="source-type">Source Type</Label>
                    <Select value={newDatasetSource} onValueChange={(v) => setNewDatasetSource(v as DatasetSourceType)}>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="uploaded_files">Uploaded Files</SelectItem>
                        <SelectItem value="code_repo">Code Repository</SelectItem>
                        <SelectItem value="generated">Generated</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="files">Files</Label>
                    <Input
                      id="files"
                      type="file"
                      multiple
                      onChange={(e) => setSelectedFiles(e.target.files)}
                    />
                    {selectedFiles && (
                      <p className="text-xs text-muted-foreground">
                        {selectedFiles.length} file(s) selected
                      </p>
                    )}
                  </div>
                  <Button
                    className="w-full"
                    onClick={handleUpload}
                    disabled={isUploading || !newDatasetName || !selectedFiles?.length}
                  >
                    {isUploading ? (
                      <>
                        <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
                        Uploading...
                      </>
                    ) : (
                      <>
                        <Upload className="h-4 w-4 mr-2" />
                        Upload
                      </>
                    )}
                  </Button>
                </div>
              </DialogContent>
            </Dialog>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Source</TableHead>
                <TableHead>Language</TableHead>
                <TableHead>Files</TableHead>
                <TableHead>Tokens</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {datasets?.map((dataset) => (
                <TableRow key={dataset.id}>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <FileText className="h-4 w-4 text-muted-foreground" />
                      <span className="font-medium">{dataset.name}</span>
                    </div>
                  </TableCell>
                  <TableCell>{getSourceTypeBadge(dataset.source_type)}</TableCell>
                  <TableCell>
                    {dataset.language ? (
                      <Badge variant="outline">{dataset.language}</Badge>
                    ) : (
                      <span className="text-muted-foreground">-</span>
                    )}
                  </TableCell>
                  <TableCell>{dataset.file_count.toLocaleString()}</TableCell>
                  <TableCell>{formatTokenCount(dataset.total_tokens)}</TableCell>
                  <TableCell>{getValidationStatusBadge(dataset.validation_status)}</TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(dataset.created_at).toLocaleDateString()}
                  </TableCell>
                </TableRow>
              ))}
              {(!datasets || datasets.length === 0) && (
                <TableRow>
                  <TableCell colSpan={7} className="text-center text-muted-foreground py-8">
                    No datasets found. Upload your first dataset to get started.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
