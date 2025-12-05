import React, { useCallback, useState } from 'react';
import { apiClient } from '@/api/client';
import FeatureLayout from '@/layout/FeatureLayout';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
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
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';
import { usePolling } from '@/hooks/usePolling';
import type { InformationDensity } from '@/hooks/useInformationDensity';
import { toast } from 'sonner';
import {
  CheckCircle,
  Clock,
  FileCode,
  Plus,
  RefreshCw,
  Search,
  XCircle,
  Trash2,
} from 'lucide-react';
import type { Repository } from '@/api/types';

type RegisterDialogProps = {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  repoPath: string;
  repoName: string;
  repoDescription: string;
  onRepoPathChange: (value: string) => void;
  onRepoNameChange: (value: string) => void;
  onRepoDescriptionChange: (value: string) => void;
  onSubmit: () => void;
  isSubmitting: boolean;
  canRegister: boolean;
};

type ControlsCardProps = {
  density: InformationDensity;
  setDensity: (value: InformationDensity) => void;
  isRegisterDialogOpen: boolean;
  onRegisterDialogChange: (open: boolean) => void;
  repoPath: string;
  repoName: string;
  repoDescription: string;
  onRepoPathChange: (value: string) => void;
  onRepoNameChange: (value: string) => void;
  onRepoDescriptionChange: (value: string) => void;
  onRegister: () => void;
  isRegistering: boolean;
  onRefresh: () => void;
  isLoading: boolean;
  lastUpdated?: Date;
  canRegister: boolean;
};

type RepoActionsProps = {
  repoId: string;
  onScan: (id: string) => void;
  onDelete: (id: string) => void;
  isScanning: boolean;
  canScan: boolean;
  canDelete: boolean;
};

type RepositoriesCardProps = {
  repositories: Repository[];
  isLoading: boolean;
  error: Error | null;
  onRetry: () => void;
  scanningRepoId: string | null;
  onScan: (id: string) => void;
  onDeleteRequest: (id: string) => void;
  canScan: boolean;
  canDelete: boolean;
  getScanStatusBadge: (status?: string) => React.ReactNode;
};

type DeleteRepositoryDialogProps = {
  repoId: string | null;
  onClose: () => void;
  onConfirm: () => void;
};

const Loader = () => (
  <div className="flex justify-center py-8">
    <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
  </div>
);

const EmptyState = () => (
  <div className="text-center py-8 text-muted-foreground">
    No repositories registered. Click "Register Repository" to add one.
  </div>
);

