// TemplateCustomizer component - Customize and save workflow templates

import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Textarea } from '../ui/textarea';
import { Badge } from '../ui/badge';
import { Switch } from '../ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import {
  Save,
  Copy,
  Download,
  Upload,
  Plus,
  Trash2,
  ArrowUp,
  ArrowDown,
  Edit,
} from 'lucide-react';
import { WorkflowTemplate, WorkflowStep, WorkflowInput } from './types';
import { toast } from 'sonner';

interface TemplateCustomizerProps {
  template: WorkflowTemplate;
  onSave: (customTemplate: WorkflowTemplate) => void;
  onCancel: () => void;
}

export function TemplateCustomizer({ template, onSave, onCancel }: TemplateCustomizerProps) {
  const [customTemplate, setCustomTemplate] = useState<WorkflowTemplate>({
    ...template,
    id: `custom-${Date.now()}`,
  });
  const [editingStep, setEditingStep] = useState<number | null>(null);

  const updateTemplate = (updates: Partial<WorkflowTemplate>) => {
    setCustomTemplate((prev) => ({ ...prev, ...updates }));
  };

  const updateStep = (index: number, updates: Partial<WorkflowStep>) => {
    const updatedSteps = [...customTemplate.steps];
    updatedSteps[index] = { ...updatedSteps[index], ...updates };
    updateTemplate({ steps: updatedSteps });
  };

  const deleteStep = (index: number) => {
    if (confirm('Are you sure you want to delete this step?')) {
      const updatedSteps = customTemplate.steps.filter((_, i) => i !== index);
      updateTemplate({ steps: updatedSteps });
      toast.success('Step deleted');
    }
  };

  const moveStep = (index: number, direction: 'up' | 'down') => {
    const newIndex = direction === 'up' ? index - 1 : index + 1;
    if (newIndex < 0 || newIndex >= customTemplate.steps.length) return;

    const updatedSteps = [...customTemplate.steps];
    [updatedSteps[index], updatedSteps[newIndex]] = [
      updatedSteps[newIndex],
      updatedSteps[index],
    ];
    updateTemplate({ steps: updatedSteps });
  };

  const addStep = () => {
    const newStep: WorkflowStep = {
      id: `step-${Date.now()}`,
      title: 'New Step',
      description: 'Description for new step',
      component: 'CustomComponent',
      config: {},
      required: true,
    };
    updateTemplate({ steps: [...customTemplate.steps, newStep] });
    toast.success('Step added');
  };

  const handleSave = () => {
    if (!customTemplate.name.trim()) {
      toast.error('Template name is required');
      return;
    }

    if (customTemplate.steps.length === 0) {
      toast.error('At least one step is required');
      return;
    }

    onSave(customTemplate);
    toast.success('Custom template saved');
  };

  const handleExport = () => {
    const dataStr = JSON.stringify(customTemplate, null, 2);
    const dataBlob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(dataBlob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `workflow-template-${customTemplate.id}.json`;
    link.click();
    URL.revokeObjectURL(url);
    toast.success('Template exported');
  };

  const handleImport = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const imported = JSON.parse(e.target?.result as string) as WorkflowTemplate;
        setCustomTemplate(imported);
        toast.success('Template imported');
      } catch (error) {
        toast.error('Failed to import template');
      }
    };
    reader.readAsText(file);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">Customize Template</h2>
          <p className="text-sm text-muted-foreground mt-1">
            Modify and save your custom workflow template
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="file"
            accept="application/json"
            onChange={handleImport}
            className="hidden"
            id="import-template"
          />
          <Button variant="outline" size="sm" asChild>
            <label htmlFor="import-template" className="cursor-pointer">
              <Upload className="h-4 w-4 mr-2" />
              Import
            </label>
          </Button>
          <Button variant="outline" size="sm" onClick={handleExport}>
            <Download className="h-4 w-4 mr-2" />
            Export
          </Button>
        </div>
      </div>

      {/* Basic Info */}
      <Card>
        <CardHeader>
          <CardTitle>Basic Information</CardTitle>
          <CardDescription>Template name, description, and metadata</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="name">Template Name</Label>
              <Input
                id="name"
                value={customTemplate.name}
                onChange={(e) => updateTemplate({ name: e.target.value })}
                placeholder="My Custom Workflow"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="category">Category</Label>
              <Select
                value={customTemplate.category}
                onValueChange={(value: any) => updateTemplate({ category: value })}
              >
                <SelectTrigger id="category">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="training">Training</SelectItem>
                  <SelectItem value="deployment">Deployment</SelectItem>
                  <SelectItem value="experimental">Experimental</SelectItem>
                  <SelectItem value="comparison">Comparison</SelectItem>
                  <SelectItem value="stack">Stack Management</SelectItem>
                  <SelectItem value="maintenance">Maintenance</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="description">Description</Label>
            <Textarea
              id="description"
              value={customTemplate.description}
              onChange={(e) => updateTemplate({ description: e.target.value })}
              placeholder="Describe your workflow template..."
              rows={3}
            />
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="space-y-2">
              <Label htmlFor="duration">Estimated Duration</Label>
              <Input
                id="duration"
                value={customTemplate.estimatedDuration}
                onChange={(e) => updateTemplate({ estimatedDuration: e.target.value })}
                placeholder="10 minutes"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="difficulty">Difficulty</Label>
              <Select
                value={customTemplate.difficulty}
                onValueChange={(value: any) => updateTemplate({ difficulty: value })}
              >
                <SelectTrigger id="difficulty">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="beginner">Beginner</SelectItem>
                  <SelectItem value="intermediate">Intermediate</SelectItem>
                  <SelectItem value="advanced">Advanced</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label htmlFor="tags">Tags (comma-separated)</Label>
              <Input
                id="tags"
                value={customTemplate.tags.join(', ')}
                onChange={(e) =>
                  updateTemplate({
                    tags: e.target.value.split(',').map((t) => t.trim()).filter(Boolean),
                  })
                }
                placeholder="training, quick, experimental"
              />
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Steps */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Workflow Steps</CardTitle>
              <CardDescription>
                Configure the steps in your workflow (drag to reorder)
              </CardDescription>
            </div>
            <Button onClick={addStep} size="sm">
              <Plus className="h-4 w-4 mr-2" />
              Add Step
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {customTemplate.steps.map((step, index) => (
            <Card key={step.id} className="border-2">
              <CardContent className="p-4">
                <div className="flex items-start gap-3">
                  {/* Step Number */}
                  <div className="flex flex-col gap-1">
                    <Badge variant="outline" className="w-8 h-8 flex items-center justify-center">
                      {index + 1}
                    </Badge>
                    <div className="flex flex-col gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => moveStep(index, 'up')}
                        disabled={index === 0}
                        className="h-6 w-8 p-0"
                      >
                        <ArrowUp className="h-3 w-3" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => moveStep(index, 'down')}
                        disabled={index === customTemplate.steps.length - 1}
                        className="h-6 w-8 p-0"
                      >
                        <ArrowDown className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>

                  {/* Step Content */}
                  <div className="flex-1 space-y-3">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      <div className="space-y-1">
                        <Label className="text-xs">Title</Label>
                        <Input
                          value={step.title}
                          onChange={(e) => updateStep(index, { title: e.target.value })}
                          placeholder="Step title"
                          className="h-8 text-sm"
                        />
                      </div>
                      <div className="space-y-1">
                        <Label className="text-xs">Component</Label>
                        <Input
                          value={step.component}
                          onChange={(e) => updateStep(index, { component: e.target.value })}
                          placeholder="ComponentName"
                          className="h-8 text-sm"
                        />
                      </div>
                    </div>

                    <div className="space-y-1">
                      <Label className="text-xs">Description</Label>
                      <Textarea
                        value={step.description}
                        onChange={(e) => updateStep(index, { description: e.target.value })}
                        placeholder="Step description"
                        rows={2}
                        className="text-sm"
                      />
                    </div>

                    <div className="flex items-center gap-4">
                      <div className="flex items-center gap-2">
                        <Switch
                          checked={step.required !== false}
                          onCheckedChange={(checked) => updateStep(index, { required: checked })}
                          id={`required-${index}`}
                        />
                        <Label htmlFor={`required-${index}`} className="text-xs">
                          Required Step
                        </Label>
                      </div>
                    </div>
                  </div>

                  {/* Actions */}
                  <div className="flex flex-col gap-1">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setEditingStep(index)}
                      className="h-8 w-8 p-0"
                    >
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => deleteStep(index)}
                      className="h-8 w-8 p-0 text-destructive"
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}

          {customTemplate.steps.length === 0 && (
            <div className="text-center py-8 text-muted-foreground">
              No steps yet. Click "Add Step" to create your workflow.
            </div>
          )}
        </CardContent>
      </Card>

      {/* Actions */}
      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button onClick={handleSave}>
          <Save className="h-4 w-4 mr-2" />
          Save Custom Template
        </Button>
      </div>
    </div>
  );
}
