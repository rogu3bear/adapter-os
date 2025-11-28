import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Alert, AlertDescription } from './ui/alert';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '@/layout/LayoutProvider';
import { getRoleGuidance } from '@/data/role-guidance';
import { BookOpen, ArrowRight, Lightbulb, GraduationCap } from 'lucide-react';
import type { UserRole } from '@/api/types';
import { useContextualTutorial } from '@/hooks/useContextualTutorial';
import { ContextualTutorial } from './ContextualTutorial';

interface PageGuidance {
  title: string;
  tips: string[];
  relatedPages: Array<{
    label: string;
    route: string;
    description: string;
  }>;
}

const pageGuidanceMap: Record<string, Record<UserRole, PageGuidance>> = {
  '/dashboard': {
    admin: {
      title: 'System Dashboard',
      tips: [
        'Monitor overall system health and resource utilization',
        'Check adapter deployment status across organizations',
        'Review recent alerts and system events'
      ],
      relatedPages: [
        { label: 'System Health', route: '/metrics', description: 'Detailed metrics and monitoring' },
        { label: 'Alerts', route: '/metrics', description: 'View active alerts' }
      ]
    },
    operator: {
      title: 'ML Pipeline Overview',
      tips: [
        'Monitor training jobs and adapter deployments',
        'Track pipeline status from training to production',
        'Review performance metrics for active adapters'
      ],
      relatedPages: [
        { label: 'Training Jobs', route: '/training', description: 'Manage training workflows' },
        { label: 'Adapters', route: '/adapters', description: 'View deployed adapters' }
      ]
    },
    sre: {
      title: 'System Health Dashboard',
      tips: [
        'Monitor resource utilization across nodes',
        'Check system capacity and performance trends',
        'Identify bottlenecks before they impact operations'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Detailed system metrics' },
        { label: 'Observability', route: '/observability', description: 'Deep system insights' }
      ]
    },
    compliance: {
      title: 'Compliance Overview',
      tips: [
        'Monitor policy compliance status',
        'Review system-wide compliance metrics',
        'Check for policy violations'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Review policy configurations' },
        { label: 'Audit', route: '/security/audit', description: 'Access audit trails' }
      ]
    },
    auditor: {
      title: 'Audit Dashboard',
      tips: [
        'Review system activity overview',
        'Monitor compliance metrics',
        'Track policy enforcement status'
      ],
      relatedPages: [
        { label: 'Audit Trails', route: '/security/audit', description: 'Detailed audit logs' },
        { label: 'Policies', route: '/security/policies', description: 'Policy configurations' }
      ]
    },
    viewer: {
      title: 'System Overview',
      tips: [
        'View system status and health metrics',
        'Monitor adapter deployment counts',
        'Review recent system activity'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'View adapter status' },
        { label: 'Inference', route: '/inference', description: 'Try inference playground' }
      ]
    }
  },
  '/adapters': {
    admin: {
      title: 'Adapter Management',
      tips: [
        'Manage adapters across all organizations',
        'Review deployment status and resource usage',
        'Monitor adapter performance metrics'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Create new adapters' },
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' }
      ]
    },
    operator: {
      title: 'Adapter Management',
      tips: [
        'Deploy trained adapters to production',
        'Monitor adapter health and performance',
        'Manage adapter lifecycle and versions'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Train new adapters' },
        { label: 'Testing', route: '/testing', description: 'Test adapters before deployment' },
        { label: 'Inference', route: '/inference', description: 'Test adapter performance' }
      ]
    },
    sre: {
      title: 'Adapter Operations',
      tips: [
        'Monitor adapter resource consumption',
        'Track adapter performance and errors',
        'Identify adapters causing issues'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Detailed performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Deep diagnostics' }
      ]
    },
    compliance: {
      title: 'Adapter Compliance',
      tips: [
        'Review adapter deployment compliance',
        'Verify adapter security configurations',
        'Monitor adapter policy adherence'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Review policies' },
        { label: 'Audit', route: '/security/audit', description: 'Adapter audit logs' }
      ]
    },
    auditor: {
      title: 'Adapter Audit',
      tips: [
        'Review adapter deployment history',
        'Verify adapter configurations',
        'Export adapter audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Adapter Status',
      tips: [
        'View adapter deployment status',
        'Monitor adapter health metrics',
        'Check adapter performance'
      ],
      relatedPages: [
        { label: 'Inference', route: '/inference', description: 'Test adapters' }
      ]
    }
  },
  '/admin/tenants': {
    admin: {
      title: 'Organization Management',
      tips: [
        'Create and manage organization workspaces',
        'Configure organization isolation and resources',
        'Monitor organization activity and usage'
      ],
      relatedPages: [
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' },
        { label: 'Admin', route: '/admin', description: 'Admin settings' }
      ]
    },
    operator: { title: '', tips: [], relatedPages: [] },
    sre: {
      title: 'Organization Infrastructure',
      tips: [
        'Monitor organization resource allocation',
        'Review organization isolation status',
        'Check organization node assignments'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Resource metrics' }
      ]
    },
    compliance: {
      title: 'Organization Compliance',
      tips: [
        'Review organization data classification',
        'Verify organization isolation compliance',
        'Check organization policy assignments'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Organization policies' },
        { label: 'Audit', route: '/security/audit', description: 'Organization audit logs' }
      ]
    },
    auditor: {
      title: 'Organization Audit',
      tips: [
        'Review organization configuration history',
        'Verify organization access controls',
        'Export organization audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/security/policies': {
    admin: {
      title: 'Policy Management',
      tips: [
        'Review all 20 policy packs regularly',
        'Sign policies after review and verification',
        'Compare policy versions before updating'
      ],
      relatedPages: [
        { label: 'Telemetry', route: '/telemetry', description: 'View policy enforcement logs' },
        { label: 'Audit Trails', route: '/security/audit', description: 'Review policy changes' }
      ]
    },
    compliance: {
      title: 'Compliance Review',
      tips: [
        'Verify all 20 policy packs are compliant',
        'Export policy attestations for audit',
        'Monitor policy violations and enforcement'
      ],
      relatedPages: [
        { label: 'Audit Trails', route: '/security/audit', description: 'Review policy audit trail' },
        { label: 'Telemetry', route: '/telemetry', description: 'Export compliance data' }
      ]
    },
    auditor: {
      title: 'Policy Audit',
      tips: [
        'Verify policy signatures and authenticity',
        'Review policy change history',
        'Export policy configurations for review'
      ],
      relatedPages: [
        { label: 'Audit Trails', route: '/security/audit', description: 'Full audit history' }
      ]
    },
    operator: {
      title: 'Policy Overview',
      tips: [
        'View active policies affecting your operations',
        'Understand policy constraints on adapters',
        'Check policy enforcement status'
      ],
      relatedPages: [
        { label: 'Telemetry', route: '/telemetry', description: 'Policy enforcement logs' }
      ]
    },
    sre: {
      title: 'Policy Operations',
      tips: [
        'Monitor policy enforcement impact on performance',
        'Review policy-related system events',
        'Check policy violation alerts'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'System metrics' },
        { label: 'Telemetry', route: '/telemetry', description: 'Policy events' }
      ]
    },
    viewer: {
      title: 'Policy Information',
      tips: [
        'View policy configurations and status',
        'Understand system policy framework',
        'Review policy documentation'
      ],
      relatedPages: []
    }
  },
  '/metrics': {
    admin: {
      title: 'System Metrics',
      tips: [
        'Monitor system-wide performance metrics',
        'Track resource utilization trends',
        'Set up alerts for critical thresholds'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'Deep system insights' },
        { label: 'Dashboard', route: '/dashboard', description: 'Overview' }
      ]
    },
    operator: {
      title: 'Performance Metrics',
      tips: [
        'Monitor adapter performance metrics',
        'Track training job progress',
        'Review inference latency and throughput'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Training metrics' },
        { label: 'Inference', route: '/inference', description: 'Test performance' }
      ]
    },
    sre: {
      title: 'System Monitoring',
      tips: [
        'Monitor resource utilization across nodes',
        'Track system capacity and bottlenecks',
        'Set up proactive alerts for issues'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'Deep diagnostics' },
        { label: 'Routing', route: '/routing', description: 'Routing performance' }
      ]
    },
    compliance: {
      title: 'Compliance Metrics',
      tips: [
        'Monitor policy compliance metrics',
        'Track compliance violations',
        'Review audit event frequency'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy configurations' },
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' }
      ]
    },
    auditor: {
      title: 'Audit Metrics',
      tips: [
        'Review system activity metrics',
        'Monitor audit event volumes',
        'Track compliance trends'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Detailed audit logs' }
      ]
    },
    viewer: {
      title: 'System Metrics',
      tips: [
        'View system performance overview',
        'Monitor key health indicators',
        'Review metric trends'
      ],
      relatedPages: []
    }
  },
  '/telemetry': {
    admin: {
      title: 'Telemetry Management',
      tips: [
        'Monitor system-wide telemetry bundles',
        'Review telemetry export and retention policies',
        'Track telemetry bundle sizes and frequency'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' },
        { label: 'Metrics', route: '/metrics', description: 'System metrics' }
      ]
    },
    operator: {
      title: 'Telemetry Monitoring',
      tips: [
        'Review adapter telemetry events',
        'Monitor training job telemetry',
        'Track inference request telemetry'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Adapter events' },
        { label: 'Training', route: '/training', description: 'Training telemetry' }
      ]
    },
    sre: {
      title: 'System Telemetry',
      tips: [
        'Monitor infrastructure telemetry',
        'Review node and worker telemetry',
        'Track system performance events'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'Deep insights' },
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' }
      ]
    },
    compliance: {
      title: 'Compliance Telemetry',
      tips: [
        'Export telemetry bundles for audit',
        'Review policy enforcement telemetry',
        'Monitor compliance event telemetry'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy events' },
        { label: 'Audit', route: '/security/audit', description: 'Audit logs' }
      ]
    },
    auditor: {
      title: 'Telemetry Audit',
      tips: [
        'Review telemetry bundle integrity',
        'Verify telemetry event completeness',
        'Export telemetry for external analysis'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' }
      ]
    },
    viewer: {
      title: 'Telemetry Overview',
      tips: [
        'View telemetry event summaries',
        'Monitor telemetry bundle status',
        'Review telemetry trends'
      ],
      relatedPages: []
    }
  },
  '/observability': {
    admin: {
      title: 'System Observability',
      tips: [
        'Access deep system diagnostics and logs',
        'Monitor distributed system traces',
        'Review system-wide performance patterns'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'System metrics' },
        { label: 'Telemetry', route: '/telemetry', description: 'Telemetry bundles' }
      ]
    },
    operator: {
      title: 'Pipeline Observability',
      tips: [
        'Trace adapter execution flows',
        'Monitor training pipeline observability',
        'Review inference request traces'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Training observability' },
        { label: 'Inference', route: '/inference', description: 'Inference traces' }
      ]
    },
    sre: {
      title: 'Deep System Diagnostics',
      tips: [
        'Access detailed system logs and traces',
        'Perform root cause analysis with observability data',
        'Monitor system performance deep dives'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Routing', route: '/routing', description: 'Routing diagnostics' }
      ]
    },
    compliance: { title: '', tips: [], relatedPages: [] },
    auditor: {
      title: 'Observability Audit',
      tips: [
        'Review system observability logs',
        'Export observability data for audit',
        'Verify system trace integrity'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' }
      ]
    },
    viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/inference': {
    admin: {
      title: 'Inference Testing',
      tips: [
        'Test adapters across all organizations',
        'Monitor inference performance and resource usage',
        'Review inference request patterns'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Manage adapters' },
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' }
      ]
    },
    operator: {
      title: 'Inference Playground',
      tips: [
        'Test adapter performance in real-time',
        'Validate adapter outputs before deployment',
        'Compare adapter responses'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'View adapters' },
        { label: 'Testing', route: '/testing', description: 'Formal testing' }
      ]
    },
    sre: {
      title: 'Inference Monitoring',
      tips: [
        'Monitor inference performance and latency',
        'Track inference resource consumption',
        'Identify inference bottlenecks'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Routing', route: '/routing', description: 'Routing performance' }
      ]
    },
    compliance: {
      title: 'Inference Compliance',
      tips: [
        'Verify inference policy compliance',
        'Review inference audit logs',
        'Monitor inference data handling'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy configurations' },
        { label: 'Audit', route: '/security/audit', description: 'Inference audit logs' }
      ]
    },
    auditor: {
      title: 'Inference Audit',
      tips: [
        'Review inference request history',
        'Verify inference compliance',
        'Export inference audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Inference Testing',
      tips: [
        'Try the inference playground',
        'Test adapter responses',
        'Explore adapter capabilities'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'View adapters' }
      ]
    }
  },
  '/security/audit': {
    admin: {
      title: 'Audit Trails',
      tips: [
        'Review all system audit events',
        'Monitor system changes and access',
        'Export audit data for compliance'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy audit logs' },
        { label: 'Telemetry', route: '/telemetry', description: 'Telemetry audit' }
      ]
    },
    operator: {
      title: 'Operation Audit',
      tips: [
        'Review adapter operation audit logs',
        'Track training job audit trails',
        'Monitor inference request audit logs'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Adapter operations' },
        { label: 'Training', route: '/training', description: 'Training logs' }
      ]
    },
    sre: {
      title: 'System Audit',
      tips: [
        'Review infrastructure audit logs',
        'Monitor system configuration changes',
        'Track security-related audit events'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'System logs' },
        { label: 'Metrics', route: '/metrics', description: 'Performance logs' }
      ]
    },
    compliance: {
      title: 'Compliance Audit',
      tips: [
        'Review compliance audit trails',
        'Export audit data for compliance reporting',
        'Monitor policy enforcement audit logs'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy audit' },
        { label: 'Telemetry', route: '/telemetry', description: 'Export telemetry' }
      ]
    },
    auditor: {
      title: 'Comprehensive Audit',
      tips: [
        'Access complete audit trail history',
        'Export audit data for external analysis',
        'Verify audit log integrity and completeness'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy audit' },
        { label: 'Reports', route: '/reports', description: 'Audit reports' }
      ]
    },
    viewer: {
      title: 'Audit Overview',
      tips: [
        'View audit event summaries',
        'Review recent audit activity',
        'Understand system audit framework'
      ],
      relatedPages: []
    }
  },
  '/base-models': {
    admin: {
      title: 'Base Model Management',
      tips: [
        'Manage base model configurations',
        'Review base model resource allocation',
        'Monitor base model usage across organizations'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'View adapter models' },
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' }
      ]
    },
    operator: {
      title: 'Base Models',
      tips: [
        'View available base models for training',
        'Check base model compatibility',
        'Review base model specifications'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Start training' },
        { label: 'Adapters', route: '/adapters', description: 'Adapter models' }
      ]
    },
    sre: {
      title: 'Model Infrastructure',
      tips: [
        'Monitor base model resource usage',
        'Track model deployment status',
        'Review model performance metrics'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' }
      ]
    },
    compliance: { title: '', tips: [], relatedPages: [] },
    auditor: { title: '', tips: [], relatedPages: [] },
    viewer: {
      title: 'Base Models',
      tips: [
        'View available base models',
        'Review model specifications',
        'Understand model capabilities'
      ],
      relatedPages: []
    }
  },
  '/workflow': {
    admin: {
      title: 'Getting Started',
      tips: [
        'Complete the workflow wizard to understand system setup',
        'Review recommended workflows for administrators',
        'Track your onboarding progress'
      ],
      relatedPages: [
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' },
        { label: 'Admin', route: '/admin', description: 'Admin settings' }
      ]
    },
    operator: {
      title: 'ML Operations Workflow',
      tips: [
        'Follow the ML pipeline workflow guide',
        'Complete workflow steps to set up your pipeline',
        'Track your progress through the workflow'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Start training' },
        { label: 'Dashboard', route: '/dashboard', description: 'Overview' }
      ]
    },
    sre: {
      title: 'Site Reliability Workflow',
      tips: [
        'Complete the SRE workflow to understand monitoring',
        'Set up system health monitoring',
        'Track your operational workflow progress'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'System metrics' },
        { label: 'Observability', route: '/observability', description: 'Deep insights' }
      ]
    },
    compliance: {
      title: 'Compliance Workflow',
      tips: [
        'Complete the compliance officer workflow',
        'Review compliance setup steps',
        'Track compliance workflow progress'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Review policies' },
        { label: 'Audit', route: '/security/audit', description: 'Audit setup' }
      ]
    },
    auditor: {
      title: 'Audit Workflow',
      tips: [
        'Complete the auditor workflow',
        'Set up audit review processes',
        'Track audit workflow progress'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' },
        { label: 'Policies', route: '/security/policies', description: 'Policy review' }
      ]
    },
    viewer: {
      title: 'Getting Started',
      tips: [
        'Complete the viewer workflow guide',
        'Learn how to navigate the system',
        'Track your onboarding progress'
      ],
      relatedPages: [
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' }
      ]
    }
  },
  '/training': {
    operator: {
      title: 'Training Adapters',
      tips: [
        'Start with a template to get going quickly',
        'Monitor training metrics in real-time',
        'Save checkpoints regularly for long training runs',
        'Use the training wizard for guided setup'
      ],
      relatedPages: [
        { label: 'Test & Validate', route: '/testing', description: 'Test your trained adapter' },
        { label: 'Deploy & Manage', route: '/adapters', description: 'Deploy after validation' }
      ]
    },
    admin: {
      title: 'Training Management',
      tips: [
        'Review training resource allocation',
        'Monitor system capacity during training',
        'Set up training resource limits'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Resource metrics' },
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' }
      ]
    },
    sre: {
      title: 'Training Operations',
      tips: [
        'Monitor training job resource consumption',
        'Set up alerts for training failures',
        'Review training performance metrics'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Training diagnostics' }
      ]
    },
    compliance: {
      title: 'Training Compliance',
      tips: [
        'Verify training data compliance',
        'Review training audit logs',
        'Monitor training policy adherence'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Training policies' },
        { label: 'Audit', route: '/security/audit', description: 'Training audit logs' }
      ]
    },
    auditor: {
      title: 'Training Audit',
      tips: [
        'Review training job audit logs',
        'Verify training data handling',
        'Export training audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Training Overview',
      tips: [
        'View training job status',
        'Monitor training progress',
        'Review training metrics'
      ],
      relatedPages: []
    }
  },
  '/testing': {
    operator: {
      title: 'Testing & Validation',
      tips: [
        'Run golden baseline comparisons before promoting',
        'Check epsilon metrics for numerical accuracy',
        'Validate on diverse test cases',
        'Review test results carefully before promotion'
      ],
      relatedPages: [
        { label: 'Compare Baselines', route: '/golden', description: 'Compare with golden runs' },
        { label: 'Promote', route: '/promotion', description: 'Promote after passing tests' }
      ]
    },
    admin: {
      title: 'Testing Management',
      tips: [
        'Review testing resource allocation',
        'Monitor test execution metrics',
        'Set up test quality gates'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Test metrics' },
        { label: 'Golden Runs', route: '/golden', description: 'Baseline comparisons' }
      ]
    },
    sre: {
      title: 'Testing Operations',
      tips: [
        'Monitor test execution performance',
        'Review test failure patterns',
        'Track test resource usage'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Test diagnostics' }
      ]
    },
    compliance: {
      title: 'Testing Compliance',
      tips: [
        'Verify test data compliance',
        'Review testing audit logs',
        'Monitor test policy adherence'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Testing policies' },
        { label: 'Audit', route: '/security/audit', description: 'Testing audit logs' }
      ]
    },
    auditor: {
      title: 'Testing Audit',
      tips: [
        'Review test execution audit logs',
        'Verify test data handling',
        'Export testing audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Testing Overview',
      tips: [
        'View test execution status',
        'Monitor test results',
        'Review test metrics'
      ],
      relatedPages: []
    }
  },
  '/golden': {
    operator: {
      title: 'Golden Baseline Comparisons',
      tips: [
        'Compare adapter outputs against golden baselines',
        'Review epsilon and numerical accuracy metrics',
        'Use golden runs for regression testing',
        'Ensure baselines match expected outputs before promotion'
      ],
      relatedPages: [
        { label: 'Testing', route: '/testing', description: 'Run tests' },
        { label: 'Promotion', route: '/promotion', description: 'Promote after validation' }
      ]
    },
    admin: {
      title: 'Baseline Management',
      tips: [
        'Manage golden baseline versions',
        'Review baseline comparison metrics',
        'Set up baseline quality standards'
      ],
      relatedPages: [
        { label: 'Testing', route: '/testing', description: 'Test execution' },
        { label: 'Metrics', route: '/metrics', description: 'Comparison metrics' }
      ]
    },
    sre: {
      title: 'Baseline Operations',
      tips: [
        'Monitor baseline comparison performance',
        'Review baseline storage usage',
        'Track baseline update frequency'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' }
      ]
    },
    compliance: { title: '', tips: [], relatedPages: [] },
    auditor: {
      title: 'Baseline Audit',
      tips: [
        'Review baseline change history',
        'Verify baseline integrity',
        'Export baseline audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Golden Baselines',
      tips: [
        'View golden baseline status',
        'Review baseline comparisons',
        'Understand baseline metrics'
      ],
      relatedPages: []
    }
  },
  '/promotion': {
    operator: {
      title: 'Adapter Promotion',
      tips: [
        'Promote tested adapters through quality gates',
        'Review promotion criteria before promoting',
        'Verify all tests pass before promotion',
        'Monitor promotion success rates'
      ],
      relatedPages: [
        { label: 'Testing', route: '/testing', description: 'Run tests first' },
        { label: 'Adapters', route: '/adapters', description: 'Manage promoted adapters' }
      ]
    },
    admin: {
      title: 'Promotion Management',
      tips: [
        'Configure promotion quality gates',
        'Review promotion policies',
        'Monitor promotion metrics'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Promotion policies' },
        { label: 'Metrics', route: '/metrics', description: 'Promotion metrics' }
      ]
    },
    sre: {
      title: 'Promotion Operations',
      tips: [
        'Monitor promotion execution performance',
        'Review promotion failure patterns',
        'Track promotion resource usage'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Promotion diagnostics' }
      ]
    },
    compliance: {
      title: 'Promotion Compliance',
      tips: [
        'Verify promotion policy compliance',
        'Review promotion audit logs',
        'Monitor promotion approvals'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Promotion policies' },
        { label: 'Audit', route: '/security/audit', description: 'Promotion audit logs' }
      ]
    },
    auditor: {
      title: 'Promotion Audit',
      tips: [
        'Review promotion audit logs',
        'Verify promotion approvals',
        'Export promotion audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Promotion Overview',
      tips: [
        'View promotion status',
        'Monitor promotion progress',
        'Review promotion history'
      ],
      relatedPages: []
    }
  },
  '/routing': {
    admin: {
      title: 'Routing Management',
      tips: [
        'Monitor routing performance across all adapters',
        'Review routing configuration and policies',
        'Track routing resource usage'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Adapter routing' },
        { label: 'Metrics', route: '/metrics', description: 'Routing metrics' }
      ]
    },
    operator: {
      title: 'Routing Inspector',
      tips: [
        'Inspect routing decisions for your adapters',
        'Review routing performance metrics',
        'Understand adapter selection logic'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Manage adapters' },
        { label: 'Inference', route: '/inference', description: 'Test routing' }
      ]
    },
    sre: {
      title: 'Routing Diagnostics',
      tips: [
        'Monitor routing performance and latency',
        'Identify routing bottlenecks',
        'Review K-sparse routing efficiency',
        'Track router resource consumption'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Routing diagnostics' }
      ]
    },
    compliance: { title: '', tips: [], relatedPages: [] },
    auditor: {
      title: 'Routing Audit',
      tips: [
        'Review routing decision audit logs',
        'Verify routing policy compliance',
        'Export routing audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Routing Overview',
      tips: [
        'View routing performance',
        'Monitor routing metrics',
        'Understand routing behavior'
      ],
      relatedPages: []
    }
  },
  '/replay': {
    admin: {
      title: 'Deterministic Replay',
      tips: [
        'Replay system events for debugging',
        'Use replay for reproducing issues',
        'Verify deterministic execution'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'System diagnostics' },
        { label: 'Audit', route: '/security/audit', description: 'Event history' }
      ]
    },
    operator: {
      title: 'Event Replay',
      tips: [
        'Replay adapter execution events',
        'Use replay to debug adapter issues',
        'Verify adapter deterministic behavior'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Adapter management' },
        { label: 'Inference', route: '/inference', description: 'Test replay' }
      ]
    },
    sre: {
      title: 'System Replay',
      tips: [
        'Replay system events for diagnostics',
        'Use replay for incident analysis',
        'Verify system deterministic execution'
      ],
      relatedPages: [
        { label: 'Observability', route: '/observability', description: 'System diagnostics' },
        { label: 'Metrics', route: '/metrics', description: 'Performance analysis' }
      ]
    },
    compliance: {
      title: 'Replay Compliance',
      tips: [
        'Review replay audit logs',
        'Verify replay data handling compliance',
        'Monitor replay usage'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Replay audit logs' }
      ]
    },
    auditor: {
      title: 'Replay Audit',
      tips: [
        'Review replay execution audit logs',
        'Verify replay integrity',
        'Export replay audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/admin': {
    admin: {
      title: 'System Administration',
      tips: [
        'Configure system-wide settings',
        'Manage infrastructure nodes',
        'Review system configuration',
        'Set up system alerts and monitoring'
      ],
      relatedPages: [
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' },
        { label: 'Metrics', route: '/metrics', description: 'System metrics' },
        { label: 'Organizations', route: '/admin/tenants', description: 'Organization management' }
      ]
    },
    operator: { title: '', tips: [], relatedPages: [] },
    sre: {
      title: 'Infrastructure Administration',
      tips: [
        'Manage infrastructure nodes',
        'Configure system monitoring',
        'Review infrastructure settings'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Infrastructure metrics' },
        { label: 'Observability', route: '/observability', description: 'System diagnostics' }
      ]
    },
    compliance: { title: '', tips: [], relatedPages: [] },
    auditor: { title: '', tips: [], relatedPages: [] },
    viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/reports': {
    admin: {
      title: 'System Reports',
      tips: [
        'Generate comprehensive system reports',
        'Export reports for analysis',
        'Schedule automated reports',
        'Review system-wide metrics and trends'
      ],
      relatedPages: [
        { label: 'Dashboard', route: '/dashboard', description: 'System overview' },
        { label: 'Metrics', route: '/metrics', description: 'Detailed metrics' }
      ]
    },
    operator: {
      title: 'Operational Reports',
      tips: [
        'Generate adapter performance reports',
        'Export training reports',
        'Review operational metrics'
      ],
      relatedPages: [
        { label: 'Adapters', route: '/adapters', description: 'Adapter data' },
        { label: 'Training', route: '/training', description: 'Training data' }
      ]
    },
    sre: {
      title: 'Infrastructure Reports',
      tips: [
        'Generate system performance reports',
        'Export infrastructure metrics',
        'Review system health trends'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'System data' }
      ]
    },
    compliance: {
      title: 'Compliance Reports',
      tips: [
        'Generate compliance reports',
        'Export compliance data for audit',
        'Review policy compliance trends'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Policy data' },
        { label: 'Audit', route: '/security/audit', description: 'Audit data' }
      ]
    },
    auditor: {
      title: 'Audit Reports',
      tips: [
        'Generate comprehensive audit reports',
        'Export audit data for analysis',
        'Review audit trends and patterns'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Audit trails' }
      ]
    },
    viewer: {
      title: 'Report Overview',
      tips: [
        'View available reports',
        'Review report summaries',
        'Access read-only reports'
      ],
      relatedPages: []
    }
  },
  '/trainer': {
    operator: {
      title: 'Single-File Trainer',
      tips: [
        'Use the single-file trainer for quick adapter training',
        'Upload your training data file',
        'Configure training parameters',
        'Monitor training progress'
      ],
      relatedPages: [
        { label: 'Training Jobs', route: '/training', description: 'Full training workflow' },
        { label: 'Adapters', route: '/adapters', description: 'Manage trained adapters' }
      ]
    },
    admin: {
      title: 'Training Management',
      tips: [
        'Monitor single-file training jobs',
        'Review training resource usage',
        'Track training completion rates'
      ],
      relatedPages: [
        { label: 'Training', route: '/training', description: 'Training management' },
        { label: 'Metrics', route: '/metrics', description: 'Resource metrics' }
      ]
    },
    sre: {
      title: 'Training Operations',
      tips: [
        'Monitor training job performance',
        'Review training resource consumption',
        'Track training failures'
      ],
      relatedPages: [
        { label: 'Metrics', route: '/metrics', description: 'Performance metrics' },
        { label: 'Observability', route: '/observability', description: 'Training diagnostics' }
      ]
    },
    compliance: {
      title: 'Training Compliance',
      tips: [
        'Verify training data compliance',
        'Review training audit logs',
        'Monitor training policy adherence'
      ],
      relatedPages: [
        { label: 'Policies', route: '/security/policies', description: 'Training policies' },
        { label: 'Audit', route: '/security/audit', description: 'Training audit logs' }
      ]
    },
    auditor: {
      title: 'Training Audit',
      tips: [
        'Review training job audit logs',
        'Verify training data handling',
        'Export training audit data'
      ],
      relatedPages: [
        { label: 'Audit', route: '/security/audit', description: 'Full audit trail' }
      ]
    },
    viewer: {
      title: 'Training Overview',
      tips: [
        'View training job status',
        'Monitor training progress',
        'Review training metrics'
      ],
      relatedPages: []
    }
  }
};

export function ContextualHelp() {
  const location = useLocation();
  const navigate = useNavigate();
  const { user } = useAuth();
  const { activeTutorial, isOpen, availableTutorials, startTutorial, closeTutorial, completeTutorial } = useContextualTutorial(location.pathname);

  if (!user) return null;

  const roleGuidance = getRoleGuidance(user.role);
  const pageGuidance = pageGuidanceMap[location.pathname]?.[user.role];

  // Show generic help if no page-specific guidance exists
  const showGenericHelp = !pageGuidance || (pageGuidance.title === '' && pageGuidance.tips.length === 0 && pageGuidance.relatedPages.length === 0);
  
  if (showGenericHelp) {
    // Try to find generic guidance for the page
    const allRolesGuidance = pageGuidanceMap[location.pathname];
    const hasAnyGuidance = allRolesGuidance && Object.values(allRolesGuidance).some(
      g => g.title !== '' || g.tips.length > 0 || g.relatedPages.length > 0
    );
    
    // Don't show if no guidance exists for any role on this page
    if (!hasAnyGuidance) {
      return (
        <>
          {activeTutorial && (
            <ContextualTutorial
              config={activeTutorial}
              open={isOpen}
              onClose={closeTutorial}
              onComplete={completeTutorial}
            />
          )}
        </>
      );
    }
  }

  return (
    <>
      <Card className="border-blue-200 bg-blue-50/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Lightbulb className="h-5 w-5 text-blue-600" />
            {showGenericHelp ? 'Need Help?' : pageGuidance?.title}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Tips */}
          {!showGenericHelp && pageGuidance && pageGuidance.tips.length > 0 && (
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Quick Tips:</p>
              <ul className="space-y-1">
                {pageGuidance.tips.map((tip, idx) => (
                  <li key={idx} className="flex items-start gap-2 text-sm">
                    <span className="text-blue-600 mt-0.5">•</span>
                    <span>{tip}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Related Pages */}
          {!showGenericHelp && pageGuidance && pageGuidance.relatedPages.length > 0 && (
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Next Steps:</p>
              <div className="space-y-2">
                {pageGuidance.relatedPages.map((page) => (
                  <Button
                    key={page.route}
                    variant="ghost"
                    size="sm"
                    className="w-full justify-start h-auto py-2"
                    onClick={() => navigate(page.route)}
                  >
                    <div className="flex-1 text-left">
                      <div className="font-medium text-sm">{page.label}</div>
                      <div className="text-xs text-muted-foreground">{page.description}</div>
                    </div>
                    <ArrowRight className="h-4 w-4 flex-shrink-0" />
                  </Button>
                ))}
              </div>
            </div>
          )}

          {/* Role-specific tip */}
          {roleGuidance && roleGuidance.tips.length > 0 && (
            <Alert>
              <BookOpen className="h-4 w-4" />
              <AlertDescription className="text-sm">
                <span className="font-medium">Role Tip: </span>
                {roleGuidance.tips[showGenericHelp ? 0 : Math.floor(Math.random() * roleGuidance.tips.length)]}
              </AlertDescription>
            </Alert>
          )}

          {/* Tutorial Button */}
          {availableTutorials.length > 0 && (
            <div className="pt-2 border-t">
              <Button
                variant="outline"
                size="sm"
                onClick={() => startTutorial()}
                className="w-full"
              >
                <GraduationCap className="h-4 w-4 mr-2" />
                Start Interactive Tutorial
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
      {activeTutorial && (
        <ContextualTutorial
          config={activeTutorial}
          open={isOpen}
          onClose={closeTutorial}
          onComplete={completeTutorial}
        />
      )}
    </>
  );
}

