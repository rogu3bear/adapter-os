import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Checkbox } from './ui/checkbox';
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
  Eye,
  Download,
  Upload
} from 'lucide-react';
import apiClient from '../api/client';
import { Repository, Commit, TrainingConfig } from '../api/types';
import { logger } from '../utils/logger';
import { toast } from 'sonner';

interface CodeIntelligenceTrainingProps {
  onConfigSelect: (config: Partial<TrainingConfig>) => void;
  initialConfig?: Partial<TrainingConfig>;
}

export function CodeIntelligenceTraining({ onConfigSelect, initialConfig }: CodeIntelligenceTrainingProps) {
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
    ...initialConfig
  });

  // Mock data for demonstration
  const mockRepositories: Repository[] = [
    {
      id: 'acme/payments',
      url: 'https://github.com/acme/payments',
      branch: 'main',
      last_scan: '2024-02-15T10:30:00Z',
      commit_count: 1247
    },
    {
      id: 'acme/frontend',
      url: 'https://github.com/acme/frontend',
      branch: 'main',
      last_scan: '2024-02-14T15:20:00Z',
      commit_count: 892
    },
    {
      id: 'acme/api',
      url: 'https://github.com/acme/api',
      branch: 'main',
      last_scan: '2024-02-13T09:45:00Z',
      commit_count: 2156
    }
  ];

  const mockCommits: Commit[] = [
    {
      sha: 'abc123def456',
      message: 'feat: add payment processing with Stripe integration',
      author: 'john.doe@acme.com',
      timestamp: '2024-02-15T10:30:00Z',
      diff_stats: {
        files_changed: 12,
        insertions: 245,
        deletions: 18
      }
    },
    {
      sha: 'def456ghi789',
      message: 'fix: resolve validation issues in payment form',
      author: 'jane.smith@acme.com',
      timestamp: '2024-02-14T15:20:00Z',
      diff_stats: {
        files_changed: 8,
        insertions: 67,
        deletions: 23
      }
    },
    {
      sha: 'ghi789jkl012',
      message: 'refactor: improve error handling in payment service',
      author: 'bob.wilson@acme.com',
      timestamp: '2024-02-13T09:45:00Z',
      diff_stats: {
        files_changed: 15,
        insertions: 189,
        deletions: 45
      }
    }
  ];

  useEffect(() => {
    const fetchData = async () => {
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
    setConfig({
      ...config,
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
    setConfig({
      ...config,
      commit_sha: commitSha,
      scope: 'commit'
    });
  };

  const handleCategoryChange = (category: string) => {
    const categoryConfigs = {
      code: { rank: 16, alpha: 32, category: 'code', scope: 'global' },
      framework: { rank: 12, alpha: 24, category: 'framework', scope: 'global' },
      codebase: { rank: 24, alpha: 48, category: 'codebase', scope: 'repo' },
      ephemeral: { rank: 8, alpha: 16, category: 'ephemeral', scope: 'commit' }
    };
    
    setConfig({
      ...config,
      ...categoryConfigs[category as keyof typeof categoryConfigs]
    });
  };

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      case 'framework': return <Layers className="h-4 w-4" />;
      case 'codebase': return <GitBranch className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Code className="h-4 w-4" />;
    }
  };

  const getCategoryDescription = (category: string) => {
    switch (category) {
      case 'code': return 'General-purpose coding knowledge across languages and patterns';
      case 'framework': return 'Framework-specific APIs, idioms, and conventions';
      case 'codebase': return 'Repository-specific knowledge and internal patterns';
      case 'ephemeral': return 'Temporary adapter for specific commits or experiments';
      default: return '';
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
          Train adapters using your codebase's intelligence data. Select repositories and commits to create targeted adapters.
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
                            {repo.last_scan ? new Date(repo.last_scan).toLocaleDateString() : 'Never scanned'}
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
                            {commit.sha.substring(0, 8)} • {commit.author} • {new Date(commit.timestamp).toLocaleString()}
                          </div>
                          {commit.diff_stats && (
                            <div className="flex items-center space-x-4 mt-2 text-xs text-muted-foreground">
                              <span>{commit.diff_stats.files_changed} files</span>
                              <span className="text-green-600">+{commit.diff_stats.insertions}</span>
                              <span className="text-red-600">-{commit.diff_stats.deletions}</span>
                            </div>
                          )}
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
                <Select value={config.category} onValueChange={handleCategoryChange}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select category" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="code">
                      <div className="flex items-center space-x-2">
                        <Code className="h-4 w-4" />
                        <span>General Code</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="framework">
                      <div className="flex items-center space-x-2">
                        <Layers className="h-4 w-4" />
                        <span>Framework Specific</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="codebase">
                      <div className="flex items-center space-x-2">
                        <GitBranch className="h-4 w-4" />
                        <span>Codebase Specific</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="ephemeral">
                      <div className="flex items-center space-x-2">
                        <Clock className="h-4 w-4" />
                        <span>Ephemeral</span>
                      </div>
                    </SelectItem>
                  </SelectContent>
                </Select>
                <p className="text-sm text-muted-foreground mt-1">
                  {getCategoryDescription(config.category || 'code')}
                </p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="rank">Rank</Label>
                  <Input 
                    id="rank" 
                    type="number" 
                    value={config.rank} 
                    onChange={(e) => setConfig({...config, rank: parseInt(e.target.value)})}
                  />
                </div>
                <div>
                  <Label htmlFor="alpha">Alpha</Label>
                  <Input 
                    id="alpha" 
                    type="number" 
                    value={config.alpha} 
                    onChange={(e) => setConfig({...config, alpha: parseInt(e.target.value)})}
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="epochs">Epochs</Label>
                  <Input 
                    id="epochs" 
                    type="number" 
                    value={config.epochs} 
                    onChange={(e) => setConfig({...config, epochs: parseInt(e.target.value)})}
                  />
                </div>
                <div>
                  <Label htmlFor="learning_rate">Learning Rate</Label>
                  <Input 
                    id="learning_rate" 
                    type="number" 
                    step="0.0001"
                    value={config.learning_rate} 
                    onChange={(e) => setConfig({...config, learning_rate: parseFloat(e.target.value)})}
                  />
                </div>
              </div>

              <div>
                <Label htmlFor="batch_size">Batch Size</Label>
                <Input 
                  id="batch_size" 
                  type="number" 
                  value={config.batch_size} 
                  onChange={(e) => setConfig({...config, batch_size: parseInt(e.target.value)})}
                />
              </div>

              <div>
                <Label>Target Modules</Label>
                <div className="grid grid-cols-2 gap-2 mt-2">
                  {['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'].map((module) => (
                    <div key={module} className="flex items-center space-x-2">
                      <Checkbox 
                        id={module}
                        checked={config.targets?.includes(module) || false}
                        onCheckedChange={(checked) => {
                          const targets = config.targets || [];
                          if (checked) {
                            setConfig({...config, targets: [...targets, module]});
                          } else {
                            setConfig({...config, targets: targets.filter(t => t !== module)});
                          }
                        }}
                      />
                      <Label htmlFor={module} className="text-sm">{module}</Label>
                    </div>
                  ))}
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="preview" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Eye className="mr-2 h-5 w-5" />
                Training Preview
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                <div className="bg-gray-50 p-4 rounded-md">
                  <h4 className="font-medium mb-2">Training Configuration</h4>
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <div className="text-muted-foreground">Category</div>
                      <div className="flex items-center space-x-1">
                        {getCategoryIcon(config.category || 'code')}
                        <span className="font-medium">{config.category}</span>
                      </div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Scope</div>
                      <div className="font-medium">{config.scope}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Rank</div>
                      <div className="font-medium">{config.rank}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Alpha</div>
                      <div className="font-medium">{config.alpha}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Epochs</div>
                      <div className="font-medium">{config.epochs}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Learning Rate</div>
                      <div className="font-medium">{config.learning_rate}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Batch Size</div>
                      <div className="font-medium">{config.batch_size}</div>
                    </div>
                    <div>
                      <div className="text-muted-foreground">Target Modules</div>
                      <div className="font-medium">{config.targets?.length || 0} selected</div>
                    </div>
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
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
