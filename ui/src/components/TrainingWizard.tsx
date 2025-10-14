import React, { useState, useEffect } from 'react';
import { Wizard, WizardStep } from './ui/wizard';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Checkbox } from './ui/checkbox';
import { Slider } from './ui/slider';
import { Alert, AlertDescription } from './ui/alert';
import { Code, Zap, GitBranch, Database, Clock, AlertTriangle, CheckCircle } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import {
  AdapterCategory,
  AdapterScope,
  TrainingConfig,
  TrainingTemplate,
  Repository,
} from '../api/types';

interface TrainingWizardProps {
  onComplete: (trainingJobId: string) => void;
  onCancel: () => void;
}

interface WizardState {
  // Step 1: Category
  category: AdapterCategory | null;
  
  // Step 2: Basic Info
  name: string;
  description: string;
  scope: AdapterScope;
  
  // Step 3: Data Source
  dataSourceType: 'repository' | 'template' | 'custom';
  repositoryId?: string;
  templateId?: string;
  customData?: string;
  
  // Step 4: Category-specific config
  // Code adapter
  language?: string;
  symbolTargets?: string[];
  // Framework adapter
  frameworkId?: string;
  frameworkVersion?: string;
  apiPatterns?: string[];
  // Codebase adapter
  repoScope?: string;
  filePatterns?: string[];
  excludePatterns?: string[];
  // Ephemeral adapter
  ttlSeconds?: number;
  contextWindow?: number;
  
  // Step 5: Training parameters
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learningRate: number;
  batchSize: number;
  warmupSteps?: number;
  maxSeqLength?: number;
}

const CATEGORY_ICONS = {
  code: Code,
  framework: Zap,
  codebase: GitBranch,
  ephemeral: Clock,
};

const CATEGORY_DESCRIPTIONS = {
  code: 'Language-specific adapters for syntax, idioms, and patterns',
  framework: 'Framework-specific adapters for APIs and best practices',
  codebase: 'Repository-specific adapters trained on your codebase',
  ephemeral: 'Short-lived adapters for specific tasks or contexts',
};

const LANGUAGES = [
  'TypeScript', 'JavaScript', 'Python', 'Rust', 'Go', 'Java', 'C++', 'C#', 'Ruby', 'PHP',
];

const LORA_TARGETS = [
  'q_proj', 'k_proj', 'v_proj', 'o_proj',
  'gate_proj', 'up_proj', 'down_proj',
  'embed_tokens', 'lm_head',
];

