import { LucideIcon, Wrench, Cpu, Code, Shield, BarChart3, Briefcase, Zap, Upload, Settings, Play, FileText, Activity, GitBranch } from 'lucide-react';

// TypeScript interfaces for persona journey data
export interface StageContent {
  whatAppears: string;
  why: string;
  context: string;
  route?: string; // Real page route in the app
  mentalModelExplanation?: string; // How this relates to the unified mental model
  mockComponent?: string; // Name of mock component to render for this stage
}

export interface Stage {
  id: string;
  title: string;
  content: StageContent;
}

export interface Persona {
  id: string;
  name: string;
  icon: LucideIcon;
  description: string;
  stages: Stage[];
}

// Stage 1: Training Environment Setup
const mlEngineerStage1: Stage = {
  id: 'training-setup',
  title: 'Training Environment Setup',
  content: {
    whatAppears: 'Training page with dataset upload, rank selection, and training job controls',
    why: 'Create custom LoRA adapters that specialize the base model for your specific task',
    context: 'Training is the first step in the flow: Training → Adapter → Stack → Inference',
    route: '/training',
    mentalModelExplanation: 'This page creates **Adapters** (core entity #2). You upload a dataset, configure LoRA rank, and train weights that will become a registered adapter.'
  }
};

// Stage 2: Adapter Registration
const mlEngineerStage2: Stage = {
  id: 'adapter-registration',
  title: 'Adapter Registration',
  content: {
    whatAppears: 'Adapters page showing lifecycle states (Unloaded, Cold, Warm, Hot, Resident), memory usage, and activation percentages',
    why: 'Register trained adapters in the system and manage their lifecycle',
    context: 'After training completes, adapters must be registered with a semantic name (workspace/domain/purpose/revision)',
    route: '/adapters',
    mentalModelExplanation: 'This page shows **Adapters** (entity #2) and their **Lifecycle** states. You can see which adapters are loaded in memory, their activation %, and pin critical adapters to prevent eviction.'
  }
};

// Stage 3: Stack Creation
const mlEngineerStage3: Stage = {
  id: 'stack-creation',
  title: 'Stack Creation',
  content: {
    whatAppears: 'Stack builder with adapter selection, workflow type chooser (Sequential, Parallel, UpstreamDownstream), and policy configuration',
    why: 'Combine multiple adapters into reusable execution sets with workflow rules',
    context: 'After adapters are registered, combine them into stacks for specific use cases',
    route: '/stacks',
    mentalModelExplanation: 'This page creates **Stacks** (entity #3). A stack is a workspace-scoped set of adapters + workflow rules. For example, [syntax-analyzer, style-checker] in Sequential mode.'
  }
};

// Stage 4: Inference Testing
const mlEngineerStage4: Stage = {
  id: 'inference-testing',
  title: 'Inference Testing',
  content: {
    whatAppears: 'Inference playground with prompt input, stack selector, and response output. Router inspector shows which adapters were selected.',
    why: 'Test your stack with real inference requests and see router decisions',
    context: 'Final step: verify the complete flow works before production deployment',
    route: '/inference',
    mentalModelExplanation: 'This page demonstrates the full execution flow: **Router** (entity #4) selects top-K adapters from your stack, **Kernel** (entity #5) executes them, and **Telemetry** (entity #6) records all events.'
  }
};

// DevOps Engineer Stages
const devOpsStage1: Stage = {
  id: 'server-config',
  title: 'Server Configuration Panel',
  content: {
    whatAppears: 'Settings page with production mode toggle, policy enforcement controls, and system configuration',
    why: 'Configure system-wide settings and deployment profiles (dev/staging/prod)',
    context: 'Infrastructure provisioning and configuration',
    route: '/settings',
    mentalModelExplanation: 'Settings control system-wide behavior like production mode (zero egress), policy enforcement, and **Workspace** resource limits.'
  }
};

