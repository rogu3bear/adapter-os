import React, { useState, useEffect } from 'react';
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
  Zap
} from 'lucide-react';
import apiClient from '../api/client';
import { Repository, Commit, User, RepositoryReportResponse } from '../api/types';
import { GitFolderPicker } from './GitFolderPicker';
import { CodeIntelligenceTraining } from './CodeIntelligenceTraining';
import { toast } from 'sonner';

interface CodeIntelligenceProps {
  user: User;
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
  const [selectedFolderPath, setSelectedFolderPath] = useState<string>('');

  const fetchData = async () => {
    setLoading(true);
    try {
      const [repos, commits] = await Promise.all([
        apiClient.listRepositories(),
        apiClient.listCommits(),
      ]);
      setRepositories(repos);
      setCommits(commits.slice(0, 10)); // Latest 10 commits
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch code intelligence data';
      console.error(errorMsg, err);
      toast.error(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [selectedTenant]);

  const handleTriggerScan = async (repoId: string) => {
    try {
      await apiClient.triggerRepositoryScan(repoId);
      toast.success('Repository scan triggered');
    } catch (err) {
      toast.error('Failed to trigger scan');
    }
  };

  const handleViewReport = async (repo: Repository) => {
    try {
      const report = await apiClient.getRepositoryReport(repo.id);
      setReportData(report);
      setSelectedRepo(repo);
      setShowReportModal(true);
    } catch (err) {
      toast.error('Failed to fetch repository report');
      console.error(err);
    }
  };

  const handleUnregister = async () => {
    if (!selectedRepo) return;
    try {
      await apiClient.unregisterRepository(selectedRepo.id);
      toast.success('Repository unregistered');
      setShowUnregisterModal(false);
      setSelectedRepo(null);
      fetchData();
    } catch (err) {
      toast.error('Failed to unregister repository');
      console.error(err);
    }
  };

  const handleFolderSelect = (folderPath: string, repoInfo: any) => {
    setSelectedFolderPath(folderPath);
    setShowFolderPicker(false);
    setShowTrainingDialog(true);
    toast.success(`Selected repository: ${repoInfo.name}`);
  };

  const handleStartTraining = async (config: any) => {
    try {
      // TODO: Implement actual training API call
      toast.success('Adapter training started');
      setShowTrainingDialog(false);
      fetchData(); // Refresh to show new repository
    } catch (err) {
      toast.error('Failed to start training');
      console.error(err);
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading code intelligence data...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Code Intelligence</h1>
          <p className="section-description">
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

          <Card className="card-standard">
            <CardHeader>
              <CardTitle>Repositories</CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="table-standard">
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
                      <TableCell className="table-cell-standard font-medium">{repo.url}</TableCell>
                      <TableCell className="table-cell-standard">
                        <Badge variant="outline">
                          <GitBranch className="icon-small mr-1" />
                          {repo.branch}
                        </Badge>
                      </TableCell>
                      <TableCell className="table-cell-standard">{repo.commit_count}</TableCell>
                      <TableCell className="table-cell-standard">
                        {repo.last_scan
                          ? new Date(repo.last_scan).toLocaleString()
                          : 'Never'}
                      </TableCell>
                      <TableCell className="table-cell-standard">
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
                      <TableCell colSpan={5} className="table-cell-standard text-center text-muted-foreground">
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
          <Card className="card-standard">
            <CardHeader>
              <CardTitle>Recent Commits</CardTitle>
            </CardHeader>
            <CardContent>
              <Table className="table-standard">
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
                      <TableCell className="table-cell-standard font-mono text-xs">
                        {commit.sha.substring(0, 8)}
                      </TableCell>
                      <TableCell className="table-cell-standard">{commit.message}</TableCell>
                      <TableCell className="table-cell-standard">{commit.author}</TableCell>
                      <TableCell className="table-cell-standard">{new Date(commit.timestamp).toLocaleString()}</TableCell>
                    </TableRow>
                  ))}
                  {commits.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={4} className="table-cell-standard text-center text-muted-foreground">
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
                  {Object.entries(reportData.languages).map(([lang, stats]) => (
                    <div key={lang} className="flex justify-between items-center p-2 bg-gray-50 rounded">
                      <span className="font-medium">{lang}</span>
                      <span className="text-sm text-muted-foreground">
                        {(stats as any).files} files, {(stats as any).lines} lines
                      </span>
                    </div>
                  ))}
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
            onConfigSelect={handleStartTraining}
            initialConfig={{
              category: 'codebase',
              scope: 'repo',
              rank: 24,
              alpha: 48,
              epochs: 3,
              learning_rate: 0.001,
              batch_size: 32,
              targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj']
            }}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}