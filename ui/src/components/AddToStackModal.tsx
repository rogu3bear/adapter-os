import React, { useState } from 'react';
import { FormModal } from '@/components/shared/Modal';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useAdapterStacks, useCreateAdapterStack, useUpdateAdapterStack } from '@/hooks/useAdmin';
import { toast } from 'sonner';
import { Plus } from 'lucide-react';
import type { AdapterStack } from '@/api/types';

interface AddToStackModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  adapterId: string;
}

export function AddToStackModal({ open, onOpenChange, adapterId }: AddToStackModalProps) {
  const [selectedStackId, setSelectedStackId] = useState<string>('');
  const [newStackName, setNewStackName] = useState('');
  const [createNew, setCreateNew] = useState(false);

  const { data: stacks = [] } = useAdapterStacks();
  const createStack = useCreateAdapterStack();
  const updateStack = useUpdateAdapterStack();

  const handleAdd = async () => {
    try {
      if (createNew) {
        // Create new stack with this adapter
        if (!newStackName.trim()) {
          toast.error('Stack name is required');
          return;
        }
        await createStack.mutateAsync({
          name: newStackName.trim(),
          description: `Stack created with adapter ${adapterId}`,
          adapters: [
            {
              adapter_id: adapterId,
              gate: 32767, // Default Q15 gate value
            },
          ],
        });
        toast.success(`Created new stack "${newStackName}" with adapter`);
      } else {
        // Add to existing stack
        if (!selectedStackId) {
          toast.error('Please select a stack');
          return;
        }
        const stack = stacks.find(s => s.id === selectedStackId);
        if (!stack) {
          toast.error('Stack not found');
          return;
        }

        // Check if adapter already in stack
        const adapterIds = stack.adapter_ids || [];
        if (adapterIds.includes(adapterId)) {
          toast.info('Adapter is already in this stack');
          return;
        }

        // Update stack with new adapter
        await updateStack.mutateAsync({
          stackId: selectedStackId,
          data: {
            name: stack.name,
            description: stack.description,
            adapters: [
              ...(stack.adapters || []).map(a => ({
                adapter_id: typeof a === 'string' ? a : a.adapter_id,
                gate: typeof a === 'object' && 'gate' in a ? a.gate : 32767,
              })),
              {
                adapter_id: adapterId,
                gate: 32767,
              },
            ],
          },
        });
        toast.success(`Added adapter to stack "${stack.name}"`);
      }
      onOpenChange(false);
      setSelectedStackId('');
      setNewStackName('');
      setCreateNew(false);
    } catch (error) {
      const err = error instanceof Error ? error : new Error('Failed to add adapter to stack');
      toast.error(err.message);
    }
  };

  const isValid = createNew ? newStackName.trim().length > 0 : !!selectedStackId;
  const isPending = createStack.isPending || updateStack.isPending;

  return (
    <FormModal
      open={open}
      onOpenChange={onOpenChange}
      title="Add Adapter to Stack"
      description={`Add adapter "${adapterId}" to an existing stack or create a new one`}
      size="md"
      onSubmit={handleAdd}
      submitText="Add"
      isSubmitting={isPending}
      isValid={isValid}
      onCancel={() => {
        setSelectedStackId('');
        setNewStackName('');
        setCreateNew(false);
      }}
    >
      <div className="space-y-4">
          <div className="flex items-center gap-4">
            <Button
              variant={!createNew ? 'default' : 'outline'}
              onClick={() => setCreateNew(false)}
            >
              Add to Existing
            </Button>
            <Button
              variant={createNew ? 'default' : 'outline'}
              onClick={() => setCreateNew(true)}
            >
              <Plus className="h-4 w-4 mr-2" />
              Create New Stack
            </Button>
          </div>

          {!createNew ? (
            <div className="space-y-2">
              <Label>Select Stack</Label>
              <Select value={selectedStackId} onValueChange={setSelectedStackId}>
                <SelectTrigger>
                  <SelectValue placeholder="Choose a stack" />
                </SelectTrigger>
                <SelectContent>
                  {stacks.map(stack => (
                    <SelectItem key={stack.id} value={stack.id}>
                      {stack.name}
                      {stack.description && ` - ${stack.description}`}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : (
            <div className="space-y-2">
              <Label htmlFor="stackName">Stack Name</Label>
              <Input
                id="stackName"
                value={newStackName}
                onChange={e => setNewStackName(e.target.value)}
                placeholder="my-new-stack"
              />
            </div>
          )}
      </div>
    </FormModal>
  );
}