const devOpsStage2: Stage = {
  id: 'resource-management',
  title: 'Resource Management Dashboard',
  content: {
    whatAppears: 'Dashboard showing UMA memory stats, adapter memory usage, eviction metrics, and lifecycle tier distribution',
    why: 'Monitor memory pressure, adapter evictions, and resource utilization across workspaces',
    context: 'Ongoing operations and memory management',
    route: '/dashboard',
    mentalModelExplanation: 'The dashboard shows **Lifecycle** tier distribution (Unloaded → Hot), memory pressure levels, and which adapters are consuming resources. Memory management automatically evicts Cold adapters when pressure exceeds 85%.'
  }
};

const devOpsStage3: Stage = {
  id: 'deployment-pipeline',
  title: 'Deployment Pipeline Interface',
  content: {
    whatAppears: 'Adapters page with upload, registration, and deployment controls for new adapter versions',
    why: 'Deploy new adapter versions safely with semantic naming and version control',
    context: 'Release management and continuous deployment',
    route: '/adapters',
    mentalModelExplanation: 'Deploying new **Adapters** follows semantic naming (workspace/domain/purpose/revision). Adapters start in Unloaded state and are promoted through lifecycle tiers based on activation %.'
  }
};

const devOpsStage4: Stage = {
  id: 'monitoring-alerting',
  title: 'Monitoring & Alerting Center',
  content: {
    whatAppears: 'Monitoring page with system metrics, health checks, performance graphs, and telemetry event streams',
    why: 'Track system health, performance SLAs, and detect issues proactively',
    context: 'Production operations and incident response',
    route: '/monitoring',
    mentalModelExplanation: 'Monitoring aggregates **Telemetry** events from all system operations (inference, lifecycle, policy enforcement) to provide real-time observability and alerting.'
  }
};

// Application Developer Stages
const appDevStage1: Stage = {
  id: 'api-documentation',
  title: 'API Documentation Browser',
  content: {
    whatAppears: 'Documentation viewer showing REST API endpoints, request/response schemas, and integration examples',
    why: 'Learn how to integrate AdapterOS into your application via REST API',
    context: 'Initial integration planning and API exploration',
    route: '/dashboard',
    mentalModelExplanation: 'The API exposes all core entities: create **Workspaces**, register **Adapters**, build **Stacks**, run inference, and query **Telemetry**.'
  }
};

const appDevStage2: Stage = {
  id: 'sdk-manager',
  title: 'Client SDK Setup',
  content: {
    whatAppears: 'Documentation for REST API client setup in various languages (curl, Python, Node.js)',
    why: 'Get started with API integration using your preferred language',
    context: 'Development environment setup and first API call',
    route: '/dashboard',
    mentalModelExplanation: 'Client SDKs wrap the REST API to create **Workspaces**, manage **Adapters**, and run inference requests. The API uses JWT auth with role-based permissions.'
  }
};

const appDevStage3: Stage = {
  id: 'integration-testing',
  title: 'Integration Testing Console',
  content: {
    whatAppears: 'Inference playground where you can test API calls with different prompts, stacks, and parameters',
    why: 'Test your integration with real inference requests and debug API responses',
    context: 'Development, testing, and debugging',
    route: '/inference',
    mentalModelExplanation: 'The inference playground calls the same API your app will use. It sends a request → **Router** selects top-K adapters → **Kernel** executes → **Telemetry** logs all events.'
  }
};

const appDevStage4: Stage = {
  id: 'performance-optimization',
  title: 'Performance Optimization Panel',
  content: {
    whatAppears: 'Monitoring page with latency graphs, throughput metrics, and router decision analytics',
    why: 'Analyze inference performance and optimize request patterns',
    context: 'Production optimization and performance tuning',
    route: '/monitoring',
    mentalModelExplanation: '**Telemetry** captures latency, tokens/sec, and **Router** decisions for every inference. Use this data to optimize K-sparse settings, adapter selection, and memory usage.'
  }
};

// Security Engineer Stages
const securityStage1: Stage = {
  id: 'policy-config',
  title: 'Policy Configuration',
  content: {
    whatAppears: 'Policies page showing 23 canonical policy packs (Egress, Determinism, Router, Evidence, etc.) with enforcement status',
    why: 'Define and enforce security policies across workspaces, adapters, and execution',
    context: 'Policy packs enforce rules at all layers of the mental model',
    route: '/security/policies',
    mentalModelExplanation: 'Policies enforce rules across all entities: **Workspaces** (isolation), **Stacks** (composition), **Router** (selection), **Kernel** (execution). Example: Egress Policy ensures zero network egress in production.'
  }
};

