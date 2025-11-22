
// 【ui/src/components/TrainingWizard.tsx§1-981】 - Add density controls and breadcrumbs

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

import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { Code, Zap, GitBranch, Database, Clock, AlertTriangle, CheckCircle, FileText, Folder, Settings, RotateCcw, ChevronDown } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';
import { DensityProvider, useDensity } from '../contexts/DensityContext';
import { BreadcrumbNavigation } from './BreadcrumbNavigation';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { HelpTooltip } from './ui/help-tooltip';
import { useWizardPersistence } from '../hooks/useWizardPersistence';
import { useFormValidation } from '../hooks/useFormValidation';
import { TrainingConfigSchema, formatValidationError } from '../schemas';
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
  // Current step
  currentStep?: number;

  // Step 1: Category
  category: AdapterCategory | null;

  // Step 2: Basic Info
  name: string;
  description: string;
  scope: AdapterScope;

  // Step 3: Data Source
  dataSourceType: 'repository' | 'template' | 'custom' | 'directory';
  repositoryId?: string;
  templateId?: string;
  customData?: string;
  datasetPath?: string;
  directoryRoot?: string;
  directoryPath?: string;
  
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

  // Step 6: Packaging & Registration
  packageAfter?: boolean;
  registerAfter?: boolean;
  adaptersRoot?: string;
  adapterId?: string;
  tier?: string;
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


