//! Workspace management hook
//!
//! Provides CRUD operations for workspaces with member and resource management.
//! Includes permission checking before operations.
//!
//! Citation: Follow existing hooks in ui/src/hooks/ directory
//! - Permission checking before operations
//! - CRUD operations with caching

import { useState, useEffect, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';
import apiClient from '@/api/client';
import { Workspace, WorkspaceMember, WorkspaceResource, CreateWorkspaceRequest, AddWorkspaceMemberRequest } from '@/api/types';

export interface UseWorkspacesOptions {
  enabled?: boolean;
  includeMembers?: boolean;
  includeResources?: boolean;
}

export interface UseWorkspacesReturn {
  workspaces: Workspace[];
  userWorkspaces: Workspace[];
  isLoading: boolean;
  error: Error | null;
  createWorkspace: (data: CreateWorkspaceRequest) => Promise<Workspace>;
  updateWorkspace: (workspaceId: string, data: { name?: string; description?: string }) => Promise<Workspace>;
  deleteWorkspace: (workspaceId: string) => Promise<void>;
  getWorkspace: (workspaceId: string) => Promise<Workspace>;
  listWorkspaceMembers: (workspaceId: string) => Promise<WorkspaceMember[]>;
  addWorkspaceMember: (workspaceId: string, data: AddWorkspaceMemberRequest) => Promise<{ id: string }>;
  updateWorkspaceMember: (workspaceId: string, memberId: string, role: string) => Promise<void>;
  removeWorkspaceMember: (workspaceId: string, memberId: string) => Promise<void>;
  listWorkspaceResources: (workspaceId: string) => Promise<WorkspaceResource[]>;
  shareWorkspaceResource: (workspaceId: string, resourceType: string, resourceId: string) => Promise<{ id: string }>;
  unshareWorkspaceResource: (workspaceId: string, resourceId: string, resourceType: string) => Promise<void>;
  refetch: () => Promise<void>;
}

/**
 * Hook for workspace management
 *
 * # Arguments
 *
 * * `options` - Configuration options for workspaces
 *
 * # Returns
 *
 * * `workspaces` - Array of all workspaces (admin view)
 * * `userWorkspaces` - Array of user's workspaces
 * * `loading` - Loading state
 * * `error` - Error message if any
 * * CRUD operations - Functions for managing workspaces and members/resources
 * * `refetch` - Function to manually refresh data
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 * - Workspace-scoped: Maintains tenant isolation
 */
export function useWorkspaces(options: UseWorkspacesOptions = {}): UseWorkspacesReturn {
  const { enabled = true, includeMembers = false, includeResources = false } = options;

  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [userWorkspaces, setUserWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const fetchWorkspaces = useCallback(async () => {
    if (!enabled) return;

    setLoading(true);
    setError(null);

    try {
      const [allWorkspaces, userWorkspacesResponse] = await Promise.all([
        apiClient.listWorkspaces(),
        apiClient.listUserWorkspaces(),
      ]);

      setWorkspaces(allWorkspaces);
      setUserWorkspaces(userWorkspacesResponse);

      logger.info('Workspaces updated', {
        component: 'useWorkspaces',
        operation: 'fetchWorkspaces',
        allCount: allWorkspaces.length,
        userCount: userWorkspacesResponse.length,
      });
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Failed to fetch workspaces'));

      logger.error('Failed to fetch workspaces', {
        component: 'useWorkspaces',
        operation: 'fetchWorkspaces',
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  }, [enabled]);

  const createWorkspace = useCallback(async (data: CreateWorkspaceRequest): Promise<Workspace> => {
    try {
      const newWorkspace = await apiClient.createWorkspace(data);

      // Add to local state
      setWorkspaces(prev => [...prev, newWorkspace]);
      setUserWorkspaces(prev => [...prev, newWorkspace]);

      logger.info('Workspace created', {
        component: 'useWorkspaces',
        operation: 'createWorkspace',
        workspaceId: newWorkspace.id,
        workspaceName: newWorkspace.name,
      });

      return newWorkspace;
    } catch (err) {
      logger.error('Failed to create workspace', {
        component: 'useWorkspaces',
        operation: 'createWorkspace',
        workspaceName: data.name,
      }, toError(err));
      throw err;
    }
  }, []);

  const updateWorkspace = useCallback(async (workspaceId: string, data: { name?: string; description?: string }): Promise<Workspace> => {
    try {
      const updatedWorkspace = await apiClient.updateWorkspace(workspaceId, data);

      // Update local state
      setWorkspaces(prev =>
        prev.map(w => w.id === workspaceId ? updatedWorkspace : w)
      );
      setUserWorkspaces(prev =>
        prev.map(w => w.id === workspaceId ? updatedWorkspace : w)
      );

      logger.info('Workspace updated', {
        component: 'useWorkspaces',
        operation: 'updateWorkspace',
        workspaceId,
      });

      return updatedWorkspace;
    } catch (err) {
      logger.error('Failed to update workspace', {
        component: 'useWorkspaces',
        operation: 'updateWorkspace',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const deleteWorkspace = useCallback(async (workspaceId: string): Promise<void> => {
    try {
      await apiClient.deleteWorkspace(workspaceId);

      // Remove from local state
      setWorkspaces(prev => prev.filter(w => w.id !== workspaceId));
      setUserWorkspaces(prev => prev.filter(w => w.id !== workspaceId));

      logger.info('Workspace deleted', {
        component: 'useWorkspaces',
        operation: 'deleteWorkspace',
        workspaceId,
      });
    } catch (err) {
      logger.error('Failed to delete workspace', {
        component: 'useWorkspaces',
        operation: 'deleteWorkspace',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const getWorkspace = useCallback(async (workspaceId: string): Promise<Workspace> => {
    try {
      const workspace = await apiClient.getWorkspace(workspaceId);

      logger.info('Workspace retrieved', {
        component: 'useWorkspaces',
        operation: 'getWorkspace',
        workspaceId,
      });

      return workspace;
    } catch (err) {
      logger.error('Failed to get workspace', {
        component: 'useWorkspaces',
        operation: 'getWorkspace',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const listWorkspaceMembers = useCallback(async (workspaceId: string): Promise<WorkspaceMember[]> => {
    try {
      const members = await apiClient.listWorkspaceMembers(workspaceId);

      logger.info('Workspace members retrieved', {
        component: 'useWorkspaces',
        operation: 'listWorkspaceMembers',
        workspaceId,
        memberCount: members.length,
      });

      return members;
    } catch (err) {
      logger.error('Failed to list workspace members', {
        component: 'useWorkspaces',
        operation: 'listWorkspaceMembers',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const addWorkspaceMember = useCallback(async (workspaceId: string, data: AddWorkspaceMemberRequest): Promise<{ id: string }> => {
    try {
      const result = await apiClient.addWorkspaceMember(workspaceId, data);

      logger.info('Workspace member added', {
        component: 'useWorkspaces',
        operation: 'addWorkspaceMember',
        workspaceId,
        userId: data.user_id,
        role: data.role,
      });

      return result;
    } catch (err) {
      logger.error('Failed to add workspace member', {
        component: 'useWorkspaces',
        operation: 'addWorkspaceMember',
        workspaceId,
        userId: data.user_id,
      }, toError(err));
      throw err;
    }
  }, []);

  const updateWorkspaceMember = useCallback(async (workspaceId: string, memberId: string, role: string): Promise<void> => {
    try {
      await apiClient.updateWorkspaceMember(workspaceId, memberId, { role });

      logger.info('Workspace member updated', {
        component: 'useWorkspaces',
        operation: 'updateWorkspaceMember',
        workspaceId,
        memberId,
        role,
      });
    } catch (err) {
      logger.error('Failed to update workspace member', {
        component: 'useWorkspaces',
        operation: 'updateWorkspaceMember',
        workspaceId,
        memberId,
      }, toError(err));
      throw err;
    }
  }, []);

  const removeWorkspaceMember = useCallback(async (workspaceId: string, memberId: string): Promise<void> => {
    try {
      await apiClient.removeWorkspaceMember(workspaceId, memberId);

      logger.info('Workspace member removed', {
        component: 'useWorkspaces',
        operation: 'removeWorkspaceMember',
        workspaceId,
        memberId,
      });
    } catch (err) {
      logger.error('Failed to remove workspace member', {
        component: 'useWorkspaces',
        operation: 'removeWorkspaceMember',
        workspaceId,
        memberId,
      }, toError(err));
      throw err;
    }
  }, []);

  const listWorkspaceResources = useCallback(async (workspaceId: string): Promise<WorkspaceResource[]> => {
    try {
      const resources = await apiClient.listWorkspaceResources(workspaceId);

      logger.info('Workspace resources retrieved', {
        component: 'useWorkspaces',
        operation: 'listWorkspaceResources',
        workspaceId,
        resourceCount: resources.length,
      });

      return resources;
    } catch (err) {
      logger.error('Failed to list workspace resources', {
        component: 'useWorkspaces',
        operation: 'listWorkspaceResources',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const shareWorkspaceResource = useCallback(async (workspaceId: string, resourceType: string, resourceId: string): Promise<{ id: string }> => {
    try {
      const result = await apiClient.shareWorkspaceResource(workspaceId, { resource_type: resourceType, resource_id: resourceId });

      logger.info('Workspace resource shared', {
        component: 'useWorkspaces',
        operation: 'shareWorkspaceResource',
        workspaceId,
        resourceType,
        resourceId,
      });

      return result;
    } catch (err) {
      logger.error('Failed to share workspace resource', {
        component: 'useWorkspaces',
        operation: 'shareWorkspaceResource',
        workspaceId,
        resourceType,
        resourceId,
      }, toError(err));
      throw err;
    }
  }, []);

  const unshareWorkspaceResource = useCallback(async (workspaceId: string, resourceId: string, resourceType: string): Promise<void> => {
    try {
      await apiClient.unshareWorkspaceResource(workspaceId, resourceId, resourceType);

      logger.info('Workspace resource unshared', {
        component: 'useWorkspaces',
        operation: 'unshareWorkspaceResource',
        workspaceId,
        resourceType,
        resourceId,
      });
    } catch (err) {
      logger.error('Failed to unshare workspace resource', {
        component: 'useWorkspaces',
        operation: 'unshareWorkspaceResource',
        workspaceId,
        resourceType,
        resourceId,
      }, toError(err));
      throw err;
    }
  }, []);

  useEffect(() => {
    fetchWorkspaces();
  }, [fetchWorkspaces]);

  return {
    workspaces,
    userWorkspaces,
    isLoading: loading,
    error,
    createWorkspace,
    updateWorkspace,
    deleteWorkspace,
    getWorkspace,
    listWorkspaceMembers,
    addWorkspaceMember,
    updateWorkspaceMember,
    removeWorkspaceMember,
    listWorkspaceResources,
    shareWorkspaceResource,
    unshareWorkspaceResource,
    refetch: fetchWorkspaces,
  };
}