export function TrainingWizard({ onComplete, onCancel }: TrainingWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [templates, setTemplates] = useState<TrainingTemplate[]>([]);
  
  const [state, setState] = useState<WizardState>({
    category: null,
    name: '',
    description: '',
    scope: 'global',
    dataSourceType: 'template',
    rank: 8,
    alpha: 16,
    targets: ['q_proj', 'v_proj'],
    epochs: 3,
    learningRate: 3e-4,
    batchSize: 4,
  });

  useEffect(() => {
    // Load repositories and templates
    const loadData = async () => {
      try {
        const [reposData, templatesData] = await Promise.all([
          apiClient.listRepositories(),
          apiClient.listTrainingTemplates(),
        ]);
        setRepositories(reposData);
        setTemplates(templatesData);
      } catch (error) {
        console.error('Failed to load data:', error);
        toast.error('Failed to load repositories and templates');
      }
    };
    loadData();
  }, []);

  const updateState = (updates: Partial<WizardState>) => {
    setState((prev) => ({ ...prev, ...updates }));
  };

  // Step 1: Category Selection
  const CategoryStep = () => (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Select the type of adapter you want to train. Each category has specific configuration options.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {(['code', 'framework', 'codebase', 'ephemeral'] as AdapterCategory[]).map((cat) => {
          const Icon = CATEGORY_ICONS[cat];
          const isSelected = state.category === cat;
          return (
            <Card
              key={cat}
              className={`cursor-pointer transition-all hover:border-primary ${
                isSelected ? 'border-primary bg-primary/5' : ''
              }`}
              onClick={() => updateState({ category: cat })}
            >
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Icon className="h-5 w-5" />
                  <span className="capitalize">{cat} Adapter</span>
                  {isSelected && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
                </CardTitle>
                <CardDescription>{CATEGORY_DESCRIPTIONS[cat]}</CardDescription>
              </CardHeader>
            </Card>
          );
        })}
      </div>
      {!state.category && (
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>Please select an adapter category to continue</AlertDescription>
        </Alert>
      )}
    </div>
  );

  // Step 2: Basic Information
  const BasicInfoStep = () => (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="name">Adapter Name</Label>
        <Input
          id="name"
          placeholder="my-awesome-adapter"
          value={state.name}
          onChange={(e) => updateState({ name: e.target.value })}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="description">Description</Label>
        <Textarea
          id="description"
          placeholder="Describe the purpose and use case for this adapter..."
          value={state.description}
          onChange={(e) => updateState({ description: e.target.value })}
          rows={3}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="scope">Scope</Label>
        <Select value={state.scope} onValueChange={(value: AdapterScope) => updateState({ scope: value })}>
          <SelectTrigger id="scope">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="global">Global - Available to all tenants</SelectItem>
            <SelectItem value="tenant">Tenant - Isolated to this tenant</SelectItem>
            <SelectItem value="repo">Repository - Scoped to a specific repository</SelectItem>
            <SelectItem value="commit">Commit - Scoped to a specific commit</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {!state.name && (
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>Adapter name is required</AlertDescription>
        </Alert>
      )}
    </div>
  );

  // Step 3: Data Source Selection
  const DataSourceStep = () => (
    <div className="space-y-4">
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card
          className={`cursor-pointer transition-all ${
            state.dataSourceType === 'template' ? 'border-primary bg-primary/5' : ''
          }`}
          onClick={() => updateState({ dataSourceType: 'template' })}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Database className="h-5 w-5" />
              Template
              {state.dataSourceType === 'template' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Use a pre-configured training template</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`cursor-pointer transition-all ${
            state.dataSourceType === 'repository' ? 'border-primary bg-primary/5' : ''
          }`}
          onClick={() => updateState({ dataSourceType: 'repository' })}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <GitBranch className="h-5 w-5" />
              Repository
              {state.dataSourceType === 'repository' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Train from a registered repository</CardDescription>
          </CardHeader>
        </Card>

        <Card
          className={`cursor-pointer transition-all ${
            state.dataSourceType === 'custom' ? 'border-primary bg-primary/5' : ''
          }`}
          onClick={() => updateState({ dataSourceType: 'custom' })}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Code className="h-5 w-5" />
              Custom
              {state.dataSourceType === 'custom' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Provide custom training data</CardDescription>
          </CardHeader>
        </Card>
      </div>

      {state.dataSourceType === 'template' && (
        <div className="space-y-2">
          <Label htmlFor="template">Select Template</Label>
          <Select value={state.templateId} onValueChange={(value) => updateState({ templateId: value })}>
            <SelectTrigger id="template">
              <SelectValue placeholder="Choose a template..." />
            </SelectTrigger>
            <SelectContent>
              {templates.map((template) => (
                <SelectItem key={template.id} value={template.id}>
                  {template.name} - {template.description}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {state.dataSourceType === 'repository' && (
        <div className="space-y-2">
          <Label htmlFor="repository">Select Repository</Label>
          <Select value={state.repositoryId} onValueChange={(value) => updateState({ repositoryId: value })}>
            <SelectTrigger id="repository">
              <SelectValue placeholder="Choose a repository..." />
            </SelectTrigger>
            <SelectContent>
              {repositories.map((repo) => (
                <SelectItem key={repo.id} value={repo.id}>
                  {repo.url} ({repo.branch})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {state.dataSourceType === 'custom' && (
        <div className="space-y-2">
          <Label htmlFor="customData">Custom Training Data</Label>
          <Textarea
            id="customData"
            placeholder="Paste or enter your training data here..."
            value={state.customData || ''}
            onChange={(e) => updateState({ customData: e.target.value })}
            rows={10}
          />
        </div>
      )}
    </div>
  );

  // Step 4: Category-Specific Configuration
  const CategoryConfigStep = () => {
    if (!state.category) return <div>No category selected</div>;

    switch (state.category) {
      case 'code':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="language">Programming Language</Label>
              <Select
                value={state.language}
                onValueChange={(value) => updateState({ language: value })}
              >
                <SelectTrigger id="language">
                  <SelectValue placeholder="Select language..." />
                </SelectTrigger>
                <SelectContent>
                  {LANGUAGES.map((lang) => (
                    <SelectItem key={lang} value={lang.toLowerCase()}>
                      {lang}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label>Symbol Targets (Optional)</Label>
              <Input
                placeholder="Enter symbols to target, comma-separated"
                value={state.symbolTargets?.join(', ') || ''}
                onChange={(e) =>
                  updateState({
                    symbolTargets: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                  })
                }
              />
              <p className="text-xs text-muted-foreground">
                Specific functions, classes, or modules to focus training on
              </p>
            </div>
          </div>
        );

      case 'framework':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="frameworkId">Framework</Label>
              <Input
                id="frameworkId"
                placeholder="e.g., react, django, rails"
                value={state.frameworkId || ''}
                onChange={(e) => updateState({ frameworkId: e.target.value })}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="frameworkVersion">Version</Label>
              <Input
                id="frameworkVersion"
                placeholder="e.g., 18.0.0, 4.2, 7.0"
                value={state.frameworkVersion || ''}
                onChange={(e) => updateState({ frameworkVersion: e.target.value })}
              />
            </div>

            <div className="space-y-2">
              <Label>API Patterns (Optional)</Label>
              <Input
                placeholder="Enter API patterns, comma-separated"
                value={state.apiPatterns?.join(', ') || ''}
                onChange={(e) =>
                  updateState({
                    apiPatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                  })
                }
              />
            </div>
          </div>
        );

      case 'codebase':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="repoScope">Repository Scope</Label>
              <Input
                id="repoScope"
                placeholder="e.g., src/, lib/, entire repo"
                value={state.repoScope || ''}
                onChange={(e) => updateState({ repoScope: e.target.value })}
              />
            </div>

            <div className="space-y-2">
              <Label>File Patterns (Include)</Label>
              <Input
                placeholder="e.g., **/*.ts, **/*.tsx"
                value={state.filePatterns?.join(', ') || ''}
                onChange={(e) =>
                  updateState({
                    filePatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                  })
                }
              />
            </div>

            <div className="space-y-2">
              <Label>Exclude Patterns (Optional)</Label>
              <Input
                placeholder="e.g., **/node_modules/**, **/*.test.ts"
                value={state.excludePatterns?.join(', ') || ''}
                onChange={(e) =>
                  updateState({
                    excludePatterns: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                  })
                }
              />
            </div>
          </div>
        );

      case 'ephemeral':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="ttl">Time to Live (seconds)</Label>
              <Input
                id="ttl"
                type="number"
                placeholder="3600"
                value={state.ttlSeconds || ''}
                onChange={(e) => updateState({ ttlSeconds: parseInt(e.target.value) || undefined })}
              />
              <p className="text-xs text-muted-foreground">
                Adapter will be automatically evicted after this duration
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="contextWindow">Context Window (tokens)</Label>
              <Input
                id="contextWindow"
                type="number"
                placeholder="4096"
                value={state.contextWindow || ''}
                onChange={(e) => updateState({ contextWindow: parseInt(e.target.value) || undefined })}
              />
            </div>
          </div>
        );
    }
  };

  // Step 5: Training Parameters
  const TrainingParamsStep = () => (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="rank">Rank (r)</Label>
          <Input
            id="rank"
            type="number"
            value={state.rank}
            onChange={(e) => updateState({ rank: parseInt(e.target.value) || 8 })}
          />
          <p className="text-xs text-muted-foreground">LoRA rank dimension (typically 4-32)</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="alpha">Alpha</Label>
          <Input
            id="alpha"
            type="number"
            value={state.alpha}
            onChange={(e) => updateState({ alpha: parseInt(e.target.value) || 16 })}
          />
          <p className="text-xs text-muted-foreground">LoRA scaling factor (typically 2r)</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="epochs">Epochs</Label>
          <Input
            id="epochs"
            type="number"
            value={state.epochs}
            onChange={(e) => updateState({ epochs: parseInt(e.target.value) || 3 })}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="learningRate">Learning Rate</Label>
          <Input
            id="learningRate"
            type="number"
            step="0.0001"
            value={state.learningRate}
            onChange={(e) => updateState({ learningRate: parseFloat(e.target.value) || 3e-4 })}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="batchSize">Batch Size</Label>
          <Input
            id="batchSize"
            type="number"
            value={state.batchSize}
            onChange={(e) => updateState({ batchSize: parseInt(e.target.value) || 4 })}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="warmupSteps">Warmup Steps (Optional)</Label>
          <Input
            id="warmupSteps"
            type="number"
            placeholder="100"
            value={state.warmupSteps || ''}
            onChange={(e) => updateState({ warmupSteps: parseInt(e.target.value) || undefined })}
          />
        </div>
      </div>

      <div className="space-y-2">
        <Label>LoRA Target Modules</Label>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
          {LORA_TARGETS.map((target) => (
            <div key={target} className="flex items-center space-x-2">
              <Checkbox
                id={target}
                checked={state.targets.includes(target)}
                onCheckedChange={(checked) => {
                  if (checked) {
                    updateState({ targets: [...state.targets, target] });
                  } else {
                    updateState({ targets: state.targets.filter((t) => t !== target) });
                  }
                }}
              />
              <Label htmlFor={target} className="text-sm font-mono">
                {target}
              </Label>
            </div>
          ))}
        </div>
        <p className="text-xs text-muted-foreground mt-2">
          Selected: {state.targets.length} module{state.targets.length !== 1 ? 's' : ''}
        </p>
      </div>
    </div>
  );

  // Step 6: Review & Confirm
  const ReviewStep = () => (
    <div className="space-y-4">
      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Review your configuration before starting training. This process may take several hours depending on the dataset size and hardware.
        </AlertDescription>
      </Alert>

      <Card>
        <CardHeader>
          <CardTitle>Configuration Summary</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <p className="font-medium">Category</p>
              <p className="text-muted-foreground capitalize">{state.category}</p>
            </div>
            <div>
              <p className="font-medium">Name</p>
              <p className="text-muted-foreground">{state.name}</p>
            </div>
            <div>
              <p className="font-medium">Scope</p>
              <p className="text-muted-foreground capitalize">{state.scope}</p>
            </div>
            <div>
              <p className="font-medium">Data Source</p>
              <p className="text-muted-foreground capitalize">{state.dataSourceType}</p>
            </div>
            <div>
              <p className="font-medium">Rank</p>
              <p className="text-muted-foreground">{state.rank}</p>
            </div>
            <div>
              <p className="font-medium">Epochs</p>
              <p className="text-muted-foreground">{state.epochs}</p>
            </div>
            <div>
              <p className="font-medium">Learning Rate</p>
              <p className="text-muted-foreground">{state.learningRate}</p>
            </div>
            <div>
              <p className="font-medium">Batch Size</p>
              <p className="text-muted-foreground">{state.batchSize}</p>
            </div>
          </div>

          {state.category === 'code' && state.language && (
            <div>
              <p className="font-medium text-sm">Language</p>
              <Badge>{state.language}</Badge>
            </div>
          )}

          {state.category === 'framework' && state.frameworkId && (
            <div>
              <p className="font-medium text-sm">Framework</p>
              <Badge>{state.frameworkId} {state.frameworkVersion}</Badge>
            </div>
          )}

          <div>
            <p className="font-medium text-sm">LoRA Targets ({state.targets.length})</p>
            <div className="flex flex-wrap gap-1 mt-1">
              {state.targets.map((target) => (
                <Badge key={target} variant="outline">{target}</Badge>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );

  const handleComplete = async () => {
    setIsLoading(true);
    try {
      // Build training config
      const trainingConfig: TrainingConfig = {
        rank: state.rank,
        alpha: state.alpha,
        targets: state.targets,
        epochs: state.epochs,
        learning_rate: state.learningRate,
        batch_size: state.batchSize,
        warmup_steps: state.warmupSteps,
        max_seq_length: state.maxSeqLength,
      };

      // Start training
      const job = await apiClient.startTraining({
        adapter_name: state.name,
        config: trainingConfig,
        template_id: state.templateId,
        repo_id: state.repositoryId,
      });

      toast.success(`Training job ${job.id} started successfully!`);
      onComplete(job.id);
    } catch (error) {
      console.error('Training failed:', error);
      toast.error(error instanceof Error ? error.message : 'Failed to start training');
    } finally {
      setIsLoading(false);
    }
  };

  const steps: WizardStep[] = [
    {
      id: 'category',
      title: 'Category',
      description: 'Select adapter type',
      component: <CategoryStep />,
      validate: () => {
        if (!state.category) {
          toast.error('Please select an adapter category');
          return false;
        }
        return true;
      },
    },
    {
      id: 'basic-info',
      title: 'Basic Info',
      description: 'Name and scope',
      component: <BasicInfoStep />,
      validate: () => {
        if (!state.name.trim()) {
          toast.error('Adapter name is required');
          return false;
        }
        return true;
      },
    },
    {
      id: 'data-source',
      title: 'Data Source',
      description: 'Select training data',
      component: <DataSourceStep />,
      validate: () => {
        if (state.dataSourceType === 'template' && !state.templateId) {
          toast.error('Please select a template');
          return false;
        }
        if (state.dataSourceType === 'repository' && !state.repositoryId) {
          toast.error('Please select a repository');
          return false;
        }
        if (state.dataSourceType === 'custom' && !state.customData?.trim()) {
          toast.error('Please provide custom training data');
          return false;
        }
        return true;
      },
    },
    {
      id: 'category-config',
      title: 'Configuration',
      description: 'Category-specific settings',
      component: <CategoryConfigStep />,
    },
    {
      id: 'training-params',
      title: 'Training Parameters',
      description: 'LoRA configuration',
      component: <TrainingParamsStep />,
      validate: () => {
        if (state.targets.length === 0) {
          toast.error('Please select at least one LoRA target module');
          return false;
        }
        return true;
      },
    },
    {
      id: 'review',
      title: 'Review',
      description: 'Confirm and start',
      component: <ReviewStep />,
    },
  ];

  return (
    <Wizard
      title="Training Wizard"
      steps={steps}
      currentStep={currentStep}
      onStepChange={setCurrentStep}
      onComplete={handleComplete}
      onCancel={onCancel}
      completeButtonText="Start Training"
      isLoading={isLoading}
    />
  );
}