// Inner component that uses density context
function TrainingWizardInner({ onComplete, onCancel }: TrainingWizardProps): JSX.Element {
  const { density, setDensity, spacing, textSizes } = useDensity();
  const [isLoading, setIsLoading] = useState(false);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [templates, setTemplates] = useState<TrainingTemplate[]>([]);
  const [wizardError, setWizardError] = useState<Error | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [showResumeDialog, setShowResumeDialog] = useState(false);
  const [savedState, setSavedState] = useState<WizardState | null>(null);

  const initialState: WizardState = {
    currentStep: 0,
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
    packageAfter: true,
    registerAfter: true,
    adaptersRoot: './adapters',
    tier: 'warm',
  };

  const {
    state,
    setState: setPersistedState,
    clearState: clearPersistedState,
    hasSavedState,
    loadSavedState,
  } = useWizardPersistence<WizardState>({
    storageKey: 'training-wizard',
    initialState,
    onSavedStateDetected: (saved) => {
      setSavedState(saved);
      setShowResumeDialog(true);
    },
  });

  const [currentStep, setCurrentStep] = useState(state.currentStep || 0);

  // Sync currentStep with state persistence
  useEffect(() => {
    setPersistedState({ currentStep });
  }, [currentStep, setPersistedState]);

  useEffect(() => {
    // Load repositories and templates
    const loadData = async () => {
      try {

        setWizardError(null);

        const [reposData, templatesData] = await Promise.all([
          apiClient.listRepositories(),
          apiClient.listTrainingTemplates(),
        ]);
        setRepositories(reposData);
        setTemplates(templatesData);
      } catch (error) {

        const err = error instanceof Error ? error : new Error('Failed to load repositories and templates');
        setWizardError(err);
        logger.error('Failed to preload training wizard data', {
          component: 'TrainingWizard',
          operation: 'loadData',
        }, toError(error));

        console.error('Failed to load data:', error);
        toast.error('Failed to load repositories and templates');
      }
    };
    loadData();
  }, []);


  const handleResume = () => {
    // loadSavedState already updates the persisted state
    const restoredState = loadSavedState();
    if (restoredState && restoredState.currentStep !== undefined) {
      setCurrentStep(restoredState.currentStep);
    }
    setShowResumeDialog(false);
  };

  const handleStartFresh = () => {
    clearPersistedState();
    // Reset to initial state
    setPersistedState(initialState);
    setCurrentStep(0);
    setShowResumeDialog(false);
  };

  const updateState = (updates: Partial<WizardState>) => {
    setPersistedState(updates);
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
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
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

        <Card
          className={`cursor-pointer transition-all ${
            state.dataSourceType === 'directory' ? 'border-primary bg-primary/5' : ''
          }`}
          onClick={() => updateState({ dataSourceType: 'directory' })}
        >
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Folder className="h-5 w-5" />
              Directory
              {state.dataSourceType === 'directory' && <CheckCircle className="h-4 w-4 text-primary ml-auto" />}
            </CardTitle>
            <CardDescription>Train from a directory on the system</CardDescription>
          </CardHeader>
        </Card>
      </div>

      {state.dataSourceType === 'directory' && (
        <div className="space-y-4 pt-4 border-t">
          <Alert>
            <CheckCircle className="h-4 w-4" />
            <AlertDescription>
              Directory-based training uses the codegraph analyzer to automatically build training examples from your code directory.
              No pre-tokenized JSON required!
            </AlertDescription>
          </Alert>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="directoryRoot">Directory Root (Absolute Path)</Label>
              <Input
                id="directoryRoot"
                placeholder="/absolute/path/to/repo"
                value={state.directoryRoot || ''}
                onChange={(e) => updateState({ directoryRoot: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">
                Absolute path to the repository root directory. Required for directory-based training.
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="directoryPath">Subdirectory (Optional)</Label>
              <Input
                id="directoryPath"
                placeholder="src or . (for entire repo)"
                value={state.directoryPath || ''}
                onChange={(e) => updateState({ directoryPath: e.target.value })}
              />
              <p className="text-xs text-muted-foreground">
                Relative path under root. Defaults to "." (entire repository).
              </p>
            </div>
          </div>
          {state.directoryRoot && (
            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm font-medium">Training Path:</p>
              <p className="text-xs text-muted-foreground font-mono">
                {state.directoryRoot}
                {state.directoryPath ? `/${state.directoryPath}` : ''}
              </p>
            </div>
          )}
        </div>
      )}

      {state.dataSourceType === 'template' && (
        <div className="space-y-2">
          <Label htmlFor="template">Select Template</Label>
          <Select value={state.templateId} onValueChange={(value) => updateState({ templateId: value })}>
            <SelectTrigger id="template">
              <SelectValue placeholder="Choose a template..." />
            </SelectTrigger>
            <SelectContent>
              {templates.filter(template => template.id && template.id !== '').map((template) => (
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
              {repositories.filter(repo => repo.id && repo.id !== '').map((repo) => (
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

      <div className="space-y-2">
        <Label htmlFor="datasetPath">Dataset Path (optional)</Label>
        <Input
          id="datasetPath"
          placeholder="e.g., data/code_to_db_training.json"
          value={state.datasetPath || ''}
          onChange={(e) => updateState({ datasetPath: e.target.value })}
        />
        <p className="text-xs text-muted-foreground">If provided, the orchestrator will load examples from this JSON file.</p>
      </div>
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
              <div className="flex items-center gap-1">
                <Label htmlFor="contextWindow">Context Window (tokens)</Label>
                <HelpTooltip content="Maximum input length. 4096 tokens = ~3000 words. Longer = more context but more memory." />
              </div>
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
          <div className="flex items-center gap-1">
            <Label htmlFor="rank">Rank (r)</Label>
            <HelpTooltip content="Controls capacity of learned patterns. Higher = more expressive but slower. Start with 8-16 for most tasks." />
          </div>
          <Input
            id="rank"
            type="number"
            value={state.rank}
            onChange={(e) => updateState({ rank: parseInt(e.target.value) || 8 })}
          />
          <p className="text-xs text-muted-foreground">LoRA rank dimension (typically 4-32)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="alpha">Alpha</Label>
            <HelpTooltip content="Controls how strongly adapter influences model. Usually keep at 2x your Rank value." />
          </div>
          <Input
            id="alpha"
            type="number"
            value={state.alpha}
            onChange={(e) => updateState({ alpha: parseInt(e.target.value) || 16 })}
          />
          <p className="text-xs text-muted-foreground">LoRA scaling factor (typically 2r)</p>
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="epochs">Epochs</Label>
            <HelpTooltip content="Number of times to repeat training data. More = better learning but risk of overfitting. Start with 3-5." />
          </div>
          <Input
            id="epochs"
            type="number"
            value={state.epochs}
            onChange={(e) => updateState({ epochs: parseInt(e.target.value) || 3 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="learningRate">Learning Rate</Label>
            <HelpTooltip content="How fast model learns. Too high = unstable, too low = slow. Default 0.0003 is safe for most cases." />
          </div>
          <Input
            id="learningRate"
            type="number"
            step="0.0001"
            value={state.learningRate}
            onChange={(e) => updateState({ learningRate: parseFloat(e.target.value) || 3e-4 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="batchSize">Batch Size</Label>
            <HelpTooltip content="Number of examples processed together. Larger = faster but needs more memory. Default 4 is conservative." />
          </div>
          <Input
            id="batchSize"
            type="number"
            value={state.batchSize}
            onChange={(e) => updateState({ batchSize: parseInt(e.target.value) || 4 })}
          />
        </div>

        <div className="space-y-2">
          <div className="flex items-center gap-1">
            <Label htmlFor="warmupSteps">Warmup Steps (Optional)</Label>
            <HelpTooltip content="Gradually increase learning rate at start to stabilize training. Optional; helps with some datasets." />
          </div>
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


  // Step 6: Packaging & Registration
  const PackagingStep = () => (
    <div className="space-y-4">
      <div className="flex items-center space-x-2">
        <Checkbox
          id="packageAfter"
          checked={!!state.packageAfter}
          onCheckedChange={(checked) => updateState({ packageAfter: !!checked })}
        />
        <Label htmlFor="packageAfter">Package adapter after training</Label>
      </div>

      <div className="flex items-center space-x-2">
        <Checkbox
          id="registerAfter"
          checked={!!state.registerAfter}
          onCheckedChange={(checked) => updateState({ registerAfter: !!checked })}
        />
        <Label htmlFor="registerAfter">Register adapter after packaging</Label>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="adaptersRoot">Adapters Root</Label>
          <Input
            id="adaptersRoot"
            placeholder="./adapters"
            value={state.adaptersRoot || ''}
            onChange={(e) => updateState({ adaptersRoot: e.target.value })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="adapterId">Adapter ID (optional)</Label>
          <Input
            id="adapterId"
            placeholder="my-awesome-adapter"
            value={state.adapterId || ''}
            onChange={(e) => updateState({ adapterId: e.target.value })}
          />
        </div>
      </div>

      <div className="space-y-2">
        <Label htmlFor="tier">Tier</Label>
        <Select value={state.tier || 'warm'} onValueChange={(value) => updateState({ tier: value })}>
          <SelectTrigger id="tier">
            <SelectValue placeholder="Select tier" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="persistent">Persistent</SelectItem>
            <SelectItem value="warm">Warm</SelectItem>
            <SelectItem value="ephemeral">Ephemeral</SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">Tier used for registration (persistent, warm, or ephemeral)</p>
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


      <Accordion type="multiple" defaultValue={['basic']} className="w-full">
        <AccordionItem value="basic">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <FileText className="h-4 w-4" />
              Basic Information
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="grid grid-cols-2 gap-4 text-sm pt-2">
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
                <p className="font-medium">Description</p>
                <p className="text-muted-foreground">{state.description || 'No description'}</p>
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="data-source">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Database className="h-4 w-4" />
              Data Source
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              <div>
                <p className="font-medium">Type</p>
                <p className="text-muted-foreground capitalize">{state.dataSourceType}</p>
              </div>
              {state.dataSourceType === 'directory' && state.directoryRoot && (
                <div>
                  <p className="font-medium">Directory Path</p>
                  <p className="text-xs text-muted-foreground font-mono">
                    {state.directoryRoot}{state.directoryPath ? `/${state.directoryPath}` : ''}
                  </p>
                </div>
              )}
              {state.dataSourceType === 'template' && state.templateId && (
                <div>
                  <p className="font-medium">Template ID</p>
                  <p className="text-muted-foreground">{state.templateId}</p>
                </div>
              )}
              {state.dataSourceType === 'repository' && state.repositoryId && (
                <div>
                  <p className="font-medium">Repository ID</p>
                  <p className="text-muted-foreground">{state.repositoryId}</p>
                </div>
              )}
              {state.datasetPath && (
                <div>
                  <p className="font-medium">Dataset Path</p>
                  <p className="text-muted-foreground font-mono text-xs">{state.datasetPath}</p>
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="category-config">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Settings className="h-4 w-4" />
              Category Configuration
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              {state.category === 'code' && state.language && (
                <div>
                  <p className="font-medium">Language</p>
                  <Badge>{state.language}</Badge>
                  {state.symbolTargets && state.symbolTargets.length > 0 && (
                    <div className="mt-2">
                      <p className="font-medium">Symbol Targets</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.symbolTargets.map((target) => (
                          <Badge key={target} variant="outline">{target}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'framework' && (
                <div className="space-y-2">
                  {state.frameworkId && (
                    <div>
                      <p className="font-medium">Framework</p>
                      <Badge>{state.frameworkId} {state.frameworkVersion || ''}</Badge>
                    </div>
                  )}
                  {state.apiPatterns && state.apiPatterns.length > 0 && (
                    <div>
                      <p className="font-medium">API Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.apiPatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'codebase' && (
                <div className="space-y-2">
                  {state.repoScope && (
                    <div>
                      <p className="font-medium">Repository Scope</p>
                      <p className="text-muted-foreground">{state.repoScope}</p>
                    </div>
                  )}
                  {state.filePatterns && state.filePatterns.length > 0 && (
                    <div>
                      <p className="font-medium">File Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.filePatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                  {state.excludePatterns && state.excludePatterns.length > 0 && (
                    <div>
                      <p className="font-medium">Exclude Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.excludePatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'ephemeral' && (
                <div className="space-y-2">
                  {state.ttlSeconds && (
                    <div>
                      <p className="font-medium">TTL</p>
                      <p className="text-muted-foreground">{state.ttlSeconds} seconds</p>
                    </div>
                  )}
                  {state.contextWindow && (
                    <div>
                      <p className="font-medium">Context Window</p>
                      <p className="text-muted-foreground">{state.contextWindow} tokens</p>
                    </div>
                  )}
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="training-params">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Zap className="h-4 w-4" />
              Training Parameters
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="grid grid-cols-2 gap-4 text-sm pt-2">
              <div>
                <p className="font-medium">Rank</p>
                <p className="text-muted-foreground">{state.rank}</p>
              </div>
              <div>
                <p className="font-medium">Alpha</p>
                <p className="text-muted-foreground">{state.alpha}</p>
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
              {state.warmupSteps && (
                <div>
                  <p className="font-medium">Warmup Steps</p>
                  <p className="text-muted-foreground">{state.warmupSteps}</p>
                </div>
              )}
              {state.maxSeqLength && (
                <div>
                  <p className="font-medium">Max Sequence Length</p>
                  <p className="text-muted-foreground">{state.maxSeqLength}</p>
                </div>
              )}
            </div>
            <div className="mt-4">
              <p className="font-medium text-sm">LoRA Targets ({state.targets.length})</p>
              <div className="flex flex-wrap gap-1 mt-2">
                {state.targets.map((target) => (
                  <Badge key={target} variant="outline">{target}</Badge>
                ))}
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="packaging">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Folder className="h-4 w-4" />
              Packaging & Registration
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              <div className="flex items-center gap-2">
                <p className="font-medium">Package After Training:</p>
                <Badge variant={state.packageAfter ? 'default' : 'outline'}>
                  {state.packageAfter ? 'Yes' : 'No'}
                </Badge>
              </div>
              <div className="flex items-center gap-2">
                <p className="font-medium">Register After Packaging:</p>
                <Badge variant={state.registerAfter ? 'default' : 'outline'}>
                  {state.registerAfter ? 'Yes' : 'No'}
                </Badge>
              </div>
              {state.adaptersRoot && (
                <div>
                  <p className="font-medium">Adapters Root</p>
                  <p className="text-muted-foreground font-mono text-xs">{state.adaptersRoot}</p>
                </div>
              )}
              {state.adapterId && (
                <div>
                  <p className="font-medium">Adapter ID</p>
                  <p className="text-muted-foreground">{state.adapterId}</p>
                </div>
              )}
              {state.tier && (
                <div>
                  <p className="font-medium">Tier</p>
                  <p className="text-muted-foreground">{state.tier}</p>
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>
      </Accordion>

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
    setWizardError(null);
    setValidationError(null);
    setIsLoading(true);
    try {
      // Validate form data against schema
      const validationResult = await TrainingConfigSchema.parseAsync({
        name: state.name,
        description: state.description,
        category: state.category,
        scope: state.scope,
        dataSourceType: state.dataSourceType,
        templateId: state.templateId,
        repositoryId: state.repositoryId,
        customData: state.customData,
        datasetPath: state.datasetPath,
        directoryRoot: state.directoryRoot,
        directoryPath: state.directoryPath,
        language: state.language,
        symbolTargets: state.symbolTargets,
        frameworkId: state.frameworkId,
        frameworkVersion: state.frameworkVersion,
        apiPatterns: state.apiPatterns,
        repoScope: state.repoScope,
        filePatterns: state.filePatterns,
        excludePatterns: state.excludePatterns,
        ttlSeconds: state.ttlSeconds,
        contextWindow: state.contextWindow,
        rank: state.rank,
        alpha: state.alpha,
        epochs: state.epochs,
        learningRate: state.learningRate,
        batchSize: state.batchSize,
        targets: state.targets,
        warmupSteps: state.warmupSteps,
        maxSeqLength: state.maxSeqLength,
        packageAfter: state.packageAfter,
        registerAfter: state.registerAfter,
        adaptersRoot: state.adaptersRoot,
        adapterId: state.adapterId,
        tier: state.tier,
      });

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
      const trainingRequest: any = {
        adapter_name: state.name,
        config: trainingConfig,
        adapters_root: state.adaptersRoot || undefined,
        package: !!state.packageAfter,
        register: !!state.registerAfter,
        adapter_id: state.adapterId || undefined,
        tier: state.tier,
      };

      // Add category and configuration fields
      trainingRequest.category = state.category || 'codebase';

      switch (state.category) {
        case 'code':
          trainingRequest.language = state.language;
          if (state.symbolTargets && state.symbolTargets.length > 0) {
            trainingRequest.symbol_targets = state.symbolTargets;
          }
          break;
        case 'framework':
          trainingRequest.framework_id = state.frameworkId;
          trainingRequest.framework_version = state.frameworkVersion;
          if (state.apiPatterns && state.apiPatterns.length > 0) {
            trainingRequest.api_patterns = state.apiPatterns;
          }
          break;
        case 'codebase':
          trainingRequest.repo_scope = state.repoScope;
          if (state.filePatterns && state.filePatterns.length > 0) {
            trainingRequest.file_patterns = state.filePatterns;
          }
          if (state.excludePatterns && state.excludePatterns.length > 0) {
            trainingRequest.exclude_patterns = state.excludePatterns;
          }
          break;
        case 'ephemeral':
          if (state.ttlSeconds) {
            trainingRequest.ttl_seconds = state.ttlSeconds;
          }
          if (state.contextWindow) {
            trainingRequest.context_window = state.contextWindow;
          }
          break;
      }

      // Add data source based on type
      if (state.dataSourceType === 'template' && state.templateId) {
        trainingRequest.template_id = state.templateId;
      } else if (state.dataSourceType === 'repository' && state.repositoryId) {
        trainingRequest.repo_id = state.repositoryId;
      } else if (state.dataSourceType === 'directory') {
        // Directory-based training
        trainingRequest.directory_root = state.directoryRoot;
        trainingRequest.directory_path = state.directoryPath || '.';
      } else if (state.dataSourceType === 'custom') {
        // For custom, dataset_path is included
      }

      if (state.datasetPath) {
        trainingRequest.dataset_path = state.datasetPath;
      }

      const job = await apiClient.startTraining(trainingRequest);

      // Success - training started, clear persisted state
      clearPersistedState();
      toast.success(`Training job ${job.id} started successfully!`);
      onComplete(job.id);
    } catch (error) {
      if (error instanceof Error && error.name === 'ZodError') {
        // Format Zod validation errors
        const validationResult = formatValidationError(error as any);
        const firstError = validationResult.errors[0];
        setValidationError(firstError?.message || 'Validation failed');
        logger.warn('Training wizard validation failed', {
          component: 'TrainingWizard',
          operation: 'validateForm',
          errorCount: validationResult.errors.length,
        });
      } else {
        const err = error instanceof Error ? error : new Error('Failed to start training');
        setWizardError(err);
        logger.error('Training job start failed', {
          component: 'TrainingWizard',
          operation: 'startTraining',
          adapterName: state.name,
        }, toError(error));
        toast.error(err.message);
      }
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
        setValidationError(null);
        if (!state.category) {
          setValidationError('Please select an adapter category');
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
        setValidationError(null);
        if (!state.name.trim()) {
          setValidationError('Adapter name is required');
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
        setValidationError(null);
        if (state.dataSourceType === 'template' && !state.templateId) {
          setValidationError('Please select a template');
          return false;
        }
        if (state.dataSourceType === 'repository' && !state.repositoryId) {
          setValidationError('Please select a repository');
          return false;
        }
        if (state.dataSourceType === 'directory' && !state.directoryRoot?.trim()) {
          setValidationError('Please provide a directory root path for directory-based training');
          return false;
        }
        if (state.dataSourceType === 'custom' && !state.datasetPath?.trim()) {
          setValidationError('For custom training, please provide a dataset_path pointing to a training JSON file');
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
      description: 'Set training speed and style',
      component: <TrainingParamsStep />,
      validate: () => {
        setValidationError(null);
        if (state.targets.length === 0) {
          setValidationError('Please select at least one LoRA target module');
          return false;
        }
        return true;
      },
    },
    {
      id: 'packaging',
      title: 'Packaging & Registration',
      description: 'Artifacts and registry',
      component: <PackagingStep />,
    },
    {
      id: 'review',
      title: 'Review',
      description: 'Confirm and start',
      component: <ReviewStep />,
    },
  ];

  return (
    <div className={spacing.sectionGap}>
      {/* Resume Dialog */}
      <Dialog open={showResumeDialog} onOpenChange={setShowResumeDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <RotateCcw className="h-5 w-5" />
              Resume Previous Session?
            </DialogTitle>
            <DialogDescription>
              We found a saved training configuration from a previous session. Would you like to resume where you left off?
            </DialogDescription>
          </DialogHeader>
          {savedState && (
            <div className="space-y-2 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Adapter:</span>
                <span className="font-medium">{savedState.name || 'Untitled'}</span>
              </div>
              {savedState.category && (
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Category:</span>
                  <span className="font-medium capitalize">{savedState.category}</span>
                </div>
              )}
              {savedState.currentStep !== undefined && (
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Progress:</span>
                  <span className="font-medium">Step {savedState.currentStep + 1} of 7</span>
                </div>
              )}
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={handleStartFresh}>
              Start Fresh
            </Button>
            <Button onClick={handleResume}>
              Resume
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <BreadcrumbNavigation />
      <div className="flex justify-between items-center mb-4">
        <h2 className={textSizes.title}>Training Wizard</h2>
        <div className="flex items-center gap-2">
          {hasSavedState && !showResumeDialog && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                const saved = loadSavedState();
                if (saved && saved.currentStep !== undefined) {
                  setCurrentStep(saved.currentStep);
                }
              }}
              className="text-xs"
            >
              <RotateCcw className="h-3 w-3 mr-1" />
              Load Saved
            </Button>
          )}
        </div>
      </div>

      {/* Error Recovery */}
      {wizardError && (
        <ErrorRecovery
          error={wizardError.message}
          onRetry={() => {
            setWizardError(null);
            setCurrentStep(0);
          }}
        />
      )}
      {validationError && (
        <ErrorRecovery
          error={validationError}
          onRetry={() => setValidationError(null)}
        />
      )}

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
    </div>
  );
}

// Outer component with DensityProvider
export function TrainingWizard({ onComplete, onCancel }: TrainingWizardProps) {
  return (
    <DensityProvider pageKey="training-wizard">
      <TrainingWizardInner onComplete={onComplete} onCancel={onCancel} />
    </DensityProvider>
  );
}
