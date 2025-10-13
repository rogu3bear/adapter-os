import { UserRole } from '../api/types';

export interface RoleGuidanceItem {
  role: UserRole;
  title: string;
  description: string;
  capabilities: string[];
  restrictions: string[];
  tips: string[];
}

export const roleGuidanceDatabase: RoleGuidanceItem[] = [
  {
    role: 'Admin',
    title: 'Administrator',
    description: 'Full system access with complete control over all aspects of the AdapterOS Control Plane.',
    capabilities: [
      'Manage all tenants and users',
      'Configure system settings',
      'Access all adapters and policies',
      'Execute all operations',
      'View all telemetry and logs',
      'Manage infrastructure nodes'
    ],
    restrictions: [],
    tips: [
      'Use Settings to configure system-wide parameters',
      'Monitor system health in Dashboard',
      'Review policies regularly for compliance',
      'Use Operations for runtime management'
    ]
  },
  {
    role: 'Operator',
    title: 'Operator',
    description: 'Operational control over runtime systems, adapters, and day-to-day management.',
    capabilities: [
      'Manage adapters and training',
      'Execute operations and plans',
      'Monitor system performance',
      'View telemetry and alerts',
      'Access inference playground'
    ],
    restrictions: [
      'Cannot modify system settings',
      'Cannot manage tenants',
      'Cannot change policies'
    ],
    tips: [
      'Focus on Adapters for model management',
      'Use Operations for runtime control',
      'Monitor Dashboard for system health',
      'Check Alerts for issues'
    ]
  },
  {
    role: 'Compliance',
    title: 'Compliance Officer',
    description: 'Oversight role focused on policy compliance, audit trails, and regulatory requirements.',
    capabilities: [
      'View and manage policies',
      'Access audit trails',
      'Monitor compliance status',
      'Review telemetry bundles',
      'View system alerts'
    ],
    restrictions: [
      'Cannot modify system settings',
      'Cannot manage adapters',
      'Cannot execute operations'
    ],
    tips: [
      'Focus on Policies for compliance management',
      'Use Dashboard for compliance overview',
      'Review Operations for audit trails',
      'Monitor Alerts for compliance issues'
    ]
  },
  {
    role: 'Viewer',
    title: 'Viewer',
    description: 'Read-only access for monitoring and reporting purposes.',
    capabilities: [
      'View system dashboard',
      'Monitor adapter status',
      'View telemetry data',
      'Access inference playground',
      'View alerts and notifications'
    ],
    restrictions: [
      'Cannot modify any settings',
      'Cannot manage adapters',
      'Cannot execute operations',
      'Cannot access policies'
    ],
    tips: [
      'Use Dashboard for system overview',
      'Monitor Adapters for status',
      'Check Operations for current state',
      'Review Alerts for issues'
    ]
  },
  {
    role: 'SRE',
    title: 'Site Reliability Engineer',
    description: 'Technical role focused on system reliability, performance, and incident response.',
    capabilities: [
      'Monitor system performance',
      'Access detailed telemetry',
      'View infrastructure metrics',
      'Manage alerts and incidents',
      'Execute diagnostic operations'
    ],
    restrictions: [
      'Cannot modify system settings',
      'Cannot manage tenants',
      'Limited policy access'
    ],
    tips: [
      'Focus on Dashboard for system health',
      'Use Operations for diagnostics',
      'Monitor Alerts for incidents',
      'Review telemetry for performance'
    ]
  },
  {
    role: 'Auditor',
    title: 'Auditor',
    description: 'Audit-focused role for compliance verification and security review.',
    capabilities: [
      'View audit trails',
      'Access compliance reports',
      'Review policy configurations',
      'Monitor security events',
      'Export telemetry data'
    ],
    restrictions: [
      'Read-only access',
      'Cannot modify any settings',
      'Cannot execute operations'
    ],
    tips: [
      'Use Policies for compliance review',
      'Check Dashboard for audit overview',
      'Review Operations for audit trails',
      'Export data for external analysis'
    ]
  }
];

export function getRoleGuidance(role: UserRole): RoleGuidanceItem | undefined {
  return roleGuidanceDatabase.find(item => item.role === role);
}

export function getRoleCapabilities(role: UserRole): string[] {
  const guidance = getRoleGuidance(role);
  return guidance?.capabilities || [];
}

export function getRoleRestrictions(role: UserRole): string[] {
  const guidance = getRoleGuidance(role);
  return guidance?.restrictions || [];
}
