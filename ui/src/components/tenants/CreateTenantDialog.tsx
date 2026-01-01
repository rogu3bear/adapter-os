import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

export interface NewTenantData {
  name: string;
  description: string;
  dataClassification: 'public' | 'internal' | 'confidential' | 'restricted';
  itarCompliant: boolean;
}

export interface CreateTenantDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (data: NewTenantData) => void;
  canManage: boolean;
}

export function CreateTenantDialog({
  open,
  onOpenChange,
  onSubmit,
  canManage,
}: CreateTenantDialogProps) {
  const [newTenant, setNewTenant] = useState<NewTenantData>({
    name: '',
    description: '',
    dataClassification: 'internal',
    itarCompliant: false,
  });

  const handleSubmit = () => {
    if (!newTenant.name.trim()) return;
    onSubmit(newTenant);
    setNewTenant({
      name: '',
      description: '',
      dataClassification: 'internal',
      itarCompliant: false,
    });
  };

  const handleCancel = () => {
    onOpenChange(false);
    setNewTenant({
      name: '',
      description: '',
      dataClassification: 'internal',
      itarCompliant: false,
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Create New Workspace</DialogTitle>
        </DialogHeader>
        <div className="mb-4">
          <div className="mb-4">
            <div className="flex items-center gap-1 mb-1">
              <Label htmlFor="name" className="font-medium text-sm">
                Workspace Name
              </Label>
              <GlossaryTooltip termId="tenant-name">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </div>
            <Input
              id="name"
              placeholder="Enter workspace name"
              value={newTenant.name}
              onChange={(e) => setNewTenant({ ...newTenant, name: e.target.value })}
              aria-required="true"
              aria-describedby="name-description"
            />
            <div id="name-description" className="sr-only">
              Workspace name is required
            </div>
          </div>

          <div className="mb-4">
            <div className="flex items-center gap-1 mb-1">
              <Label htmlFor="description" className="font-medium text-sm">
                Description
              </Label>
              <GlossaryTooltip termId="tenant-description">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </div>
            <Textarea
              id="description"
              placeholder="Describe the workspace's purpose"
              value={newTenant.description}
              onChange={(e) =>
                setNewTenant({ ...newTenant, description: e.target.value })
              }
            />
          </div>

          <div className="mb-4">
            <div className="flex items-center gap-1 mb-1">
              <Label htmlFor="classification" className="font-medium text-sm">
                Data Classification
              </Label>
              <GlossaryTooltip termId="data-classification">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </div>
            <Select
              value={newTenant.dataClassification}
              onValueChange={(value: string) =>
                setNewTenant({
                  ...newTenant,
                  dataClassification: value as NewTenantData['dataClassification'],
                })
              }
            >
              <SelectTrigger aria-required="true" aria-describedby="classification-description">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="public">Public</SelectItem>
                <SelectItem value="internal">Internal</SelectItem>
                <SelectItem value="confidential">Confidential</SelectItem>
                <SelectItem value="restricted">Restricted</SelectItem>
              </SelectContent>
            </Select>
            <div id="classification-description" className="sr-only">
              Data classification is required
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1">
              <Label htmlFor="itar" className="font-medium text-sm">
                ITAR Compliance Required
              </Label>
              <GlossaryTooltip termId="itar-compliance">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </GlossaryTooltip>
            </div>
            <Switch
              id="itar"
              checked={newTenant.itarCompliant}
              onCheckedChange={(checked) =>
                setNewTenant({ ...newTenant, itarCompliant: checked })
              }
            />
          </div>

          <div className="flex items-center justify-end mt-4">
            <Button variant="outline" onClick={handleCancel}>
              Cancel
            </Button>
            <GlossaryTooltip termId="create-tenant-action">
              <Button
                onClick={handleSubmit}
                disabled={!newTenant.name.trim() || !canManage}
              >
                Create Workspace
              </Button>
            </GlossaryTooltip>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
