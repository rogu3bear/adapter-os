import React, { useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
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
import { apiClient } from '@/api/client';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { usePolling } from '@/hooks/usePolling';
import { toast } from 'sonner';
import {
  RefreshCw,
  Plus,
  Trash2,
  Search,
  FileCode,
  GitBranch,
  CheckCircle,
  XCircle,
  Clock,
} from 'lucide-react';
import { PageHeader } from '@/components/ui/page-header';
import type { Repository } from '@/api/types';

function CodeIntelligencePageInner() {
  const { density, setDensity } = useDensity();
  const { can } = useRBAC();

  const [isRegisterDialogOpen, setIsRegisterDialogOpen] = useState(false);
  const [deleteRepoId, setDeleteRepoId] = useState<string | null>(null);
  const [scanningRepoId, setScanningRepoId] = useState<string | null>(null);

  // Form state for repository registration
  const [repoPath, setRepoPath] = useState('');
  const [repoName, setRepoName] = useState('');
  const [repoDescription, setRepoDescription] = useState('');
  const [isRegistering, setIsRegistering] = useState(false);

  // Fetch repositories
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

  // Register repository
  const handleRegisterRepository = async () => {
    if (!repoPath.trim() || !repoName.trim()) {
      toast.error('Repository path and name are required');
      return;
    }

    setIsRegistering(true);
    try {
      await apiClient.registerRepository({
        path: repoPath.trim(),
        name: repoName.trim(),
        description: repoDescription.trim() || undefined,
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

  // Trigger repository scan
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

  // Delete repository
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
    <FeatureLayout title="Code Intelligence">
      <PageHeader
        title="Code Intelligence"
        description="Manage code repositories and trigger scans"
      >
        <DensityControls density={density} onDensityChange={setDensity} />
      </PageHeader>

      <div className="space-y-6">
        {/* Controls */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Controls
              <HelpTooltip helpId="code-controls">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex gap-4 items-center flex-wrap">
              <Dialog open={isRegisterDialogOpen} onOpenChange={setIsRegisterDialogOpen}>
                <DialogTrigger asChild>
                  <Button disabled={!can('code:register')}>
                    <Plus className="h-4 w-4 mr-2" />
                    Register Repository
                  </Button>
                </DialogTrigger>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Register Repository</DialogTitle>
                    <DialogDescription>
                      Register a local code repository for analysis
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-4">
                    <div>
                      <Label htmlFor="repo-path">Repository Path *</Label>
                      <Input
                        id="repo-path"
                        value={repoPath}
                        onChange={(e) => setRepoPath(e.target.value)}
                        placeholder="/path/to/repository"
                      />
                    </div>
                    <div>
                      <Label htmlFor="repo-name">Repository Name *</Label>
                      <Input
                        id="repo-name"
                        value={repoName}
                        onChange={(e) => setRepoName(e.target.value)}
                        placeholder="my-project"
                      />
                    </div>
                    <div>
                      <Label htmlFor="repo-desc">Description</Label>
                      <Input
                        id="repo-desc"
                        value={repoDescription}
                        onChange={(e) => setRepoDescription(e.target.value)}
                        placeholder="Optional description"
                      />
                    </div>
                  </div>
                  <DialogFooter>
                    <Button
                      variant="outline"
                      onClick={() => setIsRegisterDialogOpen(false)}
                      disabled={isRegistering}
                    >
                      Cancel
                    </Button>
                    <Button onClick={handleRegisterRepository} disabled={isRegistering}>
                      {isRegistering ? 'Registering...' : 'Register'}
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>

              <Button onClick={() => refetch()} disabled={isLoading} variant="outline">
                <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
                Refresh
              </Button>

              {lastUpdated && (
                <span className="text-xs text-muted-foreground">
                  Last updated: {lastUpdated.toLocaleTimeString()}
                </span>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Repositories Table */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileCode className="h-5 w-5" />
              Repositories
              {repositories.length > 0 && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  ({repositories.length} total)
                </span>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {error && (
              <ErrorRecovery
                error={error.message}
                onRetry={() => refetch()}
              />
            )}

            {isLoading && repositories.length === 0 ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : repositories.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                No repositories registered. Click "Register Repository" to add one.
              </div>
            ) : (
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
                        <TableCell className="font-mono text-sm">{repo.path}</TableCell>
                        <TableCell className="max-w-md truncate">
                          {repo.description || '-'}
                        </TableCell>
                        <TableCell>{getScanStatusBadge(repo.scan_status)}</TableCell>
                        <TableCell className="text-right">
                          <div className="flex gap-2 justify-end">
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => handleScanRepository(repo.id)}
                              disabled={scanningRepoId === repo.id || !can('code:scan')}
                            >
                              {scanningRepoId === repo.id ? (
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
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => setDeleteRepoId(repo.id)}
                              disabled={!can('code:unregister')}
                            >
                              <Trash2 className="h-3 w-3 mr-1" />
                              Delete
                            </Button>
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

        {/* Delete Confirmation Dialog */}
        <AlertDialog open={!!deleteRepoId} onOpenChange={() => setDeleteRepoId(null)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete Repository?</AlertDialogTitle>
              <AlertDialogDescription>
                This will unregister the repository from code intelligence. This action
                cannot be undone.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={handleDeleteRepository}>
                Delete
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
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
