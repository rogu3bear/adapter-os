import { useEffect, useState, useCallback, useMemo } from 'react';
import { useForm, useFieldArray } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { FormModalWithHookForm } from '@/components/shared/Modal';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { useCreateAdapterStack, useUpdateAdapterStack } from '@/hooks/admin/useAdmin';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { AdapterStack, CreateAdapterStackRequest, ActiveAdapter } from '@/api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { X, Plus, AlertTriangle } from 'lucide-react';
import { LoadingState } from '@/components/ui/loading-state';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { calculateTotalMemory } from '@/utils/memoryEstimation';
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { StackSortableAdapterItem } from '@/components/StackSortableAdapterItem';
import { useStackUpdateNotifications } from '@/hooks/training';
import { StackFormSchema, type StackFormData } from '@/schemas/admin.schema';

interface StackFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stack?: AdapterStack;
  initialAdapterId?: string;
  onStackCreated?: (stackId: string) => void;
}

export function StackFormModal({ open, onOpenChange, stack, initialAdapterId, onStackCreated }: StackFormModalProps) {
  const isEdit = !!stack;
  const createStack = useCreateAdapterStack();
  const updateStack = useUpdateAdapterStack();
  const { notifyStackUpdate } = useStackUpdateNotifications();

  // Fetch available adapters
  const { data: availableAdapters, isLoading: loadingAdapters } = useQuery({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
    enabled: open,
  });

  // Fetch capacity for memory warnings
  const { data: capacity } = useQuery({
    queryKey: ['capacity'],
    queryFn: () => apiClient.getCapacity(),
    enabled: open,
  });

  const form = useForm<StackFormData>({
    resolver: zodResolver(StackFormSchema),
    defaultValues: {
      name: stack?.name || '',
      description: stack?.description || '',
      adapters: stack?.adapters?.map((a) => ({
        adapter_id: typeof a === 'string' ? a : a.adapter_id,
        gate: typeof a === 'object' && 'gate' in a ? a.gate : 32767,
      })) || [],
    },
  });

  const { register, formState: { errors }, reset, control } = form;

  const { fields, append, remove, move } = useFieldArray({
    control,
    name: 'adapters',
  });

  // Drag and drop sensors
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Handle drag end
  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (over && active.id !== over.id) {
      const oldIndex = fields.findIndex((field) => field.id === active.id);
      const newIndex = fields.findIndex((field) => field.id === over.id);
      if (oldIndex !== -1 && newIndex !== -1) {
        move(oldIndex, newIndex);
      }
    }
  }, [fields, move]);

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
      // Pre-populate with initialAdapterId if provided
      reset({
        name: '',
        description: '',
        adapters: initialAdapterId ? [{ adapter_id: initialAdapterId, gate: 32767 }] : [],
      });
    }
  }, [stack, initialAdapterId, reset]);

  const [warnings, setWarnings] = useState<string[]>([]);
  const [hasCriticalWarning, setHasCriticalWarning] = useState(false);

  // Memoize adapter IDs from form fields
  const currentAdapterIds = useMemo(() => {
    return fields.map(f => f.adapter_id).filter(Boolean);
  }, [fields]);

  // Calculate memory usage and check capacity
  const calculateMemoryWarnings = useCallback((adapterIds: string[]): { warnings: string[]; isCritical: boolean } => {
    const warnings: string[] = [];
    let isCritical = false;
    
    if (!capacity || !availableAdapters) {
      return { warnings, isCritical };
    }

    // Calculate total memory for selected adapters (with estimation if needed)
    const { totalBytes: totalMemoryBytes, estimated, missing } = calculateTotalMemory(
      adapterIds,
      availableAdapters
    );

    // Warn about missing adapters
    if (missing.length > 0) {
      warnings.push(
        `Warning: ${missing.length} adapter(s) not found: ${missing.slice(0, 3).join(', ')}${missing.length > 3 ? '...' : ''}`
      );
    }

    // Warn if estimation was used
    if (estimated) {
      warnings.push(
        'Memory estimate may be inaccurate. Some adapters do not have memory_bytes set.'
      );
    }

    const totalMemoryMB = totalMemoryBytes / (1024 * 1024);
    const totalRAMBytes = capacity.total_ram_bytes || 0;
    const totalRAMMB = totalRAMBytes / (1024 * 1024);
    const memoryUsagePercent = totalRAMBytes > 0 ? (totalMemoryBytes / totalRAMBytes) * 100 : 0;

    // Warn if exceeds 85% of capacity (critical - should prevent submission)
    if (memoryUsagePercent > 85) {
      isCritical = true;
      warnings.push(
        `Stack memory usage (${totalMemoryMB.toFixed(1)} MB) exceeds 85% of node capacity (${totalRAMMB.toFixed(1)} MB). ` +
        `Current usage: ${memoryUsagePercent.toFixed(1)}%`
      );
    } else if (memoryUsagePercent > 70) {
      warnings.push(
        `Stack memory usage (${totalMemoryMB.toFixed(1)} MB) is high (${memoryUsagePercent.toFixed(1)}% of capacity). ` +
        `Consider reducing adapter count or using smaller adapters.`
      );
    }

    return { warnings, isCritical };
  }, [capacity, availableAdapters]);

  // Memoize current warnings calculation
  const currentWarnings = useMemo(() => {
    if (currentAdapterIds.length === 0) {
      setHasCriticalWarning(false);
      return { warnings: [], isCritical: false };
    }
    const result = calculateMemoryWarnings(currentAdapterIds);
    setHasCriticalWarning(result.isCritical);
    return result;
  }, [currentAdapterIds, calculateMemoryWarnings]);

  const onSubmit = async (data: StackFormData) => {
    // Calculate memory warnings before submission
    const adapterIds = data.adapters.map(a => a.adapter_id);
    const { warnings: memoryWarnings, isCritical } = calculateMemoryWarnings(adapterIds);

    // Prevent submission if critical warning exists
    if (isCritical) {
      setWarnings(memoryWarnings);
      setHasCriticalWarning(true);
      throw new Error('Memory usage exceeds safe limits'); // Throw to prevent form from closing
    }

    setWarnings(memoryWarnings);
    setHasCriticalWarning(false);

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
      notifyStackUpdate(stack.id, data.name);
    } else {
      const newStack = await createStack.mutateAsync(createData);
      // Show memory warnings if any (but don't block closing)
      if (memoryWarnings.length > 0) {
        setWarnings(memoryWarnings);
      }
      notifyStackUpdate(newStack.stack.id, data.name);
      onStackCreated?.(newStack.stack.id);
    }
  };

  const addAdapter = () => {
    append({ adapter_id: '', gate: 32767 });
  };

  return (
    <FormModalWithHookForm
      open={open}
      onOpenChange={onOpenChange}
      title={isEdit ? 'Edit Adapter Stack' : 'Create Adapter Stack'}
      description={
        isEdit
          ? 'Update adapter stack configuration'
          : 'Create a new reusable adapter combination'
      }
      form={form}
      onSubmit={onSubmit}
      submitText={isEdit ? 'Update' : 'Create'}
      size="lg"
      className="sm:max-w-[600px]"
    >
      <div className="grid gap-4">
            {/* Show memory warnings prominently (memoized) */}
            {currentWarnings.warnings.length > 0 && (
              <Alert variant={currentWarnings.isCritical ? "destructive" : "default"} className={currentWarnings.isCritical ? "border-2" : ""}>
                <AlertTriangle className="h-5 w-5" />
                <AlertTitle className="text-base font-semibold">
                  {currentWarnings.isCritical ? "Critical Memory Warning" : "Memory Capacity Warnings"}
                </AlertTitle>
                <AlertDescription className="mt-2">
                  <ul className="list-disc list-inside space-y-1.5">
                    {currentWarnings.warnings.map((warning, idx) => (
                      <li key={idx} className="font-medium">{warning}</li>
                    ))}
                  </ul>
                  {currentWarnings.isCritical && (
                    <p className="mt-2 text-sm font-semibold text-destructive">
                      Submission blocked: Memory usage exceeds safe limits. Please reduce adapter count or use smaller adapters.
                    </p>
                  )}
                  {!currentWarnings.isCritical && (
                    <p className="mt-2 text-sm">
                      Consider reducing the number of adapters or using smaller adapters to avoid memory issues.
                    </p>
                  )}
                </AlertDescription>
              </Alert>
            )}
            
            {warnings.length > 0 && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>Capacity Warnings</AlertTitle>
                <AlertDescription>
                  <ul className="list-disc list-inside space-y-1">
                    {warnings.map((warning, idx) => (
                      <li key={idx}>{warning}</li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            )}
            <div className="grid gap-2">
              <Label htmlFor="name">
                Name <span className="text-destructive">*</span>
              </Label>
              <Input
                id="name"
                placeholder="my-stack"
                {...register('name')}
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

              <DndContext
                sensors={sensors}
                collisionDetection={closestCenter}
                onDragEnd={handleDragEnd}
              >
                <SortableContext
                  items={fields.map((f) => f.id)}
                  strategy={verticalListSortingStrategy}
                >
                  <div className="space-y-2">
                    {fields.map((field, index) => (
                      <StackSortableAdapterItem
                        key={field.id}
                        id={field.id}
                        adapterId={field.adapter_id}
                        gate={field.gate}
                        availableAdapters={availableAdapters}
                        onAdapterChange={(value) => {
                          const current = fields[index];
                          remove(index);
                          append({ adapter_id: value, gate: current.gate });
                        }}
                        onGateChange={() => {
                          // Gate change is handled by react-hook-form register
                        }}
                        onRemove={() => remove(index)}
                        register={register}
                        index={index}
                      />
                    ))}
                  </div>
                </SortableContext>
              </DndContext>

              {fields.length === 0 && (
                <p className="text-sm text-muted-foreground">
                  No adapters added. Click "Add Adapter" to get started.
                </p>
              )}

              <p className="text-xs text-muted-foreground">
                Confidence score is Q15 quantized (0-32767). Higher values give the adapter more weight.
              </p>
            </div>
          </div>
    </FormModalWithHookForm>
  );
}