const securityStage2: Stage = {
  id: 'telemetry-audit',
  title: 'Telemetry Audit Trail',
  content: {
    whatAppears: 'Telemetry page with event timeline, Merkle chain visualization, and bundle download',
    why: 'Audit all system operations with immutable event trail',
    context: 'Every operation (inference, lifecycle, policy) emits telemetry events',
    route: '/telemetry',
    mentalModelExplanation: '**Telemetry** (entity #6) captures all events in a Merkle chain. Each event references the previous hash, creating an immutable audit trail. Bundles are compressed, signed archives used for replay.'
  }
};

const securityStage3: Stage = {
  id: 'isolation-testing',
  title: 'Isolation Testing Interface',
  content: {
    whatAppears: 'Workspace sandbox controls and isolation verification tools',
    why: 'Test and validate workspace separation mechanisms',
    context: 'Security validation',
    mockComponent: 'SecurityIsolationTester'
  }
};

const securityStage4: Stage = {
  id: 'golden-runs-replay',
  title: 'Golden Runs & Replay',
  content: {
    whatAppears: 'Golden runs page with verified executions, replay controls, and divergence reports',
    why: 'Verify determinism by replaying golden runs and detecting divergences',
    context: 'Determinism verification ensures outputs are reproducible',
    route: '/golden-runs',
    mentalModelExplanation: '**Golden Runs** (entity #7) are verified, deterministic executions. **Replay** re-executes them to verify byte-for-byte output matching. Divergences indicate non-determinism and are logged to telemetry.'
  }
};

// Data Scientist Stages
const dataScientistStage1: Stage = {
  id: 'experiment-tracking',
  title: 'Experiment Tracking Interface',
  content: {
    whatAppears: 'Training page showing job history, loss curves, and hyperparameter configurations for comparing adapter experiments',
    why: 'Track and compare different adapter training runs to find optimal configurations',
    context: 'Research experimentation with different ranks, alphas, and datasets',
    route: '/training',
    mentalModelExplanation: 'Each training job creates a new **Adapter** variant. Compare loss curves, convergence rates, and final performance to select the best configuration for your use case.'
  }
};

const dataScientistStage2: Stage = {
  id: 'dataset-management',
  title: 'Dataset Management Portal',
  content: {
    whatAppears: 'Training page with dataset upload, validation, preprocessing controls, and dataset statistics',
    why: 'Upload, preprocess, and validate training data before adapter creation',
    context: 'Data preparation and quality validation',
    route: '/training',
    mentalModelExplanation: 'Training datasets are content-addressed (BLAKE3 hash). The system validates format, checks for duplicates, and computes statistics before training **Adapters**.'
  }
};

const dataScientistStage3: Stage = {
  id: 'evaluation-framework',
  title: 'Evaluation Framework UI',
  content: {
    whatAppears: 'Golden runs page where you can create test suites, run benchmarks, and compare adapter performance',
    why: 'Measure adapter performance against baselines using reproducible test cases',
    context: 'Model validation and performance benchmarking',
    route: '/golden-runs',
    mentalModelExplanation: '**Golden Runs** are test cases with verified outputs. Run adapters against golden run inputs, then compare performance metrics (accuracy, latency, output quality). **Replay** ensures results are reproducible.'
  }
};

const dataScientistStage4: Stage = {
  id: 'collaboration-hub',
  title: 'Collaboration Hub',
  content: {
    whatAppears: 'Adapters page where team members can view, share, and fork adapters with semantic versioning',
    why: 'Share research findings, fork adapters, and collaborate on model improvements',
    context: 'Team collaboration and knowledge sharing',
    route: '/adapters',
    mentalModelExplanation: '**Adapters** use semantic naming (workspace/domain/purpose/revision). Teams can fork adapters (create variants), track lineage (parent_id), and share across **Workspaces** with ACLs.'
  }
};

