import { LucideIcon, Wrench, Cpu, Code, Shield, BarChart3, Briefcase, Zap, Upload, Settings, Play, FileText, Activity, GitBranch } from 'lucide-react';

// TypeScript interfaces for persona journey data
export interface StageContent {
  whatAppears: string;
  why: string;
  context: string;
  mockComponent?: string; // Reference to mock component name
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
    whatAppears: 'CLI terminal with `aos train` command interface',
    why: 'Direct access to training pipeline for custom LoRA adapters',
    context: 'Local development or GPU cluster environment',
    mockComponent: 'MLEngineerTrainingSetup'
  }
};

// Stage 2: Model Registry Interaction
const mlEngineerStage2: Stage = {
  id: 'registry-interaction',
  title: 'Model Registry Interaction',
  content: {
    whatAppears: 'Registry browser showing adapter versions, metadata, and performance metrics',
    why: 'Version control and collaboration for trained adapters',
    context: 'After training completion, before deployment consideration',
    mockComponent: 'MLEngineerRegistryBrowser'
  }
};

// Stage 3: Performance Monitoring Dashboard
const mlEngineerStage3: Stage = {
  id: 'performance-monitoring',
  title: 'Performance Monitoring Dashboard',
  content: {
    whatAppears: 'Training metrics, loss curves, GPU utilization graphs',
    why: 'Validate training quality and resource efficiency',
    context: 'During and after training runs',
    mockComponent: 'MLEngineerTrainingMetrics'
  }
};

// Stage 4: Inference Testing Interface
const mlEngineerStage4: Stage = {
  id: 'inference-testing',
  title: 'Inference Testing Interface',
  content: {
    whatAppears: 'Interactive prompt interface with adapter selection dropdown',
    why: 'Test adapter behavior before production deployment',
    context: 'Quality assurance stage',
    mockComponent: 'MLEngineerInferenceTest'
  }
};

// DevOps Engineer Stages
const devOpsStage1: Stage = {
  id: 'server-config',
  title: 'Server Configuration Panel',
  content: {
    whatAppears: 'Configuration editor with deployment profiles (dev/staging/prod)',
    why: 'Set up production-ready server instances with proper policies',
    context: 'Infrastructure provisioning phase',
    mockComponent: 'DevOpsServerConfig'
  }
};

const devOpsStage2: Stage = {
  id: 'resource-management',
  title: 'Resource Management Dashboard',
  content: {
    whatAppears: 'Memory usage graphs, eviction policy controls, GPU allocation meters',
    why: 'Monitor and optimize resource utilization across tenants',
    context: 'Ongoing operations management',
    mockComponent: 'DevOpsResourceDashboard'
  }
};

const devOpsStage3: Stage = {
  id: 'deployment-pipeline',
  title: 'Deployment Pipeline Interface',
  content: {
    whatAppears: 'CI/CD integration panel with adapter deployment workflows',
    why: 'Automate safe deployment of new adapter versions',
    context: 'Release management',
    mockComponent: 'DevOpsCIDCPanel'
  }
};

const devOpsStage4: Stage = {
  id: 'monitoring-alerting',
  title: 'Monitoring & Alerting Center',
  content: {
    whatAppears: 'System metrics dashboard with configurable alerts and SLO tracking',
    why: 'Ensure system reliability and performance SLAs',
    context: 'Production operations',
    mockComponent: 'DevOpsMonitoringDashboard'
  }
};

// Application Developer Stages
const appDevStage1: Stage = {
  id: 'api-documentation',
  title: 'API Documentation Browser',
  content: {
    whatAppears: 'Interactive API docs with code examples in multiple languages',
    why: 'Understand integration patterns and available endpoints',
    context: 'Initial integration planning',
    mockComponent: 'AppDevAPIDocs'
  }
};

const appDevStage2: Stage = {
  id: 'sdk-manager',
  title: 'Client SDK Manager',
  content: {
    whatAppears: 'Package manager interface for downloading client libraries',
    why: 'Get the right SDK for the target platform (Node.js, Python, Go)',
    context: 'Development environment setup',
    mockComponent: 'AppDevSDKManager'
  }
};

const appDevStage3: Stage = {
  id: 'integration-testing',
  title: 'Integration Testing Console',
  content: {
    whatAppears: 'API testing interface with request/response panels',
    why: 'Validate integration and handle error scenarios',
    context: 'Development and debugging',
    mockComponent: 'AppDevTestConsole'
  }
};

