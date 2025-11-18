import React, { useEffect, useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import {
  GitBranch,
  Code,
  Database,
  Target,
  FileText,
  Users,
  Clock,
  CheckCircle,
  AlertTriangle,
  Brain,
  Layers,
  Zap,
} from 'lucide-react';
import apiClient from '../api/client';
import { Repository, Commit, TrainingConfig } from '../api/types';
import { logger } from '../utils/logger';
import { toast } from 'sonner';

interface CodeIntelligenceTrainingProps {
  tenantId: string;
  userId: string;
  onTrainingStarted?: (sessionId: string) => void;
  onCancel?: () => void;
  initialConfig?: Partial<TrainingConfig>;
}

type CategoryConfig = Partial<TrainingConfig> & {
  category: AdapterCategory;
  scope: AdapterScope;
};

const CATEGORY_OPTIONS: Array<{
  value: AdapterCategory;
  label: string;
  icon: React.ReactNode;
  description: string;
}> = [
  {
    value: 'code',
    label: 'General Code',
    icon: <Code className="h-4 w-4" />,
    description: 'General-purpose coding knowledge across languages and patterns',
  },
  {
    value: 'framework',
    label: 'Framework Specific',
    icon: <Layers className="h-4 w-4" />,
    description: 'Framework-specific APIs, idioms, and conventions',
  },
  {
    value: 'codebase',
    label: 'Codebase Specific',
    icon: <GitBranch className="h-4 w-4" />,
    description: 'Repository-specific knowledge and internal patterns',
  },
  {
    value: 'ephemeral',
    label: 'Ephemeral',
    icon: <Clock className="h-4 w-4" />,
    description: 'Temporary adapters for specific commits or experiments',
  },
];

export function CodeIntelligenceTraining({
  tenantId,
  userId,
  onTrainingStarted,
  onCancel,
  initialConfig,
}: CodeIntelligenceTrainingProps) {
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [commits, setCommits] = useState<Commit[]>([]);
  const [selectedRepo, setSelectedRepo] = useState<string>('');
  const [selectedCommit, setSelectedCommit] = useState<string>('');
  const [loading, setLoading] = useState(true);
  const [config, setConfig] = useState<Partial<TrainingConfig>>({
    category: 'codebase',
    scope: 'repo',
    rank: 24,
    alpha: 48,
    epochs: 3,
    learning_rate: 0.001,
    batch_size: 32,
    targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'],
    ...initialConfig,
  });

  const categoryDefaults: Record<AdapterCategory, CategoryConfig> = useMemo(
    () => ({
      code: { rank: 16, alpha: 32, category: 'code', scope: 'global' },
      framework: { rank: 12, alpha: 24, category: 'framework', scope: 'global' },
      codebase: { rank: 24, alpha: 48, category: 'codebase', scope: 'repo' },
      ephemeral: { rank: 8, alpha: 16, category: 'ephemeral', scope: 'commit' },
    }),
    [],
  );

  useEffect(() => {
    const fetchData = async () => {
      setLoading(true);
      try {
        logger.info('Fetching repositories and commits', {
          component: 'CodeIntelligenceTraining',
          operation: 'fetchData'
        });

        // Fetch repositories from API
        const repos = await apiClient.listRepositories();
        setRepositories(repos);

        // If we have repositories, fetch commits for the first one
        if (repos.length > 0 && selectedRepo) {
          const commits = await apiClient.listCommits(selectedRepo);
          setCommits(commits);
        }

        logger.info('Successfully loaded repositories and commits', {
          component: 'CodeIntelligenceTraining',
          operation: 'fetchData',
          repositoryCount: repos.length
        });
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Failed to fetch repositories';
        logger.error('Failed to fetch repositories', {
          component: 'CodeIntelligenceTraining',
          operation: 'fetchData',
          error: errorMessage
        });
        toast.error(`Failed to load repositories: ${errorMessage}`);
        // Fallback to mock data for demonstration
        setRepositories(mockRepositories);
        setCommits(mockCommits);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, [selectedRepo]);

  useEffect(() => {
    onConfigSelect(config);
  }, [config, onConfigSelect]);

  const handleRepoSelect = async (repoId: string) => {
    setSelectedRepo(repoId);
    setConfig((prev) => ({
      ...prev,
      repo_id: repoId,
      scope: 'repo'
    });

    // Fetch commits for selected repository
    try {
      logger.info('Fetching commits for repository', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleRepoSelect',
        repoId
      });

      const commits = await apiClient.listCommits(repoId);
      setCommits(commits);

      logger.info('Successfully loaded commits', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleRepoSelect',
        repoId,
        commitCount: commits.length
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch commits';
      logger.error('Failed to fetch commits', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleRepoSelect',
        repoId,
        error: errorMessage
      });
      toast.error(`Failed to load commits: ${errorMessage}`);
      // Keep mock commits as fallback
    }
  };

  const handleCommitSelect = (commitSha: string) => {
    setSelectedCommit(commitSha);
    setConfig((prev) => ({
      ...prev,
      commit_sha: commitSha,
      scope: 'commit',
    }));
  };

  const handleCategoryChange = (value: AdapterCategory) => {
    setConfig((prev) => ({
      ...prev,
      ...categoryDefaults[value],
    }));
  };

  const handleStartTraining = async () => {
    if (!selectedRepo) {
      toast.error('Please select a repository first');
      return;
    }

    const category = config.category ?? 'codebase';
    const scope = config.scope ?? 'repo';

    try {
      logger.info('Starting adapter training', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleStartTraining',
        tenantId,
        userId,
        selectedRepo,
        selectedCommit,
      });

      const result = await apiClient.startAdapterTraining({
        repository_path: selectedRepo,
        adapter_name: `${category}_${selectedRepo.replace(/[^a-zA-Z0-9]/g, '_')}`,
        description: `Adapter trained on ${selectedRepo}${
          selectedCommit ? ` at commit ${selectedCommit.substring(0, 8)}` : ''
        }`,
        training_config: {
          rank: config.rank ?? categoryDefaults[category].rank ?? 24,
          alpha: config.alpha ?? categoryDefaults[category].alpha ?? 48,
          epochs: config.epochs ?? 3,
          learning_rate: config.learning_rate ?? 0.001,
          batch_size: config.batch_size ?? 32,
          targets: config.targets ?? ['q_proj', 'k_proj', 'v_proj', 'o_proj'],
          category,
          scope,
          ...(config.repo_id && { repo_id: config.repo_id }),
          ...(config.commit_sha && { commit_sha: config.commit_sha }),
          ...(config.framework_id && { framework_id: config.framework_id }),
          ...(config.framework_version && { framework_version: config.framework_version }),
        },
        tenant_id: tenantId,
      });

      toast.success(`Training started successfully (session ${result.session_id})`);
      
      // Emit activity event (tenant_id/user_id auto-extracted from JWT)
      try {
        await apiClient.createActivityEvent({
          event_type: ACTIVITY_EVENT_TYPES.TRAINING_SESSION_STARTED,
          target_type: 'training_session',
          target_id: result.session_id,
          metadata_json: JSON.stringify({
            session_id: result.session_id,
            repo_id: selectedRepo,
            adapter_name: `${category}_${selectedRepo.replace(/[^a-zA-Z0-9]/g, '_')}`,
            category,
            scope,
          }),
        });
      } catch (activityErr) {
        // Non-blocking: log but don't fail the training start
        logger.warn('Failed to emit training start activity event', {
          component: 'CodeIntelligenceTraining',
          operation: 'handleStartTraining',
          error: activityErr instanceof Error ? activityErr.message : String(activityErr),
        });
      }
      
      onTrainingStarted?.(result.session_id);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to start training';
      logger.error('Failed to start training', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleStartTraining',
        tenantId,
        userId,
        error: message,
      }, toError(error));
      toast.error(`Failed to start training: ${message}`);
    }
  };

  const handleStartTraining = async () => {
    if (!selectedRepo) {
      toast.error('Please select a repository first');
      return;
    }

    try {
      logger.info('Starting adapter training', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleStartTraining',
        config
      });

      const result = await apiClient.startAdapterTraining({
        repository_path: selectedRepo,
        adapter_name: `${config.category}_${selectedRepo.replace(/[^a-zA-Z0-9]/g, '_')}`,
        description: `Adapter trained on ${selectedRepo}${selectedCommit ? ` at commit ${selectedCommit.substring(0, 8)}` : ''}`,
        training_config: {
          rank: config.rank || 24,
          alpha: config.alpha || 48,
          epochs: config.epochs || 3,
          learning_rate: config.learning_rate || 0.001,
          batch_size: config.batch_size || 32,
          category: config.category || 'codebase',
          scope: config.scope || 'repo',
          ...(config.repo_id && { repo_id: config.repo_id }),
          ...(config.commit_sha && { commit_sha: config.commit_sha }),
          ...(config.framework_id && { framework_id: config.framework_id }),
        },
        tenant_id: 'default' // TODO: Get from context
      });

      toast.success(`Training started successfully! Session ID: ${result.session_id}`);
      logger.info('Training started successfully', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleStartTraining',
        sessionId: result.session_id
      });

      // Navigate to training monitor or show success message
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to start training';
      logger.error('Failed to start training', {
        component: 'CodeIntelligenceTraining',
        operation: 'handleStartTraining',
        error: errorMessage,
        config
      });
      toast.error(`Failed to start training: ${errorMessage}`);
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading repositories...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center space-x-2">
        <Brain className="h-6 w-6 text-primary" />
        <h2 className="text-xl font-semibold">Code Intelligence Training</h2>
      </div>

      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Train adapters using your codebase intelligence data. Select repositories and commits to create targeted adapters.
        </AlertDescription>
      </Alert>

      <Tabs defaultValue="repositories" className="space-y-4">
        <TabsList>
          <TabsTrigger value="repositories">Repositories</TabsTrigger>
          <TabsTrigger value="commits">Commits</TabsTrigger>
          <TabsTrigger value="configuration">Configuration</TabsTrigger>
          <TabsTrigger value="preview">Preview</TabsTrigger>
        </TabsList>

        <TabsContent value="repositories" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Database className="mr-2 h-5 w-5" />
                Available Repositories
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {repositories.map((repo) => (
                  <Card
                    key={repo.id}
                    className={`cursor-pointer transition-colors ${
                      selectedRepo === repo.id ? 'ring-2 ring-primary' : 'hover:bg-gray-50'
                    }`}
                    onClick={() => handleRepoSelect(repo.id)}
                  >
                    <CardContent className="pt-4">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center space-x-3">
                          <GitBranch className="h-5 w-5 text-muted-foreground" />
                          <div>
                            <div className="font-medium">{repo.id}</div>
                            <div className="text-sm text-muted-foreground">
                              {repo.commit_count} commits • {repo.branch} branch
                            </div>
                          </div>
                        </div>
                        <div className="flex items-center space-x-2">
                          <Badge variant="outline">
                            {repo.last_scan
                              ? new Date(repo.last_scan).toLocaleDateString()
                              : 'Never scanned'}
                          </Badge>
                          {selectedRepo === repo.id && (
                            <CheckCircle className="h-4 w-4 text-primary" />
                          )}
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="commits" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <FileText className="mr-2 h-5 w-5" />
                Recent Commits
                {selectedRepo && (
                  <Badge variant="outline" className="ml-2">
                    {selectedRepo}
                  </Badge>
                )}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {commits.map((commit) => (
                  <Card
                    key={commit.sha}
                    className={`cursor-pointer transition-colors ${
                      selectedCommit === commit.sha ? 'ring-2 ring-primary' : 'hover:bg-gray-50'
                    }`}
                    onClick={() => handleCommitSelect(commit.sha)}
                  >
                    <CardContent className="pt-4">
                      <div className="flex items-center justify-between">
                        <div className="flex-1">
                          <div className="font-medium text-sm">{commit.message}</div>
                          <div className="text-xs text-muted-foreground mt-1">
                            {commit.sha.substring(0, 8)} • {commit.author} •{' '}
                            {new Date(commit.timestamp).toLocaleString()}
                          </div>
                        </div>
                        {selectedCommit === commit.sha && (
                          <CheckCircle className="h-4 w-4 text-primary" />
                        )}
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="configuration" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Target className="mr-2 h-5 w-5" />
                Training Configuration
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
              <div>
                <Label htmlFor="category">Adapter Category</Label>
                <Select
                  value={config.category}
                  onValueChange={(value) => handleCategoryChange(value as AdapterCategory)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select category" />
                  </SelectTrigger>
                  <SelectContent>
                    {CATEGORY_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        <div className="flex items-center space-x-2">
                          {option.icon}
                          <span>{option.label}</span>
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-sm text-muted-foreground mt-1">
                  {
                    CATEGORY_OPTIONS.find((option) => option.value === (config.category ?? 'codebase'))
                      ?.description
                  }
                </p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="rank">Rank</Label>
                  <Input
                    id="rank"
                    type="number"
                    value={config.rank ?? ''}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        rank: e.target.value ? parseInt(e.target.value, 10) : undefined,
                      }))
                    }
                  />
                </div>
                <div>
                  <Label htmlFor="alpha">Alpha</Label>
                  <Input
                    id="alpha"
                    type="number"
                    value={config.alpha ?? ''}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        alpha: e.target.value ? parseInt(e.target.value, 10) : undefined,
                      }))
                    }
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="epochs">Epochs</Label>
                  <Input
                    id="epochs"
                    type="number"
                    value={config.epochs ?? ''}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        epochs: e.target.value ? parseInt(e.target.value, 10) : undefined,
                      }))
                    }
                  />
                </div>
                <div>
                  <Label htmlFor="learning_rate">Learning Rate</Label>
                  <Input
                    id="learning_rate"
                    type="number"
                    step="0.0001"
                    value={config.learning_rate ?? ''}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        learning_rate: e.target.value ? parseFloat(e.target.value) : undefined,
                      }))
                    }
                  />
                </div>
              </div>

              <div>
                <Label htmlFor="batch_size">Batch Size</Label>
                <Input
                  id="batch_size"
                  type="number"
                  value={config.batch_size ?? ''}
                  onChange={(e) =>
                    setConfig((prev) => ({
                      ...prev,
                      batch_size: e.target.value ? parseInt(e.target.value, 10) : undefined,
                    }))
                  }
                />
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="preview" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Users className="mr-2 h-5 w-5" />
                Preview
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4 text-sm">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <div className="text-muted-foreground">Repository</div>
                  <div className="font-mono">{selectedRepo || 'Select a repository'}</div>
                </div>
                <div>
                  <div className="text-muted-foreground">Commit</div>
                  <div className="font-mono">
                    {selectedCommit ? selectedCommit.substring(0, 10) : 'Latest'}
                  </div>
                </div>

                {selectedRepo && (
                  <Alert>
                    <GitBranch className="h-4 w-4" />
                    <AlertDescription>
                      Training adapter for repository: <strong>{selectedRepo}</strong>
                      {selectedCommit && (
                        <> at commit: <strong>{selectedCommit.substring(0, 8)}</strong></>
                      )}
                    </AlertDescription>
                  </Alert>
                )}

                <div className="status-indicator status-info flex-between">
                  <div className="flex-center">
                    <Zap className="icon-standard" />
                    <span className="text-sm font-medium">
                      Estimated training time: 2-4 hours
                    </span>
                  </div>
                  <Button size="sm" onClick={handleStartTraining}>
                    Start Training
                  </Button>
                </div>
              </div>

              <Alert>
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  Training will consume GPU resources on the selected tenant ({tenantId}). Ensure
                  you have capacity before starting.
                </AlertDescription>
              </Alert>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button onClick={handleStartTraining} variant="success">
          <Zap className="h-4 w-4 mr-2" />
          Start Training
        </Button>
      </div>
    </div>
  );
}