// Product Manager Stages
const productManagerStage1: Stage = {
  id: 'feature-analytics',
  title: 'Feature Usage Analytics',
  content: {
    whatAppears: 'Monitoring page with usage metrics: which adapters are active, request volume, activation %, and workspace distribution',
    why: 'Understand which adapters are used most, by which workspaces, and how often',
    context: 'Product analytics and feature adoption tracking',
    route: '/monitoring',
    mentalModelExplanation: '**Telemetry** tracks every inference request, router decision, and adapter activation. Aggregate metrics show which **Adapters** are valuable and which **Workspaces** are active users.'
  }
};

const productManagerStage2: Stage = {
  id: 'system-performance',
  title: 'System Performance Overview',
  content: {
    whatAppears: 'Dashboard with system KPIs: uptime, average latency, tokens/sec, memory utilization, and policy compliance',
    why: 'Track system health and business-critical metrics for stakeholder reporting',
    context: 'Executive dashboards and SLA monitoring',
    route: '/dashboard',
    mentalModelExplanation: 'The dashboard aggregates **Telemetry** events to show system-wide metrics: inference throughput, **Router** selection latency, **Lifecycle** evictions, and policy enforcement status.'
  }
};

const productManagerStage3: Stage = {
  id: 'config-management',
  title: 'Configuration Management Portal',
  content: {
    whatAppears: 'Workspaces page where you can create workspaces, configure resource limits, and assign policies',
    why: 'Define service tiers (free, pro, enterprise) by configuring workspace resource limits and policies',
    context: 'Product tier management and workspace provisioning',
    route: '/admin/tenants',
    mentalModelExplanation: '**Workspaces** are the isolation boundary. Configure memory limits, adapter quotas, and policy packs per workspace to create service tiers (e.g., free tier = 1GB, 5 adapters; pro tier = 10GB, unlimited adapters).'
  }
};

const productManagerStage4: Stage = {
  id: 'feedback-integration',
  title: 'Feedback Integration Hub',
  content: {
    whatAppears: 'Telemetry page where you can analyze error rates, policy violations, and user-reported issues',
    why: 'Identify pain points, bugs, and feature gaps from real user interactions',
    context: 'Product feedback loop and roadmap planning',
    route: '/telemetry',
    mentalModelExplanation: '**Telemetry** events include errors, warnings, and policy violations. Analyze patterns to identify: which **Adapters** fail most, which **Router** decisions cause errors, and where users hit limits.'
  }
};

// Persona definitions
export const personas: Persona[] = [
  {
    id: 'ml-engineer',
    name: 'ML Engineer',
    icon: Cpu,
    description: 'Senior ML engineer training and deploying custom LoRA adapters',
    stages: [mlEngineerStage1, mlEngineerStage2, mlEngineerStage3, mlEngineerStage4]
  },
  {
    id: 'devops-engineer',
    name: 'DevOps Engineer',
    icon: Wrench,
    description: 'DevOps engineer managing infrastructure and production deployments',
    stages: [devOpsStage1, devOpsStage2, devOpsStage3, devOpsStage4]
  },
  {
    id: 'app-developer',
    name: 'Application Developer',
    icon: Code,
    description: 'Full-stack developer integrating AdapterOS into applications',
    stages: [appDevStage1, appDevStage2, appDevStage3, appDevStage4]
  },
  {
    id: 'security-engineer',
    name: 'Security Engineer',
    icon: Shield,
    description: 'Security engineer ensuring compliance and policy enforcement',
    stages: [securityStage1, securityStage2, securityStage3, securityStage4]
  },
  {
    id: 'data-scientist',
    name: 'Data Scientist',
    icon: BarChart3,
    description: 'Data scientist experimenting with and evaluating adapters',
    stages: [dataScientistStage1, dataScientistStage2, dataScientistStage3, dataScientistStage4]
  },
  {
    id: 'product-manager',
    name: 'Product Manager',
    icon: Briefcase,
    description: 'Product manager overseeing product strategy and requirements',
    stages: [productManagerStage1, productManagerStage2, productManagerStage3, productManagerStage4]
  }
];

// Helper functions
export function getPersonaById(id: string): Persona | undefined {
  return personas.find(persona => persona.id === id);
}

export function getStageByIds(personaId: string, stageId: string): Stage | undefined {
  const persona = getPersonaById(personaId);
  return persona?.stages.find(stage => stage.id === stageId);
}
