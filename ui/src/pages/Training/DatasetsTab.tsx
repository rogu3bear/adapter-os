import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useTraining } from '@/hooks/useTraining';
import { useRBAC } from '@/hooks/useRBAC';
import { PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { withErrorBoundary } from '@/components/withErrorBoundary';
import { TrainingWizard } from '@/components/TrainingWizard';
import {
  Database,
  Upload,
  RefreshCw,
  CheckCircle,
  XCircle,
  Clock,
  FileCode,
  Trash2,
  Eye,
  AlertCircle,
  Play,
} from 'lucide-react';
import type { Dataset, DatasetSourceType, DatasetValidationStatus } from '@/api/training-types';

const STATUS_CONFIG: Record<DatasetValidationStatus, {
  icon: React.ElementType;
  className: string;
  label: string;
}> = {
  draft: {
    icon: Clock,
    className: 'text-yellow-500',
    label: 'Draft',
  },
  validating: {
    icon: RefreshCw,
    className: 'text-blue-500 animate-spin',
    label: 'Validating',
  },
  valid: {
    icon: CheckCircle,
    className: 'text-green-500',
    label: 'Valid',
  },
  invalid: {
    icon: XCircle,
    className: 'text-red-500',
    label: 'Invalid',
  },
  failed: {
    icon: AlertCircle,
    className: 'text-red-500',
    label: 'Failed',
  },
};

function StatusBadge({ status }: { status: DatasetValidationStatus }) {
  const config = STATUS_CONFIG[status] || STATUS_CONFIG.draft;
  const Icon = config.icon;

  return (
    <Badge variant="outline" className="gap-1">
      <Icon className={`h-3 w-3 ${config.className}`} />
      <span>{config.label}</span>
    </Badge>
  );
}

export function DatasetsTab() {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const navigate = useNavigate();

  const [isUploadDialogOpen, setIsUploadDialogOpen] = useState(false);
  const [deleteDatasetId, setDeleteDatasetId] = useState<string | null>(null);
  const [isTrainingWizardOpen, setIsTrainingWizardOpen] = useState(false);
  const [initialDatasetId, setInitialDatasetId] = useState<string | undefined>(undefined);

  const {
    data: datasetsData,
    isLoading,
    error,
    refetch,
  } = useTraining.useDatasets();

  // Handle errors outside of query options (React Query v5 compatibility)
  if (error) {
    addError('fetch-datasets', error.message, () => refetch());
  }

  const { mutateAsync: deleteDataset, isPending: isDeleting } = useTraining.useDeleteDataset({
    onSuccess: () => {
      setDeleteDatasetId(null);
      refetch();
    },
    onError: (err) => {
      addError('delete-dataset', err.message);
    },
  });

  const { mutateAsync: validateDataset } = useTraining.useValidateDataset({
    onSuccess: () => {
      refetch();
    },
    onError: (err) => {
      addError('validate-dataset', err.message);
    },
  });

  const datasets = datasetsData?.datasets || [];

  const handleDeleteDataset = useCallback(async () => {
    if (!deleteDatasetId) return;
    clearError('delete-dataset');
    try {
      await deleteDataset(deleteDatasetId);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to delete dataset');
      addError('delete-dataset', error.message);
    }
  }, [deleteDatasetId, deleteDataset, clearError, addError]);

  const handleValidateDataset = useCallback(async (datasetId: string) => {
    clearError('validate-dataset');
    try {
      await validateDataset(datasetId);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to validate dataset');
      addError('validate-dataset', error.message);
    }
  }, [validateDataset, clearError, addError]);

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

  return (
    <div className="space-y-6">
      {/* Action Bar */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          {can('dataset:upload') && (
            <Button onClick={() => setIsUploadDialogOpen(true)}>
              <Upload className="h-4 w-4 mr-2" />
              Upload Dataset
            </Button>
          )}
        </div>

        <Button
          variant="outline"
          size="sm"
          onClick={() => refetch()}
          disabled={isLoading}
        >
          <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      <PageErrors errors={errors} />

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <p className="text-destructive">Failed to load datasets: {error.message}</p>
            <Button variant="outline" onClick={() => refetch()} className="mt-2">
              Retry
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Datasets Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-5 w-5" />
            Datasets
            {datasets.length > 0 && (
              <span className="text-sm font-normal text-muted-foreground">
                ({datasets.length} total)
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading && datasets.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              <RefreshCw className="h-6 w-6 animate-spin mx-auto mb-2" />
              Loading datasets...
            </div>
          ) : datasets.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              <Database className="h-8 w-8 mx-auto mb-2 opacity-50" />
              <p>No datasets found</p>
              <p className="text-sm mt-1">Upload a dataset to get started</p>
            </div>
          ) : (
            <div className="max-h-[600px] overflow-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Source Type</TableHead>
                    <TableHead>Language</TableHead>
                    <TableHead>Files</TableHead>
                    <TableHead>Tokens</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Created</TableHead>
                    <TableHead>Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {datasets.map((dataset) => (
                    <TableRow key={dataset.id}>
                      <TableCell className="font-medium">
                        <div className="flex flex-col">
                          <span className="truncate max-w-[200px]" title={dataset.name}>
                            {dataset.name}
                          </span>
                          <span className="text-xs text-muted-foreground truncate max-w-[200px]">
                            {dataset.id.slice(0, 8)}...
                          </span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">
                          {dataset.source_type === 'uploaded_files' ? 'Uploaded' :
                           dataset.source_type === 'code_repo' ? 'Code Repository' :
                           dataset.source_type === 'generated' ? 'Generated' :
                           dataset.source_type}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-muted-foreground">
                        {dataset.language || '-'}
                      </TableCell>
                      <TableCell className="text-muted-foreground">
                        {formatNumber(dataset.file_count)}
                      </TableCell>
                      <TableCell className="text-muted-foreground">
                        {formatNumber(dataset.total_tokens)}
                      </TableCell>
                      <TableCell>
                        <StatusBadge status={dataset.validation_status} />
                      </TableCell>
                      <TableCell className="text-muted-foreground text-sm">
                        {formatDate(dataset.created_at)}
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-1">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => navigate(`/training/datasets/${dataset.id}`)}
                            title="View dataset details"
                          >
                            <Eye className="h-4 w-4" />
                          </Button>

                          {dataset.validation_status === 'valid' && can('training:start') && (
                            <Button
                              size="sm"
                              variant="default"
                              onClick={() => {
                                setInitialDatasetId(dataset.id);
                                setIsTrainingWizardOpen(true);
                              }}
                              title="Start training with this dataset"
                            >
                              <Play className="h-4 w-4 mr-1" />
                              Train
                            </Button>
                          )}

                          {(dataset.validation_status === 'draft' || dataset.validation_status === 'invalid') && can('dataset:validate') && (
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => handleValidateDataset(dataset.id)}
                              title="Validate dataset"
                            >
                              <CheckCircle className="h-4 w-4" />
                            </Button>
                          )}

                          {can('dataset:delete') && (
                            <Button
                              size="sm"
                              variant="destructive"
                              onClick={() => setDeleteDatasetId(dataset.id)}
                              title="Delete dataset"
                            >
                              <Trash2 className="h-4 w-4" />
                            </Button>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Upload Dataset Dialog */}
      <Dialog open={isUploadDialogOpen} onOpenChange={setIsUploadDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Upload Dataset</DialogTitle>
          </DialogHeader>
          <UploadDatasetForm
            onSuccess={() => {
              setIsUploadDialogOpen(false);
              refetch();
            }}
            onCancel={() => setIsUploadDialogOpen(false)}
          />
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={!!deleteDatasetId} onOpenChange={() => setDeleteDatasetId(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Dataset</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this dataset? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteDataset}
              disabled={isDeleting}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isDeleting ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Dataset Detail Dialog */}
      <Dialog open={!!selectedDataset} onOpenChange={() => setSelectedDataset(null)}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Dataset Details</DialogTitle>
          </DialogHeader>
          {selectedDataset && (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label className="text-muted-foreground">Name</Label>
                  <p className="font-medium">{selectedDataset.name}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">ID</Label>
                  <p className="font-mono text-sm">{selectedDataset.id}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Source Type</Label>
                  <p className="font-medium capitalize">{selectedDataset.source_type}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Language</Label>
                  <p className="font-medium">{selectedDataset.language || '-'}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Framework</Label>
                  <p className="font-medium">{selectedDataset.framework || '-'}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Status</Label>
                  <StatusBadge status={selectedDataset.validation_status} />
                </div>
                <div>
                  <Label className="text-muted-foreground">Files</Label>
                  <p className="font-medium">{formatNumber(selectedDataset.file_count)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Total Tokens</Label>
                  <p className="font-medium">{formatNumber(selectedDataset.total_tokens)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Created At</Label>
                  <p className="text-sm">{formatDate(selectedDataset.created_at)}</p>
                </div>
                <div>
                  <Label className="text-muted-foreground">Updated At</Label>
                  <p className="text-sm">{formatDate(selectedDataset.updated_at)}</p>
                </div>
              </div>
              <div>
                <Label className="text-muted-foreground">Hash (BLAKE3)</Label>
                <p className="font-mono text-xs break-all">{selectedDataset.hash_b3}</p>
              </div>

      {/* Training Wizard Dialog */}
      <Dialog open={isTrainingWizardOpen} onOpenChange={setIsTrainingWizardOpen}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <TrainingWizard
            initialDatasetId={initialDatasetId}
            onComplete={(jobId) => {
              setIsTrainingWizardOpen(false);
              setInitialDatasetId(undefined);
              // Optionally navigate to training jobs page or show notification
            }}
            onCancel={() => {
              setIsTrainingWizardOpen(false);
              setInitialDatasetId(undefined);
            }}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}

function UploadDatasetForm({
  onSuccess,
  onCancel,
}: {
  onSuccess: () => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState('');
  const [sourceType, setSourceType] = useState<DatasetSourceType>('uploaded_files');
  const [language, setLanguage] = useState('');
  const [framework, setFramework] = useState('');
  const [files, setFiles] = useState<FileList | null>(null);

  const { mutateAsync: createDataset, isPending } = useTraining.useCreateDataset({
    onSuccess,
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    await createDataset({
      name,
      source_type: sourceType,
      language: language || undefined,
      framework: framework || undefined,
      files: files ? Array.from(files) : undefined,
    });
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div>
        <Label htmlFor="name">Dataset Name</Label>
        <Input
          id="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-dataset"
          required
        />
      </div>

      <div>
        <Label htmlFor="sourceType">Source Type</Label>
        <Select value={sourceType} onValueChange={(v) => setSourceType(v as DatasetSourceType)}>
          <SelectTrigger id="sourceType">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="uploaded_files">Uploaded Files</SelectItem>
            <SelectItem value="code_repo">Code Repository</SelectItem>
            <SelectItem value="generated">Generated</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div>
        <Label htmlFor="language">Language (optional)</Label>
        <Input
          id="language"
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          placeholder="python"
        />
      </div>

      <div>
        <Label htmlFor="framework">Framework (optional)</Label>
        <Input
          id="framework"
          value={framework}
          onChange={(e) => setFramework(e.target.value)}
          placeholder="pytorch"
        />
      </div>

      {sourceType === 'uploaded_files' && (
        <div>
          <Label htmlFor="files">Files</Label>
          <Input
            id="files"
            type="file"
            multiple
            onChange={(e) => setFiles(e.target.files)}
            accept=".py,.js,.ts,.tsx,.jsx,.json,.txt"
          />
        </div>
      )}

      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? 'Creating...' : 'Create Dataset'}
        </Button>
      </div>
    </form>
  );
}

export default withErrorBoundary(DatasetsTab, 'Failed to load datasets tab');