function RegisterRepositoryDialog({
  isOpen,
  onOpenChange,
  repoPath,
  repoName,
  repoDescription,
  onRepoPathChange,
  onRepoNameChange,
  onRepoDescriptionChange,
  onSubmit,
  isSubmitting,
  canRegister,
}: RegisterDialogProps) {
  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button disabled={!canRegister}>
          <Plus className="h-4 w-4 mr-2" />
          Register Repository
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Register Repository</DialogTitle>
          <DialogDescription>Register a local code repository for analysis</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <Label htmlFor="repo-path">Repository Path *</Label>
            <Input
              id="repo-path"
              value={repoPath}
              onChange={(e) => onRepoPathChange(e.target.value)}
              placeholder="/path/to/repository"
            />
          </div>
          <div>
            <Label htmlFor="repo-name">Repository Name *</Label>
            <Input
              id="repo-name"
              value={repoName}
              onChange={(e) => onRepoNameChange(e.target.value)}
              placeholder="my-project"
            />
          </div>
          <div>
            <Label htmlFor="repo-desc">Description</Label>
            <Input
              id="repo-desc"
              value={repoDescription}
              onChange={(e) => onRepoDescriptionChange(e.target.value)}
              placeholder="Optional description"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={isSubmitting}>
            {isSubmitting ? 'Registering...' : 'Register'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ControlsCard({
  density,
  setDensity,
  isRegisterDialogOpen,
  onRegisterDialogChange,
  repoPath,
  repoName,
  repoDescription,
  onRepoPathChange,
  onRepoNameChange,
  onRepoDescriptionChange,
  onRegister,
  isRegistering,
  onRefresh,
  isLoading,
  lastUpdated,
  canRegister,
}: ControlsCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          Controls
          <GlossaryTooltip termId="code-controls">
            <span className="cursor-help text-muted-foreground">(?)</span>
          </GlossaryTooltip>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex gap-4 items-center flex-wrap">
          <RegisterRepositoryDialog
            isOpen={isRegisterDialogOpen}
            onOpenChange={onRegisterDialogChange}
            repoPath={repoPath}
            repoName={repoName}
            repoDescription={repoDescription}
            onRepoPathChange={onRepoPathChange}
            onRepoNameChange={onRepoNameChange}
            onRepoDescriptionChange={onRepoDescriptionChange}
            onSubmit={onRegister}
            isSubmitting={isRegistering}
            canRegister={canRegister}
          />
          <Button onClick={onRefresh} disabled={isLoading} variant="outline">
            <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
          <DensityControls density={density} onDensityChange={setDensity} />
          {lastUpdated && (
            <span className="text-xs text-muted-foreground">
              Last updated: {lastUpdated.toLocaleTimeString()}
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function RepoActions({
  repoId,
  onScan,
  onDelete,
  isScanning,
  canScan,
  canDelete,
}: RepoActionsProps) {
  return (
    <div className="flex gap-2 justify-end">
      <Button
        size="sm"
        variant="outline"
        onClick={() => onScan(repoId)}
        disabled={isScanning || !canScan}
      >
        {isScanning ? (
          <>
            <Clock className="h-3 w-3 mr-1 animate-spin" />
            Scanning...
          </>
        ) : (
          <>
            <Search className="h-3 w-3 mr-1" />
            Scan
          </>
        )}
      </Button>
      <Button size="sm" variant="outline" onClick={() => onDelete(repoId)} disabled={!canDelete}>
        <Trash2 className="h-3 w-3 mr-1" />
        Delete
      </Button>
    </div>
  );
}

function RepositoriesTable({
  repositories,
  scanningRepoId,
  onScan,
  onDeleteRequest,
  canScan,
  canDelete,
  getScanStatusBadge,
}: Omit<RepositoriesCardProps, 'isLoading' | 'error' | 'onRetry'>) {
  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Path</TableHead>
            <TableHead>Description</TableHead>
            <TableHead>Scan Status</TableHead>
            <TableHead className="text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {repositories.map((repo) => (
            <TableRow key={repo.id}>
              <TableCell className="font-medium">{repo.name}</TableCell>
              <TableCell className="font-mono text-sm">{repo.url}</TableCell>
              <TableCell className="max-w-md truncate">-</TableCell>
              <TableCell>{getScanStatusBadge(repo.status)}</TableCell>
              <TableCell className="text-right">
                <RepoActions
                  repoId={repo.id}
                  onScan={onScan}
                  onDelete={onDeleteRequest}
                  isScanning={scanningRepoId === repo.id}
                  canScan={canScan}
                  canDelete={canDelete}
                />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function RepositoriesCard({
  repositories,
  isLoading,
  error,
  onRetry,
  scanningRepoId,
  onScan,
  onDeleteRequest,
  canScan,
  canDelete,
  getScanStatusBadge,
}: RepositoriesCardProps) {
  const hasData = repositories.length > 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FileCode className="h-5 w-5" />
          Repositories
          {hasData && (
            <span className="ml-2 text-sm font-normal text-muted-foreground">
              ({repositories.length} total)
            </span>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {error && <ErrorRecovery error={error.message} onRetry={onRetry} />}
        {isLoading && !hasData && <Loader />}
        {!isLoading && !hasData && <EmptyState />}
        {hasData && (
          <RepositoriesTable
            repositories={repositories}
            scanningRepoId={scanningRepoId}
            onScan={onScan}
            onDeleteRequest={onDeleteRequest}
            canScan={canScan}
            canDelete={canDelete}
            getScanStatusBadge={getScanStatusBadge}
          />
        )}
      </CardContent>
    </Card>
  );
}

function DeleteRepositoryDialog({ repoId, onClose, onConfirm }: DeleteRepositoryDialogProps) {
  return (
    <AlertDialog open={!!repoId} onOpenChange={onClose}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete Repository?</AlertDialogTitle>
          <AlertDialogDescription>
            This will unregister the repository from code intelligence. This action cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function CodeIntelligencePageInner() {
  const { density, setDensity } = useDensity();
  const { can } = useRBAC();

  const [isRegisterDialogOpen, setIsRegisterDialogOpen] = useState(false);
  const [deleteRepoId, setDeleteRepoId] = useState<string | null>(null);
  const [scanningRepoId, setScanningRepoId] = useState<string | null>(null);
  const [repoPath, setRepoPath] = useState('');
  const [repoName, setRepoName] = useState('');
  const [repoDescription, setRepoDescription] = useState('');
  const [isRegistering, setIsRegistering] = useState(false);

  const fetchRepositories = useCallback(async () => {
    return await apiClient.listRepositories();
  }, []);

  const {
    data: repositories = [],
    isLoading,
    error,
    refetch,
    lastUpdated,
  } = usePolling<Repository[]>(fetchRepositories, 'normal', {
    enabled: true,
    operationName: 'fetchCodeRepositories',
  });

  const handleRegisterRepository = async () => {
    if (!repoPath.trim() || !repoName.trim()) {
      toast.error('Repository path and name are required');
      return;
    }

    setIsRegistering(true);
    try {
      await apiClient.registerRepository({
        repo_id: repoName.trim(),
        path: repoPath.trim(),
      });

      toast.success('Repository registered successfully');
      setIsRegisterDialogOpen(false);
      setRepoPath('');
      setRepoName('');
      setRepoDescription('');
      refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to register repository');
    } finally {
      setIsRegistering(false);
    }
  };

  const handleScanRepository = async (repositoryId: string) => {
    setScanningRepoId(repositoryId);
    try {
      const response = await apiClient.triggerRepositoryScan(repositoryId);
      toast.success(`Scan started. Job ID: ${response.job_id}`);
      refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start scan');
    } finally {
      setScanningRepoId(null);
    }
  };

  const handleDeleteRepository = async () => {
    if (!deleteRepoId) return;

    try {
      await apiClient.unregisterRepository(deleteRepoId);
      toast.success('Repository deleted successfully');
      setDeleteRepoId(null);
      refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete repository');
    }
  };

  const getScanStatusBadge = (status?: string) => {
    switch (status) {
      case 'completed':
        return (
          <Badge variant="default" className="gap-1">
            <CheckCircle className="h-3 w-3" />
            Completed
          </Badge>
        );
      case 'running':
        return (
          <Badge variant="secondary" className="gap-1">
            <Clock className="h-3 w-3 animate-spin" />
            Running
          </Badge>
        );
      case 'failed':
        return (
          <Badge variant="destructive" className="gap-1">
            <XCircle className="h-3 w-3" />
            Failed
          </Badge>
        );
      case 'pending':
        return (
          <Badge variant="outline" className="gap-1">
            <Clock className="h-3 w-3" />
            Pending
          </Badge>
        );
      default:
        return (
          <Badge variant="outline" className="gap-1">
            Not scanned
          </Badge>
        );
    }
  };

  return (
    <FeatureLayout
      title="Code Intelligence"
      description="Manage code repositories and trigger scans"
      headerActions={<DensityControls density={density} onDensityChange={setDensity} />}
    >
      <SectionErrorBoundary sectionName="Code Intelligence">
        <div className="space-y-6">
          <ControlsCard
            density={density}
            setDensity={setDensity}
            isRegisterDialogOpen={isRegisterDialogOpen}
            onRegisterDialogChange={setIsRegisterDialogOpen}
            repoPath={repoPath}
            repoName={repoName}
            repoDescription={repoDescription}
            onRepoPathChange={setRepoPath}
            onRepoNameChange={setRepoName}
            onRepoDescriptionChange={setRepoDescription}
            onRegister={handleRegisterRepository}
            isRegistering={isRegistering}
            onRefresh={refetch}
            isLoading={isLoading}
            lastUpdated={lastUpdated}
            canRegister={can('code:register')}
          />
          <RepositoriesCard
            repositories={repositories}
            isLoading={isLoading}
            error={error ?? null}
            onRetry={refetch}
            scanningRepoId={scanningRepoId}
            onScan={handleScanRepository}
            onDeleteRequest={setDeleteRepoId}
            canScan={can('code:scan')}
            canDelete={can('code:unregister')}
            getScanStatusBadge={getScanStatusBadge}
          />
        </div>
      </SectionErrorBoundary>
      <DeleteRepositoryDialog
        repoId={deleteRepoId}
        onClose={() => setDeleteRepoId(null)}
        onConfirm={handleDeleteRepository}
      />
    </FeatureLayout>
  );
}

export default function CodeIntelligencePage() {
  return (
    <DensityProvider pageKey="code-intelligence">
      <CodeIntelligencePageInner />
    </DensityProvider>
  );
}
