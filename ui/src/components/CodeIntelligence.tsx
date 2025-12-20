import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import {
  Code,
  GitBranch,
  Plus,
  RefreshCw,
  FileText,
  Trash2,
  MoreHorizontal,
  FolderOpen,
  Brain,
  Target,
  Zap,
  CheckCircle,
  AlertCircle
} from 'lucide-react';
import { apiClient } from '@/api/services';
import { Repository, Commit, User, RepositoryReportResponse } from '@/api/types';
import { GitFolderPicker } from './GitFolderPicker';
import { CodeIntelligenceTraining } from './CodeIntelligenceTraining';

import { logger, toError } from '@/utils/logger';
import { errorRecoveryTemplates } from './ui/error-recovery';
import { ACTIVITY_EVENT_TYPES } from '@/api/activityEventTypes';
import { toast } from 'sonner';

interface CodeIntelligenceProps {
  user?: User;
  selectedTenant: string;
}

export function CodeIntelligence({ user, selectedTenant }: CodeIntelligenceProps) {
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [commits, setCommits] = useState<Commit[]>([]);
  const [loading, setLoading] = useState(true);
  const [showReportModal, setShowReportModal] = useState(false);
  const [reportData, setReportData] = useState<RepositoryReportResponse | null>(null);
  const [showUnregisterModal, setShowUnregisterModal] = useState(false);
  const [selectedRepo, setSelectedRepo] = useState<Repository | null>(null);
  const [showFolderPicker, setShowFolderPicker] = useState(false);
  const [showTrainingDialog, setShowTrainingDialog] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const handleTrainingStarted = (sessionId: string) => {
    showStatus(`Adapter training started: ${sessionId}`, 'success');
    setShowTrainingDialog(false);
    fetchData();
  };

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [repos, commits] = await Promise.all([
        apiClient.listRepositories(),
        apiClient.listCommits(),
      ]);
      setRepositories(repos);
      setCommits(commits.slice(0, 10)); // Latest 10 commits
      setStatusMessage(null);
      setErrorRecovery(null);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch code intelligence data';
      logger.error('Failed to fetch code intelligence data', {
        component: 'CodeIntelligence',
        operation: 'fetchData',
        tenantId: selectedTenant,
        errorMessage: errorMsg,
      }, toError(err));
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => fetchData()
        )
      );
    } finally {
      setLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleTriggerScan = async (repoId: string) => {
    try {
      await apiClient.triggerRepositoryScan(repoId);
      showStatus('Repository scan triggered.', 'success');
      
      // Emit activity event (tenant_id/user_id auto-extracted from JWT)
      try {
        await apiClient.createActivityEvent({
          event_type: ACTIVITY_EVENT_TYPES.REPO_SCAN_TRIGGERED,
          target_type: 'repository',
          target_id: repoId,
          metadata_json: JSON.stringify({
            repo_id: repoId,
          }),
        });
      } catch (activityErr) {
        // Non-blocking: log but don't fail the scan
        logger.warn('Failed to emit scan activity event', {
          component: 'CodeIntelligence',
          operation: 'handleTriggerScan',
          error: activityErr instanceof Error ? activityErr.message : String(activityErr),
        });
      }
    } catch (err) {
      setStatusMessage({ message: 'Failed to trigger scan.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to trigger scan'),
          () => handleTriggerScan(repoId)
        )
      );
      logger.error('Failed to trigger repository scan', {
        component: 'CodeIntelligence',
        operation: 'triggerScan',
        repoId,
        tenantId: selectedTenant,
      }, toError(err));
    }
  };

  const handleViewReport = async (repo: Repository) => {
    try {
      const report = await apiClient.getRepositoryReport(repo.id);
      setReportData(report);
      setSelectedRepo(repo);
      setShowReportModal(true);
      
      // Emit activity event (tenant_id/user_id auto-extracted from JWT)
      try {
        await apiClient.createActivityEvent({
          event_type: ACTIVITY_EVENT_TYPES.REPO_REPORT_VIEWED,
          target_type: 'repository',
          target_id: repo.id,
          metadata_json: JSON.stringify({
            repo_id: repo.id,
          }),
        });
      } catch (activityErr) {
        // Non-blocking: log but don't fail the report view
        logger.warn('Failed to emit report view activity event', {
          component: 'CodeIntelligence',
          operation: 'handleViewReport',
          error: activityErr instanceof Error ? activityErr.message : String(activityErr),
        });
      }
    } catch (err) {
      setStatusMessage({ message: 'Failed to fetch repository report.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to fetch repository report'),
          () => handleViewReport(repo)
        )
      );
      logger.error('Failed to fetch repository report', {
        component: 'CodeIntelligence',
        operation: 'getRepositoryReport',
        repoId: repo.id,
        tenantId: selectedTenant,
      }, toError(err));
    }
  };

  const handleUnregister = async () => {
    if (!selectedRepo) return;
    try {
      await apiClient.unregisterRepository(selectedRepo.id);
      showStatus('Repository unregistered.', 'success');
      setShowUnregisterModal(false);
      setSelectedRepo(null);
      fetchData();
    } catch (err) {
      setStatusMessage({ message: 'Failed to unregister repository.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to unregister repository'),
          () => handleUnregister()
        )
      );
      logger.error('Failed to unregister repository', {
        component: 'CodeIntelligence',
        operation: 'unregisterRepository',
        repoId: selectedRepo.id,
        tenantId: selectedTenant,
      }, toError(err));
    }
  };

  const handleFolderSelect = (folderPath: string, repoInfo: { name: string; path: string }) => {
    setShowFolderPicker(false);
    setShowTrainingDialog(true);

    showStatus(`Selected repository: ${repoInfo.name}`, 'info');

    toast.success(`Selected repository: ${repoInfo.name}`);
  };

  const handleStartTraining = async (config: {
    repositoryPath: string;
    adapterName: string;
    description: string;
    trainingConfig: Record<string, string | number | boolean>;
  }) => {
    try {
      logger.info('Starting adapter training', {
        component: 'CodeIntelligence',
        operation: 'startTraining',
        repositoryPath: config.repositoryPath,
        adapterName: config.adapterName,
        tenantId: selectedTenant,
        userId: user?.id
      });

      // Call the training API
      const trainingSession = await apiClient.startAdapterTraining({
        repository_path: config.repositoryPath,
        adapter_name: config.adapterName,
        description: config.description,
        training_config: config.trainingConfig,
        tenant_id: selectedTenant
      });

      toast.success(`Adapter training started: ${trainingSession.session_id}`);
      setShowTrainingDialog(false);
      fetchData(); // Refresh to show new repository
      
      logger.info('Adapter training started successfully', {
        component: 'CodeIntelligence',
        operation: 'startTraining',
        sessionId: trainingSession.session_id,
        tenantId: selectedTenant,
        userId: user?.id
      });
      
    } catch (err) {
      toast.error('Failed to start training');
      logger.error('Failed to start adapter training', {
        component: 'CodeIntelligence',
        operation: 'startTraining',
        repositoryPath: config.repositoryPath,
        adapterName: config.adapterName,
        tenantId: selectedTenant,
        userId: user?.id
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading code intelligence data...</div>;
  }

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          {statusMessage.variant === 'success' ? (
            <CheckCircle className="h-4 w-4 text-green-600" />
          ) : (
            <AlertCircle className={`h-4 w-4 ${statusMessage.variant === 'warning' ? 'text-amber-600' : 'text-blue-600'}`} />
          )}
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold">Code Intelligence</h1>
          <p className="text-sm text-muted-foreground">
            Build codebase adapters from your Git repositories
          </p>
        </div>
        <div className="flex space-x-2">
          <Button onClick={() => setShowFolderPicker(true)}>
            <FolderOpen className="icon-standard mr-2" />
            Add Local Repository
          </Button>
          <Button variant="outline">
            <Plus className="icon-standard mr-2" />
            Register Remote Repository
          </Button>
        </div>
      </div>

      <Tabs defaultValue="repositories" className="space-y-4">
        <TabsList>
          <TabsTrigger value="repositories">Repositories</TabsTrigger>
          <TabsTrigger value="training">Training</TabsTrigger>
          <TabsTrigger value="commits">Commits</TabsTrigger>
        </TabsList>

        <TabsContent value="repositories" className="space-y-4">

          <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
            <CardHeader>
              <CardTitle>Repositories</CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="border-collapse w-full">
                <TableHeader>
                  <TableRow>
                    <TableHead>Repository</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commits</TableHead>
                    <TableHead>Last Scan</TableHead>
                    <TableHead>Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {repositories.map((repo) => (
                    <TableRow key={repo.id}>
                      <TableCell className="p-4 border-b border-border font-medium">{repo.url}</TableCell>
                      <TableCell className="p-4 border-b border-border">
                        <Badge variant="outline">
                          <GitBranch className="h-3 w-3 mr-1" />
                          {repo.branch}
                        </Badge>
                      </TableCell>
                      <TableCell className="p-4 border-b border-border">{repo.commit_count}</TableCell>
                      <TableCell className="p-4 border-b border-border">
                        {repo.last_scan
                          ? new Date(repo.last_scan).toLocaleString()
                          : 'Never'}
                      </TableCell>
                      <TableCell className="p-4 border-b border-border">
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem onClick={() => handleTriggerScan(repo.id)}>
                              <RefreshCw className="mr-2 h-4 w-4" />
                              Trigger Scan
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => handleViewReport(repo)}>
                              <FileText className="mr-2 h-4 w-4" />
                              View Report
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={() => {
                              setSelectedRepo(repo);
                              setShowUnregisterModal(true);
                            }}>
                              <Trash2 className="mr-2 h-4 w-4 text-red-600" />
                              Unregister
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    </TableRow>
                  ))}
                  {repositories.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={5} className="p-4 border-b border-border text-center text-muted-foreground">
                        No repositories registered
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="training" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Brain className="mr-2 h-5 w-5" />
                Adapter Training
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-center py-8">
                <Target className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">Ready to Train Adapters</h3>
                <p className="text-muted-foreground mb-4">
                  Select a repository to start training codebase-specific adapters
                </p>
                <Button onClick={() => setShowFolderPicker(true)}>
                  <FolderOpen className="mr-2 h-4 w-4" />
                  Select Repository
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="commits" className="space-y-4">
          <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
            <CardHeader>
              <CardTitle>Recent Commits</CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="border-collapse w-full">
                <TableHeader>
                  <TableRow>
                    <TableHead>SHA</TableHead>
                    <TableHead>Message</TableHead>
                    <TableHead>Author</TableHead>
                    <TableHead>Timestamp</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {commits.map((commit) => (
                    <TableRow key={commit.sha}>
                      <TableCell className="p-4 border-b border-border font-mono text-xs">
                        {commit.sha.substring(0, 8)}
                      </TableCell>
                      <TableCell className="p-4 border-b border-border">{commit.message}</TableCell>
                      <TableCell className="p-4 border-b border-border">{commit.author}</TableCell>
                      <TableCell className="p-4 border-b border-border">{new Date(commit.timestamp).toLocaleString()}</TableCell>
                    </TableRow>
                  ))}
                  {commits.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={4} className="p-4 border-b border-border text-center text-muted-foreground">
                        No commits analyzed
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Report Modal */}
      <Dialog open={showReportModal} onOpenChange={setShowReportModal}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>Repository Analysis Report</DialogTitle>
          </DialogHeader>
          {reportData && selectedRepo && (
            <div className="space-y-4">
              <div>
                <Label>Repository</Label>
                <p className="text-sm text-muted-foreground">{selectedRepo.url}</p>
              </div>
              <div>
                <Label>Total Files</Label>
                <p>{reportData.total_files}</p>
              </div>
              <div>
                <Label>Total Lines</Label>
                <p>{reportData.total_lines}</p>
              </div>
              <div>
                <Label>Languages</Label>
                <div className="space-y-2 mt-2">
                  {Object.entries(reportData.languages ?? {}).map(([lang, stats]) => {
                    const langStats = stats as unknown as { files: number; lines: number };
                    return (
                      <div key={lang} className="flex justify-between items-center p-2 bg-gray-50 rounded">
                        <span className="font-medium">{lang}</span>
                        <span className="text-sm text-muted-foreground">
                          {langStats.files} files, {langStats.lines} lines
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
              <div>
                <Label>Ephemeral Adapters Generated</Label>
                <p>{reportData.ephemeral_adapters_count}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setShowReportModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Unregister Modal */}
      <Dialog open={showUnregisterModal} onOpenChange={setShowUnregisterModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Unregister Repository</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertDescription>
              This will remove the repository <strong>{selectedRepo?.url}</strong> and all associated
              ephemeral adapters. This action cannot be undone.
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowUnregisterModal(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleUnregister}>
              Confirm Unregister
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Folder Picker Modal */}
      <Dialog open={showFolderPicker} onOpenChange={setShowFolderPicker}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Select Git Repository</DialogTitle>
          </DialogHeader>
          <GitFolderPicker
            onFolderSelect={handleFolderSelect}
            onCancel={() => setShowFolderPicker(false)}
          />
        </DialogContent>
      </Dialog>

      {/* Training Dialog */}
      <Dialog open={showTrainingDialog} onOpenChange={setShowTrainingDialog}>
        <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center">
              <Zap className="mr-2 h-5 w-5" />
              Train Codebase Adapter
            </DialogTitle>
          </DialogHeader>
          <CodeIntelligenceTraining
            tenantId={selectedTenant}
            userId={user?.id || ''}
            onTrainingStarted={handleTrainingStarted}
            onCancel={() => setShowTrainingDialog(false)}
            initialConfig={{
              category: 'codebase',
              scope: 'repo',
              rank: 24,
              alpha: 48,
              epochs: 3,
              learning_rate: 0.001,
              batch_size: 32,
              targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'],
            }}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}
