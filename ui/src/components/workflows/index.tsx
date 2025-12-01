// Workflow system main component - Orchestrates templates, executor, and history

import React, { useState } from 'react';
import { WorkflowTemplates } from './WorkflowTemplates';
import { WorkflowExecutor } from './WorkflowExecutor';
import { WorkflowHistory } from './WorkflowHistory';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { WorkflowTemplate, WorkflowExecution } from './types';
import { useWorkflowPersistence } from '@/hooks/useWorkflowPersistence';
import { toast } from 'sonner';
import { ChevronLeft, History, Layout } from 'lucide-react';

export function WorkflowSystem() {
  const [selectedTemplate, setSelectedTemplate] = useState<WorkflowTemplate | null>(null);
  const [activeTab, setActiveTab] = useState('templates');

  const {
    savedState,
    clearState,
    executions,
    saveExecution,
    deleteExecution,
  } = useWorkflowPersistence({ storageKey: 'workflow-system' });

  const handleSelectTemplate = (template: WorkflowTemplate) => {
    setSelectedTemplate(template);
    setActiveTab('executor');
  };

  const handleComplete = (execution: WorkflowExecution) => {
    saveExecution(execution);
    setSelectedTemplate(null);
    setActiveTab('history');
    clearState();
    toast.success('Workflow completed successfully!');
  };

  const handleCancel = () => {
    if (confirm('Are you sure you want to cancel this workflow?')) {
      setSelectedTemplate(null);
      setActiveTab('templates');
    }
  };

  const handleReplay = (execution: WorkflowExecution) => {
    // Find template and replay with same inputs
    const template = { id: execution.templateId, name: execution.templateName };
    toast.info('Replay functionality coming soon!');
  };

  const handleExport = (execution: WorkflowExecution) => {
    const dataStr = JSON.stringify(execution, null, 2);
    const dataBlob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(dataBlob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `workflow-${execution.id}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Workflow Automation</h1>
        <p className="text-muted-foreground mt-2">
          Streamline common tasks with pre-configured workflow templates
        </p>
      </div>

      {/* Executor View (Full Width) */}
      {selectedTemplate ? (
        <div>
          <Button
            variant="ghost"
            className="mb-4"
            onClick={() => setSelectedTemplate(null)}
          >
            <ChevronLeft className="h-4 w-4 mr-2" />
            Back to Templates
          </Button>
          <WorkflowExecutor
            template={selectedTemplate}
            onComplete={handleComplete}
            onCancel={handleCancel}
            savedState={savedState || undefined}
          />
        </div>
      ) : (
        /* Tabbed View (Templates & History) */
        <Tabs value={activeTab} onValueChange={setActiveTab}>
          <TabsList>
            <TabsTrigger value="templates">
              <Layout className="h-4 w-4 mr-2" />
              Templates
            </TabsTrigger>
            <TabsTrigger value="history">
              <History className="h-4 w-4 mr-2" />
              History ({executions.length})
            </TabsTrigger>
          </TabsList>

          <TabsContent value="templates" className="mt-6">
            <WorkflowTemplates onSelectTemplate={handleSelectTemplate} />
          </TabsContent>

          <TabsContent value="history" className="mt-6">
            <WorkflowHistory
              executions={executions}
              onReplay={handleReplay}
              onDelete={deleteExecution}
              onExport={handleExport}
            />
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}

// Export all workflow components
export { WorkflowTemplates } from './WorkflowTemplates';
export { WorkflowExecutor } from './WorkflowExecutor';
export { WorkflowProgress } from './WorkflowProgress';
export { WorkflowHistory } from './WorkflowHistory';
export * from './types';
export * from './templates';
