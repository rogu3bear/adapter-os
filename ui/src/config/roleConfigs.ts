import type { UserRole } from '@/api/auth-types';

export interface RoleLanguage {
  welcomeTitle: string;
  roleLabel: string;
  aiModelsLabel: string;
  learningTasksLabel: string;
  systemHealthLabel: string;
  technicalDetailsLabel: string;
  emptyActivityCopy: string;
  emptyTasksCopy: string;
}

export const sharedLanguage = {
  aiModels: 'AI Models',
  learningTasks: 'Learning Tasks',
  systemHealth: 'System Health',
  technicalDetails: 'Technical Details',
  tenantHelp: 'Workspaces your teams share (tenants)',
  egressHelp: 'Outbound data leaving your deployment',
};

export const roleLanguage: Record<UserRole | 'unknown', RoleLanguage> = {
  admin: {
    welcomeTitle: 'Welcome back',
    roleLabel: 'Administrator',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No recent admin activity to review.',
    emptyTasksCopy: 'No admin tasks are waiting.',
  },
  developer: {
    welcomeTitle: 'Welcome back',
    roleLabel: 'Developer',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No recent development activity.',
    emptyTasksCopy: 'No development tasks in queue.',
  },
  operator: {
    welcomeTitle: 'Welcome back',
    roleLabel: 'Operator',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No recent operator actions yet.',
    emptyTasksCopy: 'No learning tasks active.',
  },
  viewer: {
    welcomeTitle: 'Welcome',
    roleLabel: 'Viewer',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No recent activity yet.',
    emptyTasksCopy: 'No tasks in progress.',
  },
  sre: {
    welcomeTitle: 'Welcome back',
    roleLabel: 'Reliability',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No reliability alerts right now.',
    emptyTasksCopy: 'No remediation tasks are running.',
  },
  compliance: {
    welcomeTitle: 'Welcome back',
    roleLabel: 'Compliance',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No compliance updates right now.',
    emptyTasksCopy: 'No reviews waiting.',
  },
  auditor: {
    welcomeTitle: 'Welcome',
    roleLabel: 'Auditor',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No audit events to review.',
    emptyTasksCopy: 'No audits in progress.',
  },
  unknown: {
    welcomeTitle: 'Welcome',
    roleLabel: 'User',
    aiModelsLabel: sharedLanguage.aiModels,
    learningTasksLabel: sharedLanguage.learningTasks,
    systemHealthLabel: sharedLanguage.systemHealth,
    technicalDetailsLabel: sharedLanguage.technicalDetails,
    emptyActivityCopy: 'No recent activity yet.',
    emptyTasksCopy: 'No tasks in progress.',
  },
};

export function getRoleLanguage(role?: UserRole | null): RoleLanguage {
  const normalized = (role || 'unknown') as UserRole | 'unknown';
  return roleLanguage[normalized] ?? roleLanguage.unknown;
}
