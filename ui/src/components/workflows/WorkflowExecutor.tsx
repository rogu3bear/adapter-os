// WorkflowExecutor component - Execute workflow steps with wizard interface

import React, { useState, useEffect, useCallback } from 'react';
import { Wizard, WizardStep } from '../ui/wizard';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Alert, AlertDescription } from '../ui/alert';
import { Badge } from '../ui/badge';
import {
  AlertTriangle,
  CheckCircle,
  Info,
  Loader2,
  RotateCcw,
  Save,
  XCircle,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  WorkflowTemplate,
  WorkflowExecution,
  WorkflowProgress as WorkflowProgressType,
  WorkflowResult,
  SavedWorkflowState,
} from './types';
import { WorkflowProgress } from './WorkflowProgress';

interface WorkflowExecutorProps {
  template: WorkflowTemplate;
  initialInputs?: Record<string, any>;
  onComplete: (execution: WorkflowExecution) => void;
  onCancel: () => void;
  savedState?: SavedWorkflowState;
}

export function WorkflowExecutor({
  template,
  initialInputs = {},
  onComplete,
  onCancel,
  savedState,
}: WorkflowExecutorProps) {
  const [currentStep, setCurrentStep] = useState(savedState?.currentStep || 0);
  const [workflowData, setWorkflowData] = useState<Record<string, any>>(
    savedState?.data || initialInputs
  );
  const [isExecuting, setIsExecuting] = useState(false);
  const [executionError, setExecutionError] = useState<string | null>(null);
  const [results, setResults] = useState<WorkflowResult[]>([]);
  const [startTime] = useState(savedState?.savedAt || new Date().toISOString());

  // Auto-save progress
  useEffect(() => {
    const saveState: SavedWorkflowState = {
      executionId: `exec-${Date.now()}`,
      templateId: template.id,
      currentStep,
      data: workflowData,
      savedAt: new Date().toISOString(),
    };
    localStorage.setItem(`workflow-${template.id}`, JSON.stringify(saveState));
  }, [template.id, currentStep, workflowData]);

  const updateData = (stepId: string, data: any) => {
    setWorkflowData((prev) => ({
      ...prev,
      [stepId]: data,
    }));
  };

  const shouldSkipStep = (step: any): boolean => {
    if (!step.skip) return false;

    const { field, operator, value } = step.skip;
    const fieldValue = workflowData[field];

    switch (operator) {
      case 'equals':
        return fieldValue === value;
      case 'notEquals':
        return fieldValue !== value;
      case 'contains':
        return Array.isArray(fieldValue) && fieldValue.includes(value);
      case 'notContains':
        return Array.isArray(fieldValue) && !fieldValue.includes(value);
      default:
        return false;
    }
  };

  const validateStep = async (step: any): Promise<boolean> => {
    if (!step.validation) return true;

    const validation = step.validation;

    switch (validation.type) {
      case 'required':
        if (!workflowData[step.id]) {
          toast.error(validation.message);
          return false;
        }
        break;

      case 'custom':
        if (validation.validate && !validation.validate(workflowData)) {
          toast.error(validation.message);
          return false;
        }
        break;

      default:
        break;
    }

    return true;
  };

  const executeStep = async (step: any): Promise<WorkflowResult> => {
    const stepStartTime = Date.now();

    try {
      // Check if step should be skipped
      if (shouldSkipStep(step)) {
        return {
          stepId: step.id,
          stepTitle: step.title,
          status: 'skipped',
          data: null,
          duration: Date.now() - stepStartTime,
        };
      }

      // Validate step
      const isValid = await validateStep(step);
      if (!isValid) {
        return {
          stepId: step.id,
          stepTitle: step.title,
          status: 'failure',
          data: null,
          duration: Date.now() - stepStartTime,
          error: 'Validation failed',
        };
      }

      // Simulate step execution (in real implementation, this would call actual APIs)
      await new Promise((resolve) => setTimeout(resolve, 1000));

      // Mark step as successful
      return {
        stepId: step.id,
        stepTitle: step.title,
        status: 'success',
        data: workflowData[step.id] || null,
        duration: Date.now() - stepStartTime,
      };
    } catch (error) {
      return {
        stepId: step.id,
        stepTitle: step.title,
        status: 'failure',
        data: null,
        duration: Date.now() - stepStartTime,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  };

  const handleComplete = async () => {
    setIsExecuting(true);
    setExecutionError(null);

    try {
      // Execute all remaining steps
      const remainingSteps = template.steps.slice(currentStep);
      const stepResults: WorkflowResult[] = [];

      for (const step of remainingSteps) {
        const result = await executeStep(step);
        stepResults.push(result);
        setResults((prev) => [...prev, result]);

        if (result.status === 'failure' && step.required !== false) {
          setExecutionError(`Step "${step.title}" failed: ${result.error}`);
          setIsExecuting(false);
          return;
        }
      }

      // Create execution record
      const execution: WorkflowExecution = {
        id: `exec-${Date.now()}`,
        templateId: template.id,
        templateName: template.name,
        status: 'completed',
        startedAt: startTime,
        completedAt: new Date().toISOString(),
        currentStep: template.steps.length,
        totalSteps: template.steps.length,
        inputs: initialInputs,
        outputs: workflowData,
        results: stepResults,
      };

      // Clear saved state
      localStorage.removeItem(`workflow-${template.id}`);

      toast.success('Workflow completed successfully!');
      onComplete(execution);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      setExecutionError(errorMessage);
      toast.error(`Workflow failed: ${errorMessage}`);
    } finally {
      setIsExecuting(false);
    }
  };

  const handleSave = () => {
    const saveState: SavedWorkflowState = {
      executionId: `exec-${Date.now()}`,
      templateId: template.id,
      currentStep,
      data: workflowData,
      savedAt: new Date().toISOString(),
    };
    localStorage.setItem(`workflow-${template.id}`, JSON.stringify(saveState));
    toast.success('Workflow progress saved');
  };

  const handleReset = () => {
    if (confirm('Are you sure you want to reset this workflow? All progress will be lost.')) {
      setCurrentStep(0);
      setWorkflowData(initialInputs);
      setResults([]);
      setExecutionError(null);
      localStorage.removeItem(`workflow-${template.id}`);
      toast.info('Workflow reset');
    }
  };

  // Build wizard steps
  const wizardSteps: WizardStep[] = template.steps.map((step) => ({
    id: step.id,
    title: step.title,
    description: step.description,
    component: (
      <div className="space-y-4">
        {/* Step Configuration Card */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{step.title}</CardTitle>
            <CardDescription>{step.description}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* Help Text */}
            {step.helpText && (
              <Alert>
                <Info className="h-4 w-4" />
                <AlertDescription>{step.helpText}</AlertDescription>
              </Alert>
            )}

            {/* Component Placeholder */}
            <div className="p-4 border rounded-lg bg-muted/20">
              <p className="text-sm text-muted-foreground">
                Component: <span className="font-mono">{step.component}</span>
              </p>
              <p className="text-xs text-muted-foreground mt-2">
                Configuration: {JSON.stringify(step.config, null, 2)}
              </p>
            </div>

            {/* Required Badge */}
            {step.required !== false && (
              <Badge variant="outline" className="text-xs">
                Required Step
              </Badge>
            )}
          </CardContent>
        </Card>

        {/* Step Results */}
        {results.find((r) => r.stepId === step.id) && (
          <Card>
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                {results.find((r) => r.stepId === step.id)?.status === 'success' ? (
                  <CheckCircle className="h-4 w-4 text-green-500" />
                ) : (
                  <XCircle className="h-4 w-4 text-red-500" />
                )}
                Step Result
              </CardTitle>
            </CardHeader>
            <CardContent>
              <pre className="text-xs bg-muted p-3 rounded overflow-auto">
                {JSON.stringify(results.find((r) => r.stepId === step.id), null, 2)}
              </pre>
            </CardContent>
          </Card>
        )}
      </div>
    ),
    validate: async () => {
      const isValid = await validateStep(step);
      return isValid;
    },
  }));

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">{template.name}</h2>
          <p className="text-sm text-muted-foreground mt-1">{template.description}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleSave} disabled={isExecuting}>
            <Save className="h-4 w-4 mr-2" />
            Save Progress
          </Button>
          <Button variant="outline" size="sm" onClick={handleReset} disabled={isExecuting}>
            <RotateCcw className="h-4 w-4 mr-2" />
            Reset
          </Button>
        </div>
      </div>

      {/* Execution Error */}
      {executionError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>{executionError}</AlertDescription>
        </Alert>
      )}

      {/* Resumed State Notice */}
      {savedState && (
        <Alert>
          <Info className="h-4 w-4" />
          <AlertDescription>
            Resumed from saved progress at step {savedState.currentStep + 1}
          </AlertDescription>
        </Alert>
      )}

      {/* Wizard */}
      <Wizard
        steps={wizardSteps}
        currentStep={currentStep}
        onStepChange={setCurrentStep}
        onComplete={handleComplete}
        onCancel={onCancel}
        completeButtonText={isExecuting ? 'Executing...' : 'Complete Workflow'}
        isLoading={isExecuting}
      />

      {/* Progress Sidebar (Compact) */}
      <div className="fixed right-4 bottom-4 w-80 z-50">
        <WorkflowProgress
          progress={{
            currentStep,
            totalSteps: template.steps.length,
            stepStatus: template.steps.reduce((acc, step, index) => {
              const result = results.find((r) => r.stepId === step.id);
              if (result) {
                acc[step.id] = result.status === 'success' ? 'completed' : 'failed';
              } else if (index === currentStep) {
                acc[step.id] = 'running';
              } else if (index < currentStep) {
                acc[step.id] = 'completed';
              } else {
                acc[step.id] = 'pending';
              }
              return acc;
            }, {} as Record<string, any>),
            data: workflowData,
            startedAt: startTime,
            lastUpdate: new Date().toISOString(),
          }}
          steps={template.steps}
          compact
        />
      </div>
    </div>
  );
}
