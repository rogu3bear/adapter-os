// Activity event type constants matching backend ActivityEventType enum
// Backend source: crates/adapteros-db/src/activity_events.rs

export const ACTIVITY_EVENT_TYPES = {
  // Adapter events
  ADAPTER_CREATED: 'adapter_created',
  ADAPTER_UPDATED: 'adapter_updated',
  ADAPTER_DELETED: 'adapter_deleted',
  ADAPTER_SHARED: 'adapter_shared',
  ADAPTER_UNSHARED: 'adapter_unshared',
  
  // Resource events
  RESOURCE_SHARED: 'resource_shared',
  RESOURCE_UNSHARED: 'resource_unshared',
  
  // Message events
  MESSAGE_SENT: 'message_sent',
  MESSAGE_EDITED: 'message_edited',
  
  // User events
  USER_MENTIONED: 'user_mentioned',
  USER_JOINED_WORKSPACE: 'user_joined_workspace',
  USER_LEFT_WORKSPACE: 'user_left_workspace',
  
  // Workspace events
  WORKSPACE_CREATED: 'workspace_created',
  WORKSPACE_UPDATED: 'workspace_updated',
  
  // Member events
  MEMBER_ADDED: 'member_added',
  MEMBER_REMOVED: 'member_removed',
  MEMBER_ROLE_CHANGED: 'member_role_changed',
  
  // Code intelligence events
  REPO_SCAN_TRIGGERED: 'repo_scan_triggered',
  REPO_REPORT_VIEWED: 'repo_report_viewed',
  
  // Training events
  TRAINING_SESSION_STARTED: 'training_session_started',
} as const;

export type ActivityEventType = typeof ACTIVITY_EVENT_TYPES[keyof typeof ACTIVITY_EVENT_TYPES];

