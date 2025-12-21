//! Workspace selector component
//!
//! Dropdown to select active workspace for messaging and collaboration.
//! Shows workspace names and member counts.
//!
//! Citation: Tenant selector from ui/src/layout/RootLayout.tsx L144-L156 (Select component)
//! - Dropdown to switch active workspace for messaging

import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Loader2, Users, Building } from 'lucide-react';
import { Workspace } from '@/api/types';

interface WorkspaceSelectorProps {
  workspaces: Workspace[];
  selectedWorkspaceId: string;
  onWorkspaceSelect: (workspaceId: string) => void;
  loading?: boolean;
}

export function WorkspaceSelector({
  workspaces,
  selectedWorkspaceId,
  onWorkspaceSelect,
  loading = false
}: WorkspaceSelectorProps) {
  const selectedWorkspace = workspaces.find(w => w.id === selectedWorkspaceId);

  if (loading) {
    return (
      <div className="flex items-center gap-2">
        <Loader2 className="h-4 w-4 animate-spin" />
        <span className="text-sm text-muted-foreground">Loading workspaces...</span>
      </div>
    );
  }

  if (workspaces.length === 0) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Building className="h-4 w-4" />
        No workspaces available
      </div>
    );
  }

  return (
    <div className="flex items-center gap-4">
      <Select value={selectedWorkspaceId} onValueChange={onWorkspaceSelect}>
        <SelectTrigger className="w-[300px]">
          <SelectValue placeholder="Select a workspace">
            {selectedWorkspace && (
              <div className="flex items-center gap-2">
                <Building className="h-4 w-4" />
                <span>{selectedWorkspace.name}</span>
                {selectedWorkspace.description && (
                  <span className="text-muted-foreground text-sm truncate">
                    • {selectedWorkspace.description}
                  </span>
                )}
              </div>
            )}
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {workspaces.map((workspace) => (
            <SelectItem key={workspace.id} value={workspace.id}>
              <div className="flex items-center justify-between w-full">
                <div className="flex items-center gap-2">
                  <Building className="h-4 w-4" />
                  <div>
                    <div className="font-medium">{workspace.name}</div>
                    {workspace.description && (
                      <div className="text-xs text-muted-foreground truncate max-w-[200px]">
                        {workspace.description}
                      </div>
                    )}
                  </div>
                </div>
                <Badge variant="outline" className="text-xs">
                  <Users className="h-3 w-3 mr-1" />
                  {/* We don't have member count in Workspace type, so we'll show a placeholder */}
                  Members
                </Badge>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {selectedWorkspace && (
        <div className="text-sm text-muted-foreground">
          Created {new Date(selectedWorkspace.created_at).toLocaleDateString()}
        </div>
      )}
    </div>
  );
}
