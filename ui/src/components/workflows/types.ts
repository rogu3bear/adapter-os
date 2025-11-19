// Workflow template types for AdapterOS

export interface WorkflowTemplate {
  id: string;
  name: string;
  description: string;
  category: WorkflowCategory;
  steps: WorkflowStep[];
  requiredInputs: WorkflowInput[];
  estimatedDuration: string;
  tags: string[];
  difficulty: 'beginner' | 'intermediate' | 'advanced';
  icon?: string;
}

export type WorkflowCategory =
  | 'training'
  | 'deployment'
  | 'experimental'
  | 'comparison'
  | 'stack'
  | 'maintenance';

export interface WorkflowStep {
  id: string;
  title: string;
  description: string;
  component: string; // Component name to render
  config: Record<string, any>; // Step-specific configuration
  validation?: WorkflowValidation;
  skip?: WorkflowCondition;
  required?: boolean;
  helpText?: string;
}

export interface WorkflowInput {
  id: string;
  label: string;
  type: 'text' | 'number' | 'select' | 'file' | 'directory' | 'adapter' | 'dataset' | 'stack';
  required: boolean;
  default?: any;
  options?: Array<{ label: string; value: string }>;
  placeholder?: string;
  helpText?: string;
}

export interface WorkflowValidation {
  type: 'required' | 'min' | 'max' | 'pattern' | 'custom';
  value?: any;
  message: string;
  validate?: (data: any) => boolean;
}

export interface WorkflowCondition {
  field: string;
  operator: 'equals' | 'notEquals' | 'contains' | 'notContains';
  value: any;
}

export interface WorkflowExecution {
  id: string;
  templateId: string;
  templateName: string;
  status: WorkflowStatus;
  startedAt: string;
  completedAt?: string;
  currentStep: number;
  totalSteps: number;
  inputs: Record<string, any>;
  outputs: Record<string, any>;
  error?: string;
  results?: WorkflowResult[];
}

export type WorkflowStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface WorkflowResult {
  stepId: string;
  stepTitle: string;
  status: 'success' | 'failure' | 'skipped';
  data: any;
  duration?: number;
  error?: string;
}

export interface WorkflowProgress {
  currentStep: number;
  totalSteps: number;
  stepStatus: Record<string, 'pending' | 'running' | 'completed' | 'failed' | 'skipped'>;
  data: Record<string, any>;
  startedAt: string;
  lastUpdate: string;
}

// Saved workflow state for resume capability
export interface SavedWorkflowState {
  executionId: string;
  templateId: string;
  currentStep: number;
  data: Record<string, any>;
  savedAt: string;
}