const appDevStage4: Stage = {
  id: 'performance-optimization',
  title: 'Performance Optimization Panel',
  content: {
    whatAppears: 'Latency graphs, throughput meters, cost calculators',
    why: 'Optimize application performance and costs',
    context: 'Production optimization',
    mockComponent: 'AppDevPerformancePanel'
  }
};

// Security Engineer Stages
const securityStage1: Stage = {
  id: 'policy-config',
  title: 'Policy Configuration Studio',
  content: {
    whatAppears: 'Policy pack editor with rule builder and validation tools',
    why: 'Define and enforce security policies across the system',
    context: 'Security policy definition',
    mockComponent: 'SecurityPolicyEditor'
  }
};

const securityStage2: Stage = {
  id: 'evidence-audit',
  title: 'Evidence Audit Trail Viewer',
  content: {
    whatAppears: 'Timeline of policy decisions with detailed evidence logs',
    why: 'Audit compliance and investigate security incidents',
    context: 'Compliance monitoring and incident response',
    mockComponent: 'SecurityAuditTrail'
  }
};

const securityStage3: Stage = {
  id: 'isolation-testing',
  title: 'Isolation Testing Interface',
  content: {
    whatAppears: 'Tenant sandbox controls and isolation verification tools',
    why: 'Test and validate tenant separation mechanisms',
    context: 'Security validation',
    mockComponent: 'SecurityIsolationTester'
  }
};

const securityStage4: Stage = {
  id: 'threat-detection',
  title: 'Threat Detection Dashboard',
  content: {
    whatAppears: 'Real-time security event monitoring with anomaly detection',
    why: 'Identify and respond to potential security threats',
    context: 'Ongoing security operations',
    mockComponent: 'SecurityThreatDashboard'
  }
};

// Data Scientist Stages
const dataScientistStage1: Stage = {
  id: 'experiment-tracking',
  title: 'Experiment Tracking Interface',
  content: {
    whatAppears: 'Experiment comparison dashboard with A/B testing controls',
    why: 'Track and compare different adapter configurations',
    context: 'Research and experimentation phase',
    mockComponent: 'DataScientistExperimentTracker'
  }
};

const dataScientistStage2: Stage = {
  id: 'dataset-management',
  title: 'Dataset Management Portal',
  content: {
    whatAppears: 'Data upload interface with preprocessing pipeline controls',
    why: 'Prepare and validate training data for adapter creation',
    context: 'Data preparation stage',
    mockComponent: 'DataScientistDatasetManager'
  }
};

const dataScientistStage3: Stage = {
  id: 'evaluation-framework',
  title: 'Evaluation Framework UI',
  content: {
    whatAppears: 'Benchmark suite with custom metric definitions',
    why: 'Measure adapter performance against baseline models',
    context: 'Model validation',
    mockComponent: 'DataScientistEvaluationUI'
  }
};

const dataScientistStage4: Stage = {
  id: 'collaboration-hub',
  title: 'Collaboration Hub',
  content: {
    whatAppears: 'Shared workspace with team notebooks and adapter sharing',
    why: 'Collaborate on research findings and model improvements',
    context: 'Team collaboration',
    mockComponent: 'DataScientistCollaborationHub'
  }
};

// Product Manager Stages
const productManagerStage1: Stage = {
  id: 'feature-analytics',
  title: 'Feature Usage Analytics',
  content: {
    whatAppears: 'Adoption dashboards with user behavior metrics',
    why: 'Understand feature utilization and identify improvement opportunities',
    context: 'Product planning and prioritization',
    mockComponent: 'ProductManagerUsageAnalytics'
  }
};

const productManagerStage2: Stage = {
  id: 'system-performance',
  title: 'System Performance Overview',
  content: {
    whatAppears: 'Business metrics dashboard with uptime, latency, and user satisfaction KPIs',
    why: 'Monitor overall system health and business impact',
    context: 'Executive reporting',
    mockComponent: 'ProductManagerPerformanceOverview'
  }
};

const productManagerStage3: Stage = {
  id: 'config-management',
  title: 'Configuration Management Portal',
  content: {
    whatAppears: 'Tenant configuration templates and deployment scenario builder',
    why: 'Define and manage different service tiers and configurations',
    context: 'Product configuration management',
    mockComponent: 'ProductManagerConfigPortal'
  }
};

const productManagerStage4: Stage = {
  id: 'feedback-integration',
  title: 'Feedback Integration Hub',
  content: {
    whatAppears: 'User feedback collection and feature request management system',
    why: 'Gather and prioritize user requirements for product roadmap',
    context: 'Product development planning',
    mockComponent: 'ProductManagerFeedbackHub'
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
