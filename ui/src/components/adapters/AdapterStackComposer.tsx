import React, { useState, useCallback, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
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
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import {
  Plus,
  Trash2,
  Eye,
  Save,
  X,
  AlertCircle,
  CheckCircle2,
  GripVertical,
  Settings,
} from 'lucide-react';
import { cn } from '@/components/ui/utils';
import apiClient from '@/api/client';
import { Adapter } from '@/api/types';
import {
  StackPreview,
  ValidationReport,
  InferenceTestResult,
} from './StackPreview';
import { SortableAdapterItem } from './SortableAdapterItem';

interface StackAdapter {
  adapter: Adapter;
  order: number;
  enabled: boolean;
}

interface AdapterStackComposerProps {
  onStackCreated?: (stackId: string, stackName: string) => void;
  onStackUpdated?: (stackId: string, adapters: StackAdapter[]) => void;
  initialStackId?: string;
  initialStackName?: string;
  initialAdapters?: StackAdapter[];
}

export const AdapterStackComposer: React.FC<AdapterStackComposerProps> = ({
  onStackCreated,
  onStackUpdated,
  initialStackId,
  initialStackName,
  initialAdapters,
}) => {
  const [adapters, setAdapters] = useState<StackAdapter[]>(
    initialAdapters || []
  );
  const [stackName, setStackName] = useState(initialStackName || '');
  const [stackDescription, setStackDescription] = useState('');
  const [availableAdapters, setAvailableAdapters] = useState<Adapter[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState<string>('');
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [validationReport, setValidationReport] =
    useState<ValidationReport | null>(null);
  const [testResult, setTestResult] = useState<InferenceTestResult | null>(
    null
  );
  const [showPreview, setShowPreview] = useState(false);

  // Drag and drop setup
  const sensors = useSensors(
    useSensor(PointerSensor, {
      // @ts-ignore - dnd-kit types
      distance: 8,
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Load available adapters
  React.useEffect(() => {
    const fetchAdapters = async () => {
      setIsLoading(true);
      try {
        const fetchedAdapters = await apiClient.listAdapters();
        const sortedAdapters = (fetchedAdapters || []).sort((a, b) => {
          const aTs = a.created_at ? new Date(a.created_at).getTime() : 0;
          const bTs = b.created_at ? new Date(b.created_at).getTime() : 0;
          return bTs - aTs; // last imported first; no semver/lex heuristics
        });
        setAvailableAdapters(sortedAdapters);
        setSelectedAdapter((prev) => prev || sortedAdapters[0]?.adapter_id || '');
      } catch (error: unknown) {
        console.error('Failed to fetch adapters:', error);
      } finally {
        setIsLoading(false);
      }
    };

    fetchAdapters();
  }, []);

  const handleAddAdapter = useCallback(() => {
    if (!selectedAdapter) return;

    const adapter = availableAdapters.find(
      (a) => a.adapter_id === selectedAdapter
    );
    if (!adapter) return;

    // Check if already in stack
    if (adapters.some((item) => item.adapter.adapter_id === selectedAdapter)) {
      alert('Adapter already in stack');
      return;
    }

    const newOrder = Math.max(0, ...adapters.map((a) => a.order)) + 1;
    setAdapters((prev) => [
      ...prev,
      {
        adapter,
        order: newOrder,
        enabled: true,
      },
    ]);

    setSelectedAdapter('');
  }, [selectedAdapter, availableAdapters, adapters]);

  const handleRemoveAdapter = useCallback((adapterId: string) => {
    setAdapters((prev) =>
      prev.filter((item) => item.adapter.adapter_id !== adapterId)
    );
  }, []);

  const handleToggleAdapter = useCallback((adapterId: string) => {
    setAdapters((prev) =>
      prev.map((item) =>
        item.adapter.adapter_id === adapterId
          ? { ...item, enabled: !item.enabled }
          : item
      )
    );
  }, []);

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      setAdapters((prev) => {
        const oldIndex = prev.findIndex(
          (a) => a.adapter.adapter_id === active.id
        );
        const newIndex = prev.findIndex(
          (a) => a.adapter.adapter_id === over.id
        );

        const newAdapters = arrayMove(prev, oldIndex, newIndex);
        return newAdapters.map((item, idx) => ({
          ...item,
          order: idx,
        }));
      });
    }
  };

  const handleSaveStack = async () => {
    if (!stackName.trim()) {
      alert('Please enter a stack name');
      return;
    }

    if (adapters.length === 0) {
      alert('Stack must contain at least one adapter');
      return;
    }

    if (!validationReport?.isValid) {
      alert('Please resolve validation errors before saving');
      return;
    }

    setIsSaving(true);
    try {
      const payload = {
        name: stackName,
        description: stackDescription,
        adapter_ids: adapters.map((item) => item.adapter.adapter_id),
        adapter_order: adapters.map((item) => ({
          adapter_id: item.adapter.adapter_id,
          order: item.order,
        })),
        workflow_type: 'sequential',
      };

      let response: { data: { id?: string; stack_id?: string } };
      if (initialStackId) {
        response = await apiClient.request<{ data: { id?: string; stack_id?: string } }>(`/api/adapter-stacks/${initialStackId}`, {
          method: 'PUT',
          body: JSON.stringify(payload),
        });
      } else {
        response = await apiClient.request<{ data: { id?: string; stack_id?: string } }>('/api/adapter-stacks', {
          method: 'POST',
          body: JSON.stringify(payload),
        });
      }

      const stackId = response.data.id || response.data.stack_id || '';

      if (initialStackId && onStackUpdated && stackId) {
        onStackUpdated(stackId, adapters);
      } else if (!initialStackId && onStackCreated && stackId) {
        onStackCreated(stackId, stackName);
      }

      alert(
        initialStackId
          ? 'Stack updated successfully'
          : 'Stack created successfully'
      );
    } catch (error: unknown) {
      console.error('Failed to save stack:', error);
      alert(`Failed to save stack: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsSaving(false);
    }
  };

  const getUnusedAdapters = useMemo(() => {
    const usedIds = adapters.map((item) => item.adapter.adapter_id);
    return availableAdapters.filter((a) => !usedIds.includes(a.adapter_id));
  }, [availableAdapters, adapters]);

  const canSaveStack = adapters.length > 0 && stackName.trim().length > 0;

  return (
    <div className="space-y-4">
      <Tabs defaultValue="composer" className="w-full">
        <TabsList className="grid w-full grid-cols-2">
          <TabsTrigger value="composer">Composer</TabsTrigger>
          <TabsTrigger value="preview" onClick={() => setShowPreview(true)}>
            Preview & Validate
          </TabsTrigger>
        </TabsList>

        <TabsContent value="composer" className="space-y-4">
          {/* Stack Details */}
          <Card>
            <CardHeader>
              <CardTitle>Stack Details</CardTitle>
              <CardDescription>
                Configure your adapter stack settings
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <Label htmlFor="stack-name" className="text-sm">
                  Stack Name
                </Label>
                <Input
                  id="stack-name"
                  placeholder="e.g., production-code-review"
                  value={stackName}
                  onChange={(e) => setStackName(e.target.value)}
                  className="mt-2"
                />
                <p className="text-xs text-muted-foreground mt-2">
                  Use semantic naming: {'{tenant}/{domain}/{purpose}/{revision}'}
                </p>
              </div>

              <div>
                <Label htmlFor="stack-description" className="text-sm">
                  Description (Optional)
                </Label>
                <textarea
                  id="stack-description"
                  placeholder="Describe the purpose and use case for this stack..."
                  value={stackDescription}
                  onChange={(e) => setStackDescription(e.target.value)}
                  className="w-full p-3 border rounded-md bg-background text-sm resize-none"
                  rows={3}
                />
              </div>
            </CardContent>
          </Card>

          {/* Add Adapters */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Add Adapters</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                  <SelectTrigger className="flex-1">
                    <SelectValue placeholder="Select an adapter to add..." />
                  </SelectTrigger>
                  <SelectContent>
                    {getUnusedAdapters.map((adapter) => (
                      <SelectItem
                        key={adapter.adapter_id}
                        value={adapter.adapter_id}
                      >
                      <div className="flex flex-col">
                        <span className="font-medium">{adapter.name}</span>
                        <span className="text-xs text-muted-foreground">
                          {adapter.version ? `v${adapter.version}` : 'no version'}
                          {adapter.hash_b3 ? ` • b3 ${adapter.hash_b3.slice(0, 8)}…` : ''}
                        </span>
                        <span className="text-[10px] text-muted-foreground">
                          {adapter.adapter_id}
                        </span>
                      </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>

                <Button
                  onClick={handleAddAdapter}
                  disabled={!selectedAdapter || getUnusedAdapters.length === 0}
                >
                  <Plus className="h-4 w-4 mr-2" />
                  Add
                </Button>
              </div>

              {getUnusedAdapters.length === 0 && adapters.length > 0 && (
                <Alert>
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>
                    All available adapters have been added to the stack
                  </AlertDescription>
                </Alert>
              )}
            </CardContent>
          </Card>

          {/* Stack Adapters */}
          {adapters.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">
                  Stack Adapters ({adapters.length})
                </CardTitle>
                <CardDescription>
                  Drag to reorder adapters. Execution order matters.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <DndContext
                  sensors={sensors}
                  collisionDetection={closestCenter}
                  onDragEnd={handleDragEnd}
                >
                  <SortableContext
                    items={adapters.map((a) => a.adapter.adapter_id)}
                    strategy={verticalListSortingStrategy}
                  >
                    <div className="space-y-2">
                      {adapters.map((item) => (
                        <SortableAdapterItem
                          key={item.adapter.adapter_id}
                          item={item}
                          onRemove={() =>
                            handleRemoveAdapter(
                              item.adapter.adapter_id
                            )
                          }
                          onToggle={() =>
                            handleToggleAdapter(
                              item.adapter.adapter_id
                            )
                          }
                        />
                      ))}
                    </div>
                  </SortableContext>
                </DndContext>
              </CardContent>
            </Card>
          )}

          {/* Validation Status */}
          {validationReport && (
            <Card
              className={
                validationReport.isValid
                  ? 'border-green-200 bg-green-50/50'
                  : 'border-red-200 bg-red-50/50'
              }
            >
              <CardHeader className="pb-3">
                <div className="flex items-center gap-2">
                  {validationReport.isValid ? (
                    <CheckCircle2 className="h-5 w-5 text-green-600" />
                  ) : (
                    <AlertCircle className="h-5 w-5 text-red-600" />
                  )}
                  <CardTitle className="text-base">
                    {validationReport.isValid
                      ? 'Stack is Valid'
                      : 'Stack has Issues'}
                  </CardTitle>
                </div>
              </CardHeader>
              <CardContent>
                <p className="text-sm text-muted-foreground">
                  {validationReport.issues.filter((i) => i.level === 'error').length > 0
                    ? `${validationReport.issues.filter((i) => i.level === 'error').length} error(s), `
                    : ''}
                  {validationReport.issues.filter((i) => i.level === 'warning').length > 0
                    ? `${validationReport.issues.filter((i) => i.level === 'warning').length} warning(s)`
                    : 'No issues found'}
                </p>
              </CardContent>
            </Card>
          )}

          {/* Action Buttons */}
          <div className="flex gap-2">
            <Button
              onClick={() => setShowPreview(true)}
              variant="outline"
              disabled={adapters.length === 0}
            >
              <Eye className="h-4 w-4 mr-2" />
              Preview & Test
            </Button>

            <Button
              onClick={handleSaveStack}
              disabled={!canSaveStack || !validationReport?.isValid || isSaving}
            >
              <Save className="h-4 w-4 mr-2" />
              {isSaving ? 'Saving...' : 'Save Stack'}
            </Button>
          </div>
        </TabsContent>

        <TabsContent value="preview">
          {adapters.length > 0 && (
            <StackPreview
              adapters={adapters}
              stackName={stackName}
              stackId={initialStackId}
              onValidation={setValidationReport}
              onTestInference={setTestResult}
            />
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
};

export type { StackAdapter };
