import { useEffect, useState } from 'react';
import { useForm, useFieldArray } from 'react-hook-form';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { useCreateAdapterStack, useUpdateAdapterStack } from '@/hooks/useAdmin';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { AdapterStack, CreateAdapterStackRequest, ActiveAdapter } from '@/api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { X, Plus } from 'lucide-react';
import { LoadingState } from '@/components/ui/loading-state';

interface StackFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stack?: AdapterStack;
}

interface FormData {
  name: string;
  description?: string;
  adapters: Array<{
    adapter_id: string;
    gate: number;
  }>;
}

export function StackFormModal({ open, onOpenChange, stack }: StackFormModalProps) {
  const isEdit = !!stack;
  const createStack = useCreateAdapterStack();
  const updateStack = useUpdateAdapterStack();

  // Fetch available adapters
  const { data: availableAdapters, isLoading: loadingAdapters } = useQuery({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
    enabled: open,
  });

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
    control,
  } = useForm<FormData>({
    defaultValues: {
      name: stack?.name || '',
      description: stack?.description || '',
      adapters: stack?.adapters?.map((a) => ({
        adapter_id: typeof a === 'string' ? a : a.adapter_id,
        gate: typeof a === 'object' && 'gate' in a ? a.gate : 32767,
      })) || [],
    },
  });

  const { fields, append, remove } = useFieldArray({
    control,
    name: 'adapters',
  });

  useEffect(() => {
    if (stack) {
      reset({
        name: stack.name,
        description: stack.description,
        adapters: stack.adapters?.map((a) => ({
          adapter_id: typeof a === 'string' ? a : a.adapter_id,
          gate: typeof a === 'object' && 'gate' in a ? a.gate : 32767,
        })) || [],
      });
    } else {
      reset({
        name: '',
        description: '',
        adapters: [],
      });
    }
  }, [stack, reset]);

  const onSubmit = async (data: FormData) => {
    try {
      const createData: CreateAdapterStackRequest = {
        name: data.name,
        description: data.description,
        adapters: data.adapters.map((a) => ({
          adapter_id: a.adapter_id,
          gate: a.gate,
        })) as ActiveAdapter[],
      };

      if (isEdit && stack) {
        await updateStack.mutateAsync({
          stackId: stack.id,
          data: {
            name: data.name,
            description: data.description,
            adapters: data.adapters.map((a) => ({
              adapter_id: a.adapter_id,
              gate: a.gate,
            })),
          },
        });
      } else {
        await createStack.mutateAsync(createData);
      }

      onOpenChange(false);
      reset();
    } catch (error) {
      // Error handling is done in the hook
    }
  };

  const addAdapter = () => {
    append({ adapter_id: '', gate: 32767 });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px] max-h-[80vh] overflow-y-auto">
        <form onSubmit={handleSubmit(onSubmit)}>
          <DialogHeader>
            <DialogTitle>{isEdit ? 'Edit Adapter Stack' : 'Create Adapter Stack'}</DialogTitle>
            <DialogDescription>
              {isEdit
                ? 'Update adapter stack configuration'
                : 'Create a new reusable adapter combination'}
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="name">
                Name <span className="text-destructive">*</span>
              </Label>
              <Input
                id="name"
                placeholder="my-stack"
                {...register('name', {
                  required: 'Name is required',
                  pattern: {
                    value: /^[a-z0-9-]+$/,
                    message: 'Name must be lowercase alphanumeric with hyphens',
                  },
                })}
              />
              {errors.name && (
                <p className="text-sm text-destructive">{errors.name.message}</p>
              )}
            </div>

            <div className="grid gap-2">
              <Label htmlFor="description">Description</Label>
              <Textarea
                id="description"
                placeholder="Describe the purpose of this stack..."
                rows={3}
                {...register('description')}
              />
            </div>

            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <Label>
                  Adapters <span className="text-destructive">*</span>
                </Label>
                <Button type="button" variant="outline" size="sm" onClick={addAdapter}>
                  <Plus className="h-4 w-4 mr-2" />
                  Add Adapter
                </Button>
              </div>

              {loadingAdapters && <LoadingState message="Loading adapters..." />}

              <div className="space-y-2">
                {fields.map((field, index) => (
                  <div key={field.id} className="flex gap-2 items-start">
                    <div className="flex-1">
                      <Select
                        value={field.adapter_id}
                        onValueChange={(value) => {
                          const current = fields[index];
                          remove(index);
                          append({ adapter_id: value, gate: current.gate });
                        }}
                      >
                        <SelectTrigger>
                          <SelectValue placeholder="Select adapter" />
                        </SelectTrigger>
                        <SelectContent>
                          {availableAdapters?.map((adapter) => (
                            <SelectItem key={adapter.id} value={adapter.id}>
                              {adapter.id}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>
                    <div className="w-32">
                      <Input
                        type="number"
                        placeholder="Gate (Q15)"
                        {...register(`adapters.${index}.gate`, {
                          valueAsNumber: true,
                          min: { value: 0, message: 'Gate must be >= 0' },
                          max: { value: 32767, message: 'Gate must be <= 32767' },
                        })}
                      />
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => remove(index)}
                    >
                      <X className="h-4 w-4" />
                    </Button>
                  </div>
                ))}
              </div>

              {fields.length === 0 && (
                <p className="text-sm text-muted-foreground">
                  No adapters added. Click "Add Adapter" to get started.
                </p>
              )}

              <p className="text-xs text-muted-foreground">
                Gate value is Q15 quantized (0-32767). Higher values give the adapter more weight.
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                onOpenChange(false);
                reset();
              }}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting || fields.length === 0}>
              {isSubmitting ? 'Saving...' : isEdit ? 'Update' : 'Create'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
