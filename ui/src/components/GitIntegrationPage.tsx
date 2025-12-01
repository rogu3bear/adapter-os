import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import {
  GitBranch,
  GitCommit,
  GitMerge,
  Plus,
  RefreshCw,
  FileText,
  Calendar,
  User,
  Hash,
  TrendingUp,
  Activity,
  Code,
  AlertCircle
} from 'lucide-react';
import apiClient from '@/api/client';
import { Repository, Commit, CommitDiff } from '@/api/types';

import { logger } from '@/utils/logger';
import { Alert, AlertDescription } from './ui/alert';
import { errorRecoveryTemplates } from './ui/error-recovery';
import { toast } from 'sonner';

// Helper function to format repository URLs for display
// Handles cases where the URL is actually a repo_id fallback
function formatRepositoryUrl(url: string, isFallback?: boolean): string {
  // If explicitly marked as not a fallback, show the URL as-is
  if (isFallback === false) {
    return url;
  }

  // If explicitly marked as a fallback, format it nicely
  if (isFallback === true) {
    return `Repository: ${url}`;
  }

  // Fallback logic for backward compatibility (if isFallback is undefined)
  // If it looks like a proper URL (starts with http/https), show it as-is
  if (url.startsWith('http://') || url.startsWith('https://')) {
    return url;
  }

  // If it looks like a git URL (ends with .git), show it as-is
  if (url.endsWith('.git')) {
    return url;
  }

  // Otherwise, it's likely a repo_id fallback, format it nicely
  return `Repository: ${url}`;
}

interface GitIntegrationPageProps {
  selectedTenant: string;
}

