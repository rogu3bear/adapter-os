import React from 'react';
import { Button } from '@/components/ui/button';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Plus } from 'lucide-react';

export interface TenantsHeaderProps {
  canManage: boolean;
  onCreateClick: () => void;
}

export function TenantsHeader({ canManage, onCreateClick }: TenantsHeaderProps) {
  return (
    <div className="flex items-center justify-between mb-6">
      <div>
        <h1 className="text-2xl font-bold">Workspace Management</h1>
        <p className="text-sm text-muted-foreground">
          Manage workspace isolation, data classification, and access controls
        </p>
      </div>
      <GlossaryTooltip termId="create-tenant-button">
        <Button disabled={!canManage} onClick={onCreateClick}>
          <Plus className="h-4 w-4 mr-2" />
          Create Workspace
        </Button>
      </GlossaryTooltip>
    </div>
  );
}
