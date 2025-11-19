// Workflow template definitions for common AdapterOS scenarios

import { WorkflowTemplate } from './types';

export const WORKFLOW_TEMPLATES: WorkflowTemplate[] = [
  // Quick Training Workflow
  {
    id: 'quick-training',
    name: 'Quick Training',
    description: 'Fast adapter training with sensible defaults for rapid iteration',
    category: 'training',
    estimatedDuration: '5 minutes',
    difficulty: 'beginner',
    tags: ['training', 'quick', 'development'],
    requiredInputs: [
      {
        id: 'datasetPath',
        label: 'Dataset Path',
        type: 'file',
        required: true,
        placeholder: 'data/training.json',
        helpText: 'Path to your training dataset JSON file',
      },
      {
        id: 'adapterName',
        label: 'Adapter Name',
        type: 'text',
        required: true,
        placeholder: 'my-quick-adapter',
        helpText: 'Unique identifier for your adapter',
      },
    ],
    steps: [
      {
        id: 'select-dataset',
        title: 'Select Dataset',
        description: 'Choose or upload your training dataset',
        component: 'DatasetSelector',
        config: {
          allowUpload: true,
          validateFormat: true,
        },
        required: true,
      },
      {
        id: 'configure',
        title: 'Configure Training',
        description: 'Quick configuration with optimized defaults',
        component: 'QuickTrainingConfig',
        config: {
          defaults: {
            rank: 8,
            alpha: 16,
            epochs: 3,
            learningRate: 0.0003,
            batchSize: 4,
            targets: ['q_proj', 'v_proj'],
          },
          allowOverride: true,
        },
        required: true,
        helpText: 'Using production-ready defaults. Override only if needed.',
      },
      {
        id: 'start-training',
        title: 'Start Training',
        description: 'Begin the training process',
        component: 'TrainingStarter',
        config: {
          showProgress: true,
          allowCancel: true,
        },
        required: true,
      },
      {
        id: 'verify-results',
        title: 'Verify Results',
        description: 'Quick validation of trained adapter',
        component: 'TrainingVerification',
        config: {
          autoTest: true,
          showMetrics: true,
        },
        required: false,
        helpText: 'Optional: Run quick validation tests',
      },
    ],
  },

  // Production Deployment Workflow
  {
    id: 'production-deployment',
    name: 'Production Deployment',
    description: 'Full validation and promotion workflow for production adapters',
    category: 'deployment',
    estimatedDuration: '15 minutes',
    difficulty: 'advanced',
    tags: ['deployment', 'production', 'validation'],
    requiredInputs: [
      {
        id: 'adapterId',
        label: 'Adapter ID',
        type: 'adapter',
        required: true,
        helpText: 'Select the adapter to deploy to production',
      },
      {
        id: 'goldenRunId',
        label: 'Golden Run ID',
        type: 'select',
        required: true,
        helpText: 'Golden baseline for comparison',
      },
    ],
    steps: [
      {
        id: 'load-adapter',
        title: 'Load Adapter',
        description: 'Load adapter into lifecycle manager',
        component: 'AdapterLoader',
        config: {
          validateBeforeLoad: true,
          checkDependencies: true,
        },
        required: true,
      },
      {
        id: 'validate-adapter',
        title: 'Validate Adapter',
        description: 'Run comprehensive validation checks',
        component: 'AdapterValidator',
        config: {
          checks: [
            'manifest',
            'weights',
            'policy',
            'determinism',
            'security',
          ],
          failFast: false,
        },
        required: true,
        helpText: 'All validation checks must pass to continue',
      },
      {
        id: 'golden-compare',
        title: 'Golden Comparison',
        description: 'Compare against golden baseline',
        component: 'GoldenComparison',
        config: {
          strictness: 'epsilon-tolerant',
          epsilonTolerance: 1e-6,
          verifyToolchain: true,
          verifyAdapters: true,
          verifyDevice: true,
          verifySignature: true,
        },
        required: true,
        validation: {
          type: 'custom',
          message: 'Golden comparison must pass',
          validate: (data: any) => data.comparisonPassed === true,
        },
      },
      {
        id: 'promote',
        title: 'Promote to Production',
        description: 'Promote adapter with gate checks',
        component: 'AdapterPromotion',
        config: {
          requireApproval: true,
          runGates: true,
          skipGates: false,
        },
        required: true,
        helpText: 'Final promotion step - requires approval',
      },
      {
        id: 'verify-deployment',
        title: 'Verify Deployment',
        description: 'Post-deployment health checks',
        component: 'DeploymentVerification',
        config: {
          runHealthChecks: true,
          monitorDuration: 60,
          autoRollback: true,
        },
        required: true,
      },
    ],
  },

  // Experimental Training Workflow
  {
    id: 'experimental-training',
    name: 'Experimental Training',
    description: 'Rapid prototyping with ephemeral adapters for quick experiments',
    category: 'experimental',
    estimatedDuration: '10 minutes',
    difficulty: 'intermediate',
    tags: ['experimental', 'prototype', 'ephemeral'],
    requiredInputs: [
      {
        id: 'codeDirectory',
        label: 'Code Directory',
        type: 'directory',
        required: true,
        helpText: 'Directory containing code to analyze',
      },
      {
        id: 'experimentName',
        label: 'Experiment Name',
        type: 'text',
        required: true,
        placeholder: 'exp-feature-x',
        helpText: 'Name for this experiment',
      },
      {
        id: 'ttl',
        label: 'Time to Live (seconds)',
        type: 'number',
        required: false,
        default: 3600,
        helpText: 'Auto-evict after this duration (default: 1 hour)',
      },
    ],
    steps: [
      {
        id: 'upload-code',
        title: 'Upload Code',
        description: 'Upload and analyze code directory',
        component: 'CodeUploader',
        config: {
          analyze: true,
          extractPatterns: true,
          supportedLanguages: ['typescript', 'javascript', 'python', 'rust'],
        },
        required: true,
      },
      {
        id: 'auto-configure',
        title: 'Auto-Configure',
        description: 'Automatic configuration based on code analysis',
        component: 'AutoConfigurator',
        config: {
          analyzeComplexity: true,
          suggestRank: true,
          detectFrameworks: true,
          tier: 8, // Ephemeral tier
        },
        required: true,
        helpText: 'Configuration auto-generated from code analysis',
      },
      {
        id: 'quick-train',
        title: 'Quick Train',
        description: 'Fast training with reduced epochs',
        component: 'QuickTrainer',
        config: {
          maxEpochs: 2,
          fastMode: true,
          skipValidation: true,
        },
        required: true,
      },
      {
        id: 'test-adapter',
        title: 'Test Adapter',
        description: 'Interactive testing and validation',
        component: 'InteractiveTester',
        config: {
          allowPrompts: true,
          showOutput: true,
          saveResults: true,
        },
        required: false,
      },
    ],
  },

  // Golden Run Comparison Workflow
  {
    id: 'golden-comparison',
    name: 'Golden Run Comparison',
    description: 'Compare adapters against golden baselines for determinism validation',
    category: 'comparison',
    estimatedDuration: '8 minutes',
    difficulty: 'intermediate',
    tags: ['comparison', 'validation', 'determinism'],
    requiredInputs: [
      {
        id: 'adapterId',
        label: 'Adapter to Test',
        type: 'adapter',
        required: true,
        helpText: 'Adapter to compare against golden baseline',
      },
      {
        id: 'goldenRunId',
        label: 'Golden Baseline',
        type: 'select',
        required: true,
        helpText: 'Reference golden run for comparison',
      },
      {
        id: 'strictness',
        label: 'Comparison Strictness',
        type: 'select',
        required: false,
        default: 'epsilon-tolerant',
        options: [
          { label: 'Bitwise (Exact)', value: 'bitwise' },
          { label: 'Epsilon Tolerant', value: 'epsilon-tolerant' },
          { label: 'Statistical', value: 'statistical' },
        ],
      },
    ],
    steps: [
      {
        id: 'load-adapter',
        title: 'Load Adapter',
        description: 'Load adapter for comparison',
        component: 'AdapterLoader',
        config: {
          validateManifest: true,
        },
        required: true,
      },
      {
        id: 'load-golden',
        title: 'Load Golden Baseline',
        description: 'Fetch golden run data',
        component: 'GoldenLoader',
        config: {
          verifySignature: true,
          validateMetadata: true,
        },
        required: true,
      },
      {
        id: 'compare',
        title: 'Run Comparison',
        description: 'Execute comparison with configured strictness',
        component: 'ComparisonExecutor',
        config: {
          generateReport: true,
          showDivergences: true,
          highlightFailures: true,
        },
        required: true,
      },
      {
        id: 'generate-report',
        title: 'Generate Report',
        description: 'Create detailed comparison report',
        component: 'ReportGenerator',
        config: {
          includeCharts: true,
          includeMetadata: true,
          exportFormats: ['json', 'pdf'],
        },
        required: true,
        helpText: 'Comprehensive report with visualizations',
      },
    ],
  },

  // Stack Creation Workflow
  {
    id: 'stack-creation',
    name: 'Create Adapter Stack',
    description: 'Compose and validate multi-adapter stacks for complex workflows',
    category: 'stack',
    estimatedDuration: '8 minutes',
    difficulty: 'intermediate',
    tags: ['stack', 'composition', 'multi-adapter'],
    requiredInputs: [
      {
        id: 'stackName',
        label: 'Stack Name',
        type: 'text',
        required: true,
        placeholder: 'my-production-stack',
        helpText: 'Unique name for this adapter stack',
      },
      {
        id: 'adapterIds',
        label: 'Adapters',
        type: 'adapter',
        required: true,
        helpText: 'Select adapters to include in stack (order matters)',
      },
    ],
    steps: [
      {
        id: 'select-adapters',
        title: 'Select Adapters',
        description: 'Choose adapters to compose',
        component: 'AdapterMultiSelector',
        config: {
          allowReorder: true,
          showPreview: true,
          minAdapters: 1,
          maxAdapters: 10,
        },
        required: true,
        validation: {
          type: 'min',
          value: 1,
          message: 'At least one adapter must be selected',
        },
      },
      {
        id: 'order-adapters',
        title: 'Order Adapters',
        description: 'Arrange adapters in execution order',
        component: 'AdapterOrdering',
        config: {
          dragAndDrop: true,
          showDependencies: true,
          validateOrder: true,
        },
        required: true,
        helpText: 'Drag to reorder - execution flows from top to bottom',
      },
      {
        id: 'validate-stack',
        title: 'Validate Stack',
        description: 'Check for conflicts and compatibility',
        component: 'StackValidator',
        config: {
          checkConflicts: true,
          checkCompatibility: true,
          checkPerformance: true,
        },
        required: true,
        validation: {
          type: 'custom',
          message: 'Stack validation must pass',
          validate: (data: any) => data.validationPassed === true,
        },
      },
      {
        id: 'test-stack',
        title: 'Test Stack',
        description: 'Run integration tests',
        component: 'StackTester',
        config: {
          runSampleInference: true,
          measureLatency: true,
          checkMemory: true,
        },
        required: false,
        helpText: 'Recommended: Validate stack behavior',
      },
      {
        id: 'save-stack',
        title: 'Save Stack',
        description: 'Register stack for use',
        component: 'StackSaver',
        config: {
          generateManifest: true,
          registerInDB: true,
          makeAvailable: true,
        },
        required: true,
      },
    ],
  },

  // Maintenance Workflow
  {
    id: 'adapter-maintenance',
    name: 'Adapter Maintenance',
    description: 'Clean up, optimize, and manage adapter lifecycle',
    category: 'maintenance',
    estimatedDuration: '10 minutes',
    difficulty: 'beginner',
    tags: ['maintenance', 'cleanup', 'optimization'],
    requiredInputs: [
      {
        id: 'tenantId',
        label: 'Tenant ID',
        type: 'select',
        required: false,
        helpText: 'Filter by tenant (optional)',
      },
    ],
    steps: [
      {
        id: 'scan-adapters',
        title: 'Scan Adapters',
        description: 'Identify unused and expired adapters',
        component: 'AdapterScanner',
        config: {
          scanExpired: true,
          scanUnused: true,
          scanOrphaned: true,
          daysInactive: 30,
        },
        required: true,
      },
      {
        id: 'review-findings',
        title: 'Review Findings',
        description: 'Select adapters for cleanup',
        component: 'MaintenanceReview',
        config: {
          allowSelection: true,
          showDetails: true,
          confirmDeletion: true,
        },
        required: true,
      },
      {
        id: 'cleanup',
        title: 'Clean Up',
        description: 'Remove selected adapters',
        component: 'AdapterCleanup',
        config: {
          backupBeforeDelete: true,
          confirmEach: false,
          showProgress: true,
        },
        required: true,
        helpText: 'Creates backup before deletion',
      },
      {
        id: 'optimize',
        title: 'Optimize Storage',
        description: 'Reclaim space and optimize database',
        component: 'StorageOptimizer',
        config: {
          vacuumDB: true,
          compactLogs: true,
          rebuildIndices: true,
        },
        required: false,
      },
      {
        id: 'summary',
        title: 'Summary',
        description: 'Review maintenance results',
        component: 'MaintenanceSummary',
        config: {
          showStatistics: true,
          showSpaceReclaimed: true,
          exportReport: true,
        },
        required: true,
      },
    ],
  },
];

// Helper to get template by ID
export function getTemplateById(id: string): WorkflowTemplate | undefined {
  return WORKFLOW_TEMPLATES.find((t) => t.id === id);
}

// Helper to get templates by category
export function getTemplatesByCategory(category: string): WorkflowTemplate[] {
  return WORKFLOW_TEMPLATES.filter((t) => t.category === category);
}

// Helper to get templates by tag
export function getTemplatesByTag(tag: string): WorkflowTemplate[] {
  return WORKFLOW_TEMPLATES.filter((t) => t.tags.includes(tag));
}

// Helper to search templates
export function searchTemplates(query: string): WorkflowTemplate[] {
  const lowerQuery = query.toLowerCase();
  return WORKFLOW_TEMPLATES.filter(
    (t) =>
      t.name.toLowerCase().includes(lowerQuery) ||
      t.description.toLowerCase().includes(lowerQuery) ||
      t.tags.some((tag) => tag.toLowerCase().includes(lowerQuery))
  );
}