export function GitIntegrationPage({ selectedTenant }: GitIntegrationPageProps) {
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [selectedRepo, setSelectedRepo] = useState<Repository | null>(null);
  const [commits, setCommits] = useState<Commit[]>([]);
  const [selectedCommit, setSelectedCommit] = useState<Commit | null>(null);
  const [commitDiff, setCommitDiff] = useState<CommitDiff | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);


  // New repository form
  const [newRepoUrl, setNewRepoUrl] = useState('');
  const [newRepoBranch, setNewRepoBranch] = useState('main');
  const [showAddRepo, setShowAddRepo] = useState(false);


  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const loadRepositories = useCallback(async () => {
    setIsLoading(true);
    try {
      const repos = await apiClient.listRepositories();
      setRepositories(repos);

      if (repos.length > 0) {
        setSelectedRepo((prev) => prev ?? repos[0]);
      }
      setStatusMessage(null);
      setErrorRecovery(null);

      if (repos.length > 0 && !selectedRepo) {
        setSelectedRepo(repos[0]);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load repositories';
      logger.error('Failed to load repositories', {
        component: 'GitIntegrationPage',
        operation: 'loadRepositories',
        tenant: selectedTenant,
        error: errorMessage
      });

      setStatusMessage({ message: 'Failed to load repositories.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to load repositories'),
          () => loadRepositories()
        )
      );
    } finally {
      setIsLoading(false);
    }
  }, [selectedTenant, selectedRepo]);

  useEffect(() => {
    loadRepositories();
  }, [loadRepositories]);

  const loadCommits = useCallback(async (repoId: string) => {
    setIsLoading(true);
    try {
      const commitsList = await apiClient.listCommits(repoId);
      setCommits(commitsList);

      setStatusMessage(null);
      setErrorRecovery(null);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load commits';
      logger.error('Failed to load commits', {
        component: 'GitIntegrationPage',
        operation: 'loadCommits',
        repoId,
        error: errorMessage
      });

      setStatusMessage({ message: 'Failed to load commits.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to load commits'),
          () => loadCommits(repoId)
        )
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (selectedRepo) {
      loadCommits(selectedRepo.id);
    }
  }, [selectedRepo, loadCommits]);

  const loadCommitDiff = async (sha: string) => {
    setIsLoading(true);
    try {
      const diff = await apiClient.getCommitDiff(sha);
      setCommitDiff(diff);

      setStatusMessage(null);
      setErrorRecovery(null);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load commit diff';
      logger.error('Failed to load commit diff', {
        component: 'GitIntegrationPage',
        operation: 'loadCommitDiff',
        sha,
        error: errorMessage
      });

      setStatusMessage({ message: 'Failed to load commit diff.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to load commit diff'),
          () => loadCommitDiff(sha)
        )
      );

      toast.error('Failed to load commit diff');
    } finally {
      setIsLoading(false);
    }
  };

  const handleAddRepository = async () => {
    if (!newRepoUrl.trim()) {

      showStatus('Please enter a repository URL.', 'warning');

      toast.error('Please enter a repository URL');
      return;
    }

    setIsLoading(true);
    try {
      await apiClient.registerRepository({
        repo_id: newRepoUrl.split('/').pop()?.replace('.git', '') || 'repository',
        path: newRepoUrl
      });

      showStatus('Repository registered successfully.', 'success');

      toast.success('Repository registered successfully');
      setShowAddRepo(false);
      setNewRepoUrl('');
      setNewRepoBranch('main');
      loadRepositories();
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to register repository';
      logger.error('Failed to register repository', {
        component: 'GitIntegrationPage',
        operation: 'handleAddRepository',
        url: newRepoUrl,
        branch: newRepoBranch,
        error: errorMessage
      });

      setStatusMessage({ message: 'Failed to register repository.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to register repository'),
          () => handleAddRepository()
        )
      );

      toast.error('Failed to register repository');
    } finally {
      setIsLoading(false);
    }
  };

  const handleScanRepository = async (repoId: string) => {
    setIsLoading(true);
    try {
      await apiClient.triggerRepositoryScan(repoId);

      showStatus('Repository scan started.', 'success');

      toast.success('Repository scan started');
      // Wait a bit and reload commits
      setTimeout(() => loadCommits(repoId), 2000);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to scan repository';
      logger.error('Failed to scan repository', {
        component: 'GitIntegrationPage',
        operation: 'handleScanRepository',
        repoId,
        error: errorMessage
      });

      setStatusMessage({ message: 'Failed to scan repository.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error('Failed to scan repository'),
          () => handleScanRepository(repoId)
        )
      );

      toast.error('Failed to scan repository');
    } finally {
      setIsLoading(false);
    }
  };

  const handleCommitClick = async (commit: Commit) => {
    setSelectedCommit(commit);
    await loadCommitDiff(commit.sha);
  };

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


      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Git Integration</h2>
          <p className="text-muted-foreground">
            Manage repositories, track commits, and analyze code changes
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={loadRepositories} disabled={isLoading}>
            <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
          <Button onClick={() => setShowAddRepo(!showAddRepo)}>
            <Plus className="w-4 h-4 mr-2" />
            Add Repository
          </Button>
        </div>
      </div>

      {/* Add Repository Form */}
      {showAddRepo && (
        <Card>
          <CardHeader>
            <CardTitle>Register New Repository</CardTitle>
            <CardDescription>
              Add a Git repository for code intelligence and adapter training
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="repo-url">Repository URL</Label>
              <Input
                id="repo-url"
                placeholder="https://github.com/user/repo.git"
                value={newRepoUrl}
                onChange={(e) => setNewRepoUrl(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="repo-branch">Branch</Label>
              <Input
                id="repo-branch"
                placeholder="main"
                value={newRepoBranch}
                onChange={(e) => setNewRepoBranch(e.target.value)}
              />
            </div>
            <div className="flex gap-2">
              <Button onClick={handleAddRepository} disabled={isLoading}>
                Register Repository
              </Button>
              <Button variant="outline" onClick={() => setShowAddRepo(false)}>
                Cancel
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Repository List */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <GitBranch className="w-5 h-5 mr-2" />
            Registered Repositories
          </CardTitle>
        </CardHeader>
        <CardContent>
          {repositories.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              <GitBranch className="w-12 h-12 mx-auto mb-3 opacity-20" />
              <p>No repositories registered yet</p>
              <Button
                variant="link"
                onClick={() => setShowAddRepo(true)}
                className="mt-2"
              >
                Add your first repository
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              {repositories.map((repo) => (
                <div
                  key={repo.id}
                  className={`
                    p-4 rounded-lg border cursor-pointer transition-colors
                    ${selectedRepo?.id === repo.id
                      ? 'border-primary bg-primary/5'
                      : 'border-border hover:bg-muted/50'
                    }
                  `}
                  onClick={() => setSelectedRepo(repo)}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <GitBranch className="w-4 h-4 text-muted-foreground" />

                        <span className="font-mono text-sm" title={repo.url}>
                          {formatRepositoryUrl(repo.url, repo.url_is_fallback)}
                        </span>

                        <span className="font-mono text-sm">{repo.url}</span>
                      </div>
                      <div className="flex items-center gap-4 mt-2 text-sm text-muted-foreground">
                        <span className="flex items-center gap-1">
                          <GitMerge className="w-3 h-3" />
                          {repo.branch}
                        </span>
                        <span className="flex items-center gap-1">
                          <GitCommit className="w-3 h-3" />
                          {repo.commit_count} commits
                        </span>
                        {repo.last_scan && (
                          <span className="flex items-center gap-1">
                            <Calendar className="w-3 h-3" />
                            Last scan: {new Date(repo.last_scan).toLocaleDateString()}
                          </span>
                        )}
                      </div>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleScanRepository(repo.id);
                      }}
                      disabled={isLoading}
                    >
                      <RefreshCw className={`w-3 h-3 mr-1 ${isLoading ? 'animate-spin' : ''}`} />
                      Scan
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Commits and Diff */}
      {selectedRepo && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Commits List */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <GitCommit className="w-5 h-5 mr-2" />
                Recent Commits
              </CardTitle>
              <CardDescription>
                {commits.length} commits in {selectedRepo.branch}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-2 max-h-[600px] overflow-y-auto">
                {commits.length === 0 ? (
                  <div className="text-center py-8 text-muted-foreground">
                    <GitCommit className="w-12 h-12 mx-auto mb-3 opacity-20" />
                    <p>No commits found</p>
                    <Button
                      variant="link"
                      onClick={() => handleScanRepository(selectedRepo.id)}
                      className="mt-2"
                    >
                      Scan repository
                    </Button>
                  </div>
                ) : (
                  commits.map((commit) => (
                    <div
                      key={commit.sha}
                      className={`
                        p-3 rounded-lg border cursor-pointer transition-colors
                        ${selectedCommit?.sha === commit.sha
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:bg-muted/50'
                        }
                      `}
                      onClick={() => handleCommitClick(commit)}
                    >
                      <div className="flex items-start gap-3">
                        <GitCommit className="w-4 h-4 text-muted-foreground mt-0.5" />
                        <div className="flex-1 min-w-0">
                          <div className="font-medium text-sm truncate">
                            {commit.message}
                          </div>
                          <div className="flex items-center gap-3 mt-1 text-xs text-muted-foreground">
                            <span className="flex items-center gap-1">
                              <User className="w-3 h-3" />
                              {commit.author}
                            </span>
                            <span className="flex items-center gap-1">
                              <Calendar className="w-3 h-3" />
                              {new Date(commit.timestamp).toLocaleDateString()}
                            </span>
                          </div>
                          <div className="flex items-center gap-1 mt-1">
                            <Hash className="w-3 h-3 text-muted-foreground" />
                            <code className="text-xs font-mono">{commit.sha.substring(0, 8)}</code>
                          </div>
                          {commit.diff_stats && (
                            <div className="flex items-center gap-3 mt-2 text-xs">
                              <Badge variant="outline" className="text-green-600">
                                +{commit.diff_stats.additions}
                              </Badge>
                              <Badge variant="outline" className="text-red-600">
                                -{commit.diff_stats.deletions}
                              </Badge>
                              <span className="text-muted-foreground">
                                {commit.diff_stats.files_changed} files
                              </span>
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </CardContent>
          </Card>

          {/* Commit Diff */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <FileText className="w-5 h-5 mr-2" />
                Commit Diff
              </CardTitle>
              {selectedCommit && (
                <CardDescription>
                  {selectedCommit.message} • {selectedCommit.sha.substring(0, 8)}
                </CardDescription>
              )}
            </CardHeader>
            <CardContent>
              {!selectedCommit ? (
                <div className="text-center py-12 text-muted-foreground">
                  <FileText className="w-12 h-12 mx-auto mb-3 opacity-20" />
                  <p>Select a commit to view diff</p>
                </div>
              ) : isLoading ? (
                <div className="text-center py-12">
                  <RefreshCw className="w-8 h-8 mx-auto mb-3 animate-spin text-primary" />
                  <p className="text-muted-foreground">Loading diff...</p>
                </div>
              ) : commitDiff ? (
                <div className="space-y-4">
                  {/* Stats */}
                  <div className="flex items-center gap-4 p-3 bg-muted rounded-lg">
                    <div className="flex items-center gap-2">
                      <FileText className="w-4 h-4 text-muted-foreground" />
                      <span className="text-sm font-medium">
                        {commitDiff.stats.files_changed} files changed
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-green-600 font-medium">
                        +{commitDiff.stats.insertions}
                      </span>
                      <span className="text-sm text-red-600 font-medium">
                        -{commitDiff.stats.deletions}
                      </span>
                    </div>
                  </div>

                  {/* Diff */}
                  <div className="max-h-[500px] overflow-y-auto">
                    <pre className="text-xs font-mono bg-muted p-4 rounded-lg overflow-x-auto">
                      {commitDiff.diff}
                    </pre>
                  </div>
                </div>
              ) : (
                <div className="text-center py-12 text-muted-foreground">
                  <AlertCircle className="w-12 h-12 mx-auto mb-3 opacity-20" />
                  <p>Failed to load diff</p>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      )}

      {/* Repository Stats */}
      {selectedRepo && commits.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <TrendingUp className="w-5 h-5 mr-2" />
              Repository Statistics
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 text-muted-foreground mb-1">
                  <GitCommit className="w-4 h-4" />
                  <span className="text-sm">Total Commits</span>
                </div>
                <div className="text-2xl font-bold">{selectedRepo.commit_count}</div>
              </div>
              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 text-muted-foreground mb-1">
                  <User className="w-4 h-4" />
                  <span className="text-sm">Contributors</span>
                </div>
                <div className="text-2xl font-bold">
                  {new Set(commits.map(c => c.author)).size}
                </div>
              </div>
              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 text-muted-foreground mb-1">
                  <Code className="w-4 h-4" />
                  <span className="text-sm">Avg Files/Commit</span>
                </div>
                <div className="text-2xl font-bold">
                  {commits.reduce((sum, c) => sum + (c.diff_stats?.files_changed || 0), 0) / commits.length || 0}
                </div>
              </div>
              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 text-muted-foreground mb-1">
                  <Activity className="w-4 h-4" />
                  <span className="text-sm">Recent Activity</span>
                </div>
                <div className="text-sm font-medium">
                  {commits[0] ? new Date(commits[0].timestamp).toLocaleDateString() : 'N/A'}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
