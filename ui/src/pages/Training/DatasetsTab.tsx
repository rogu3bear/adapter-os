import React, { useState, useCallback, useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
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
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { TERMS, formatSourceType, formatValidationStatus } from '@/constants/terminology';
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
import { formatTimestamp, formatNumber } from '@/utils/format';

const STATUS_CONFIG: Record<DatasetValidationStatus, {
  icon: React.ElementType;
  className: string;
  label: string;
}> = {
  draft: {
    icon: Clock,
    className: 'text-yellow-500',
    label: formatValidationStatus('draft'),
  },
  validating: {
    icon: RefreshCw,
    className: 'text-blue-500 animate-spin',
    label: formatValidationStatus('validating'),
  },
  valid: {
    icon: CheckCircle,
    className: 'text-green-500',
    label: formatValidationStatus('valid'),
  },
  invalid: {
    icon: XCircle,
    className: 'text-red-500',
    label: formatValidationStatus('invalid'),
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

type DatasetListItem = Dataset;

const ActionBar = ({
  canUpload,
  onOpenUpload,
  onRefresh,
  isLoading,
}: {
  canUpload: boolean;
  onOpenUpload: () => void;
  onRefresh: () => void;
  isLoading: boolean;
}) => (
  <div className="flex items-center justify-between">
    <div className="flex items-center gap-4">
      {canUpload && (
        <div className="flex items-center gap-2">
          <Button onClick={onOpenUpload}>
            <Upload className="mr-2 h-4 w-4" />
            {TERMS.uploadDataset}
          </Button>
          <GlossaryTooltip brief="For large or complex collections. Use Training Wizard for simple uploads." />
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        Tip: Train an adapter for consistent, repeatable responses. Use document collections for one-time lookups.
      </p>
    </div>
    <Button variant="outline" size="sm" onClick={onRefresh} disabled={isLoading}>
      <RefreshCw className={`mr-2 h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
      Refresh
    </Button>
  </div>
);

const DatasetTableRow = ({
  dataset,
  canStartTraining,
  canValidate,
  canDelete,
  onView,
  onStartTraining,
  onValidate,
  onDelete,
}: {
  dataset: DatasetListItem;
  canStartTraining: boolean;
  canValidate: boolean;
  canDelete: boolean;
  onView: (id: string) => void;
  onStartTraining: (id: string) => void;
  onValidate: (id: string) => void;
  onDelete: (id: string) => void;
}) => (
  <TableRow>
    <TableCell className="font-medium">
      <div className="flex flex-col">
        <span className="max-w-[calc(var(--base-unit)*50)] truncate" title={dataset.name}>
          {dataset.name}
        </span>
        <span className="max-w-[calc(var(--base-unit)*50)] truncate text-xs text-muted-foreground">{dataset.id.slice(0, 8)}...</span>
      </div>
    </TableCell>
    <TableCell>
      <Badge variant="outline">{formatSourceType(dataset.source_type)}</Badge>
    </TableCell>
    <TableCell className="text-muted-foreground">{dataset.language || '-'}</TableCell>
    <TableCell className="text-muted-foreground">{formatNumber(dataset.file_count || 0)}</TableCell>
    <TableCell className="text-muted-foreground">{formatNumber(dataset.total_tokens || 0)}</TableCell>
    <TableCell>
      <StatusBadge status={dataset.validation_status} />
    </TableCell>
    <TableCell className="text-sm text-muted-foreground">
      {formatTimestamp(dataset.created_at, 'long')}
    </TableCell>
    <TableCell>
      <div className="flex items-center gap-1">
        <Button size="sm" variant="outline" onClick={() => onView(dataset.id)} title="View collection details">
          <Eye className="h-4 w-4" />
        </Button>
        {dataset.validation_status === 'valid' && canStartTraining && (
          <Button
            size="sm"
            variant="default"
            onClick={() => onStartTraining(dataset.id)}
            title="Start training with this collection"
          >
            <Play className="mr-1 h-4 w-4" />
            Use in training job
          </Button>
        )}
        {(dataset.validation_status === 'draft' || dataset.validation_status === 'invalid') && canValidate && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => onValidate(dataset.id)}
            title="Validate collection"
          >
            <CheckCircle className="h-4 w-4" />
          </Button>
        )}
        {canDelete && (
          <Button size="sm" variant="destructive" onClick={() => onDelete(dataset.id)} title="Delete collection">
            <Trash2 className="h-4 w-4" />
          </Button>
        )}
      </div>
    </TableCell>
  </TableRow>
);

const DatasetsTable = ({
  datasets,
  canStartTraining,
  canValidate,
  canDelete,
  onView,
  onStartTraining,
  onValidate,
  onDelete,
}: {
  datasets: DatasetListItem[];
  canStartTraining: boolean;
  canValidate: boolean;
  canDelete: boolean;
  onView: (id: string) => void;
  onStartTraining: (id: string) => void;
  onValidate: (id: string) => void;
  onDelete: (id: string) => void;
}) => (
  <div className="max-h-[calc(var(--base-unit)*150)] overflow-auto">
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{TERMS.datasetName}</TableHead>
          <TableHead>Source Type</TableHead>
          <TableHead>Language</TableHead>
          <TableHead>{TERMS.documents}</TableHead>
          <TableHead>Tokens</TableHead>
          <TableHead>{TERMS.datasetStatus}</TableHead>
          <TableHead>Created</TableHead>
          <TableHead>Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {datasets.map((dataset) => (
          <DatasetTableRow
            key={dataset.id}
            dataset={dataset}
            canStartTraining={canStartTraining}
            canValidate={canValidate}
            canDelete={canDelete}
            onView={onView}
            onStartTraining={onStartTraining}
            onValidate={onValidate}
            onDelete={onDelete}
          />
        ))}
      </TableBody>
    </Table>
  </div>
);

const DatasetsCard = ({
  datasets,
  isLoading,
  canStartTraining,
  canValidate,
  canDelete,
  onView,
  onStartTraining,
  onValidate,
  onDelete,
}: {
  datasets: DatasetListItem[];
  isLoading: boolean;
  canStartTraining: boolean;
  canValidate: boolean;
  canDelete: boolean;
  onView: (id: string) => void;
  onStartTraining: (id: string) => void;
  onValidate: (id: string) => void;
  onDelete: (id: string) => void;
}) => (
  <Card>
    <CardHeader>
      <CardTitle className="flex items-center gap-2">
        <Database className="h-5 w-5" />
        Document Collections
        {datasets.length > 0 && (
          <span className="text-sm font-normal text-muted-foreground">({datasets.length} total)</span>
        )}
      </CardTitle>
    </CardHeader>
    <CardContent>
      {isLoading && datasets.length === 0 && (
        <div className="py-8 text-center text-muted-foreground">
          <RefreshCw className="mx-auto mb-2 h-6 w-6 animate-spin" />
          Loading {TERMS.datasets}...
        </div>
      )}
      {!isLoading && datasets.length === 0 && (
        <div className="py-8 text-center text-muted-foreground">
          <Database className="mx-auto mb-2 h-8 w-8 opacity-50" />
          <p>{TERMS.noDatasets}</p>
          <p className="mt-1 text-sm">{TERMS.noDatasetsDescription}</p>
        </div>
      )}
      {!isLoading && datasets.length > 0 && (
        <DatasetsTable
          datasets={datasets}
          canStartTraining={canStartTraining}
          canValidate={canValidate}
          canDelete={canDelete}
          onView={onView}
          onStartTraining={onStartTraining}
          onValidate={onValidate}
          onDelete={onDelete}
        />
      )}
    </CardContent>
  </Card>
);

const UploadDatasetDialog = ({
  isOpen,
  onOpenChange,
  onSuccess,
  onCancel,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess: () => void;
  onCancel: () => void;
}) => (
  <Dialog open={isOpen} onOpenChange={onOpenChange}>
    <DialogContent className="max-w-2xl">
      <DialogHeader>
        <DialogTitle>{TERMS.uploadDataset}</DialogTitle>
      </DialogHeader>
      <UploadDatasetForm onSuccess={onSuccess} onCancel={onCancel} />
    </DialogContent>
  </Dialog>
);

const DeleteDatasetDialog = ({
  datasetId,
  onClose,
  onConfirm,
  isDeleting,
}: {
  datasetId: string | null;
  onClose: () => void;
  onConfirm: () => void;
  isDeleting: boolean;
}) => (
  <AlertDialog open={!!datasetId} onOpenChange={onClose}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{TERMS.deleteDataset}</AlertDialogTitle>
        <AlertDialogDescription>
          Are you sure you want to delete this collection? This action cannot be undone.
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel>Cancel</AlertDialogCancel>
        <AlertDialogAction
          onClick={onConfirm}
          disabled={isDeleting}
          className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
        >
          {isDeleting ? 'Deleting...' : 'Delete'}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
);

const DatasetDetailDialog = ({
  dataset,
  onClose,
}: {
  dataset: Dataset | null;
  onClose: () => void;
}) => (
  <Dialog open={!!dataset} onOpenChange={onClose}>
    <DialogContent className="max-w-2xl">
      <DialogHeader>
        <DialogTitle>Collection Details</DialogTitle>
      </DialogHeader>
      {dataset && (
        <div className="space-y-4">
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
              <p className="font-medium capitalize">{dataset.source_type}</p>
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
              <Label className="text-muted-foreground">Status</Label>
              <StatusBadge status={dataset.validation_status} />
            </div>
            <div>
              <Label className="text-muted-foreground">Files</Label>
              <p className="font-medium">{formatNumber(dataset.file_count || 0)}</p>
            </div>
            <div>
              <Label className="text-muted-foreground">Total Tokens</Label>
              <p className="font-medium">{formatNumber(dataset.total_tokens || 0)}</p>
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
          <div>
            <Label className="text-muted-foreground">Hash (BLAKE3)</Label>
            <p className="break-all font-mono text-xs">{dataset.hash_b3}</p>
          </div>
        </div>
      )}
    </DialogContent>
  </Dialog>
);

const TrainingWizardDialogWrapper = ({
  isOpen,
  onOpenChange,
  initialDatasetId,
  onComplete,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  initialDatasetId?: string;
  onComplete: (jobId: string) => void;
}) => (
  <Dialog open={isOpen} onOpenChange={onOpenChange}>
    <DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto">
      <TrainingWizard
        initialDatasetId={initialDatasetId}
        onComplete={onComplete}
        onCancel={() => onOpenChange(false)}
      />
    </DialogContent>
  </Dialog>
);

export function DatasetsTab() {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const location = useLocation();
  const navigate = useNavigate();

  const [isUploadDialogOpen, setIsUploadDialogOpen] = useState(false);
  const [deleteDatasetId, setDeleteDatasetId] = useState<string | null>(null);
  const [isTrainingWizardOpen, setIsTrainingWizardOpen] = useState(false);
  const [initialDatasetId, setInitialDatasetId] = useState<string | undefined>(undefined);
  const [selectedDataset, setSelectedDataset] = useState<Dataset | null>(null);

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

  useEffect(() => {
    const shouldOpenUpload = (location.state as { openUpload?: boolean } | null)?.openUpload;
    if (shouldOpenUpload) {
      setIsUploadDialogOpen(true);
      navigate(location.pathname, { replace: true, state: {} });
    }
  }, [location.pathname, location.state, navigate]);

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

  return (
    <div className="space-y-6">
      <ActionBar
        canUpload={can('dataset:upload')}
        onOpenUpload={() => setIsUploadDialogOpen(true)}
        onRefresh={refetch}
        isLoading={isLoading}
      />

      <PageErrors errors={errors} />

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <p className="text-destructive">Failed to load {TERMS.datasets}: {error.message}</p>
            <Button variant="outline" onClick={() => refetch()} className="mt-2">
              Retry
            </Button>
          </CardContent>
        </Card>
      )}

      <DatasetsCard
        datasets={datasets}
        isLoading={isLoading}
        canStartTraining={can('training:start')}
        canValidate={can('dataset:validate')}
        canDelete={can('dataset:delete')}
        onView={(id) => navigate(`/training/datasets/${id}`)}
        onStartTraining={(id) => {
          navigate(`/training/jobs?datasetId=${encodeURIComponent(id)}`);
        }}
        onValidate={handleValidateDataset}
        onDelete={(id) => setDeleteDatasetId(id)}
      />

      <UploadDatasetDialog
        isOpen={isUploadDialogOpen}
        onOpenChange={setIsUploadDialogOpen}
        onSuccess={() => {
          setIsUploadDialogOpen(false);
          refetch();
        }}
        onCancel={() => setIsUploadDialogOpen(false)}
      />

      <DeleteDatasetDialog
        datasetId={deleteDatasetId}
        onClose={() => setDeleteDatasetId(null)}
        onConfirm={handleDeleteDataset}
        isDeleting={isDeleting}
      />

      <DatasetDetailDialog dataset={selectedDataset} onClose={() => setSelectedDataset(null)} />

      <TrainingWizardDialogWrapper
        isOpen={isTrainingWizardOpen}
        onOpenChange={(open) => {
          setIsTrainingWizardOpen(open);
          if (!open) {
            setInitialDatasetId(undefined);
          }
        }}
        initialDatasetId={initialDatasetId}
        onComplete={() => {
          setIsTrainingWizardOpen(false);
          setInitialDatasetId(undefined);
        }}
      />
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
        <Label htmlFor="name">{TERMS.datasetName}</Label>
        <Input
          id="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-collection"
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
            <SelectItem value="uploaded_files">{formatSourceType('uploaded_files')}</SelectItem>
            <SelectItem value="code_repo">{formatSourceType('code_repo')}</SelectItem>
            <SelectItem value="generated">{formatSourceType('generated')}</SelectItem>
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
          <Label htmlFor="files">{TERMS.documents}</Label>
          <Input
            id="files"
            type="file"
            multiple
            onChange={(e) => setFiles(e.target.files)}
            accept=".py,.js,.ts,.tsx,.jsx,.json,.txt,.pdf,.md"
          />
        </div>
      )}

      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? 'Creating...' : TERMS.createDataset}
        </Button>
      </div>
    </form>
  );
}

export default withErrorBoundary(DatasetsTab, 'Failed to load datasets tab');
