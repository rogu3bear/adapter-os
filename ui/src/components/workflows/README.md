# Workflow Templates System

A comprehensive workflow automation system for AdapterOS that provides reusable templates for common scenarios.

## Overview

The Workflow Templates System streamlines repetitive tasks by providing pre-configured, customizable workflows. Users can select templates, execute multi-step workflows, track progress, and review execution history.

## Architecture

```
workflows/
├── types.ts                  # Type definitions
├── templates.ts              # Pre-built workflow templates
├── WorkflowTemplates.tsx     # Template selector/browser
├── WorkflowExecutor.tsx      # Workflow execution engine
├── WorkflowProgress.tsx      # Progress tracking component
├── WorkflowHistory.tsx       # Execution history viewer
├── TemplateCustomizer.tsx    # Template customization editor
├── index.tsx                 # Main orchestrator component
└── README.md                 # This file
```

## Components

### 1. WorkflowSystem (Main Component)

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/index.tsx`

The main orchestrator component that manages the overall workflow experience.

**Usage:**
```tsx
import { WorkflowSystem } from './components/workflows';

function App() {
  return <WorkflowSystem />;
}
```

**Features:**
- Tab-based navigation (Templates / History)
- Template selection and execution
- Automatic state persistence
- Execution history tracking

---

### 2. WorkflowTemplates

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/WorkflowTemplates.tsx`

Template browser and selector with search, filtering, and favorites.

**Props:**
```typescript
interface WorkflowTemplatesProps {
  onSelectTemplate: (template: WorkflowTemplate) => void;
  onCancel?: () => void;
}
```

**Features:**
- Search by name, description, or tags
- Filter by category and difficulty
- Favorite templates
- Visual template cards
- Category icons and badges

**Example:**
```tsx
<WorkflowTemplates
  onSelectTemplate={(template) => console.log(template)}
/>
```

---

### 3. WorkflowExecutor

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/WorkflowExecutor.tsx`

Wizard-based workflow execution engine with step validation and progress tracking.

**Props:**
```typescript
interface WorkflowExecutorProps {
  template: WorkflowTemplate;
  initialInputs?: Record<string, any>;
  onComplete: (execution: WorkflowExecution) => void;
  onCancel: () => void;
  savedState?: SavedWorkflowState;
}
```

**Features:**
- Step-by-step wizard interface
- Automatic state persistence
- Resume capability
- Step validation
- Skip logic for optional steps
- Rollback on failure
- Real-time progress tracking

**Example:**
```tsx
<WorkflowExecutor
  template={selectedTemplate}
  onComplete={(execution) => console.log('Done!', execution)}
  onCancel={() => console.log('Cancelled')}
/>
```

---

### 4. WorkflowProgress

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/WorkflowProgress.tsx`

Visual progress tracker for workflow executions.

**Props:**
```typescript
interface WorkflowProgressProps {
  progress: WorkflowProgress;
  steps: WorkflowStep[];
  compact?: boolean;
}
```

**Features:**
- Overall progress bar
- Step-by-step status indicators
- Elapsed time tracking
- Success/failure badges
- Compact mode for sidebars
- Summary statistics

**Example:**
```tsx
<WorkflowProgress
  progress={currentProgress}
  steps={template.steps}
  compact={true}
/>
```

---

### 5. WorkflowHistory

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/WorkflowHistory.tsx`

Execution history viewer with search, filtering, and replay capabilities.

**Props:**
```typescript
interface WorkflowHistoryProps {
  executions: WorkflowExecution[];
  onReplay?: (execution: WorkflowExecution) => void;
  onDelete?: (executionId: string) => void;
  onExport?: (execution: WorkflowExecution) => void;
}
```

**Features:**
- Searchable execution table
- Status filtering
- Success rate visualization
- Detailed execution viewer
- Replay functionality
- Export to JSON
- Execution deletion

**Example:**
```tsx
<WorkflowHistory
  executions={pastExecutions}
  onReplay={(exec) => console.log('Replaying', exec)}
  onExport={(exec) => downloadJSON(exec)}
/>
```

---

### 6. TemplateCustomizer

**Location:** `/Users/star/Dev/aos/ui/src/components/workflows/TemplateCustomizer.tsx`

Template editor for creating custom workflows.

**Props:**
```typescript
interface TemplateCustomizerProps {
  template: WorkflowTemplate;
  onSave: (customTemplate: WorkflowTemplate) => void;
  onCancel: () => void;
}
```

**Features:**
- Visual template editor
- Add/remove/reorder steps
- Step configuration editor
- Import/export templates
- Validation

**Example:**
```tsx
<TemplateCustomizer
  template={baseTemplate}
  onSave={(custom) => saveCustomTemplate(custom)}
  onCancel={() => closEditor()}
/>
```

---

## Pre-Built Templates

### 1. Quick Training
**ID:** `quick-training`
**Duration:** 5 minutes
**Difficulty:** Beginner

Fast adapter training with sensible defaults.

**Steps:**
1. Select Dataset
2. Configure Training (with defaults)
3. Start Training
4. Verify Results (optional)

**Use Case:** Rapid iteration during development.

---

### 2. Production Deployment
**ID:** `production-deployment`
**Duration:** 15 minutes
**Difficulty:** Advanced

Full validation and promotion workflow.

**Steps:**
1. Load Adapter
2. Validate Adapter
3. Golden Comparison
4. Promote to Production
5. Verify Deployment

**Use Case:** Deploying adapters to production with comprehensive checks.

---

### 3. Experimental Training
**ID:** `experimental-training`
**Duration:** 10 minutes
**Difficulty:** Intermediate

Rapid prototyping with ephemeral adapters.

**Steps:**
1. Upload Code
2. Auto-Configure
3. Quick Train
4. Test Adapter (optional)

**Use Case:** Quick experiments without long-term commitment.

---

### 4. Golden Run Comparison
**ID:** `golden-comparison`
**Duration:** 8 minutes
**Difficulty:** Intermediate

Compare adapters against golden baselines.

**Steps:**
1. Load Adapter
2. Load Golden Baseline
3. Run Comparison
4. Generate Report

**Use Case:** Determinism validation and regression testing.

---

### 5. Stack Creation
**ID:** `stack-creation`
**Duration:** 8 minutes
**Difficulty:** Intermediate

Compose multi-adapter stacks.

**Steps:**
1. Select Adapters
2. Order Adapters
3. Validate Stack
4. Test Stack (optional)
5. Save Stack

**Use Case:** Creating reusable adapter combinations.

---

### 6. Adapter Maintenance
**ID:** `adapter-maintenance`
**Duration:** 10 minutes
**Difficulty:** Beginner

Clean up and optimize adapters.

**Steps:**
1. Scan Adapters
2. Review Findings
3. Clean Up
4. Optimize Storage (optional)
5. Summary

**Use Case:** Periodic maintenance and cleanup.

---

## Hooks

### useWorkflowPersistence

**Location:** `/Users/star/Dev/aos/ui/src/hooks/useWorkflowPersistence.ts`

Manages workflow state persistence and execution history.

**Usage:**
```typescript
const {
  savedState,
  saveState,
  clearState,
  hasSavedState,
  executions,
  saveExecution,
  deleteExecution,
} = useWorkflowPersistence({ storageKey: 'my-workflow' });
```

**Features:**
- Auto-save workflow state
- Resume capability
- Execution history tracking
- LocalStorage-based persistence

---

## Type Definitions

### WorkflowTemplate
```typescript
interface WorkflowTemplate {
  id: string;
  name: string;
  description: string;
  category: WorkflowCategory;
  steps: WorkflowStep[];
  requiredInputs: WorkflowInput[];
  estimatedDuration: string;
  tags: string[];
  difficulty: 'beginner' | 'intermediate' | 'advanced';
}
```

### WorkflowStep
```typescript
interface WorkflowStep {
  id: string;
  title: string;
  description: string;
  component: string;
  config: Record<string, any>;
  validation?: WorkflowValidation;
  skip?: WorkflowCondition;
  required?: boolean;
  helpText?: string;
}
```

### WorkflowExecution
```typescript
interface WorkflowExecution {
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
```

---

## Creating Custom Templates

### Step 1: Define Template

```typescript
import { WorkflowTemplate } from './types';

const myTemplate: WorkflowTemplate = {
  id: 'my-custom-workflow',
  name: 'My Custom Workflow',
  description: 'Does something awesome',
  category: 'training',
  estimatedDuration: '5 minutes',
  difficulty: 'beginner',
  tags: ['custom', 'training'],
  requiredInputs: [
    {
      id: 'dataPath',
      label: 'Data Path',
      type: 'file',
      required: true,
    },
  ],
  steps: [
    {
      id: 'step-1',
      title: 'First Step',
      description: 'Do something',
      component: 'MyComponent',
      config: { option: 'value' },
      required: true,
    },
  ],
};
```

### Step 2: Register Template

Add to `templates.ts`:

```typescript
export const WORKFLOW_TEMPLATES: WorkflowTemplate[] = [
  // ... existing templates
  myTemplate,
];
```

### Step 3: Implement Step Components

Create React components for each step (referenced by `component` field).

---

## Step Validation

Add validation to ensure data quality:

```typescript
{
  id: 'my-step',
  // ...
  validation: {
    type: 'custom',
    message: 'Validation failed',
    validate: (data: any) => {
      return data.someField !== null;
    },
  },
}
```

**Validation Types:**
- `required` - Field must exist
- `min` - Minimum value/length
- `max` - Maximum value/length
- `pattern` - Regex pattern
- `custom` - Custom function

---

## Skip Logic

Conditionally skip steps based on data:

```typescript
{
  id: 'optional-step',
  // ...
  skip: {
    field: 'experimentMode',
    operator: 'equals',
    value: false,
  },
}
```

**Operators:**
- `equals` / `notEquals`
- `contains` / `notContains`

---

## Persistence

Workflows auto-save progress to LocalStorage:

**Storage Keys:**
- `workflow-state-{storageKey}` - Current workflow state
- `workflow-executions` - Execution history

**Manual Save:**
```typescript
const handleSave = () => {
  const state: SavedWorkflowState = {
    executionId: 'exec-123',
    templateId: template.id,
    currentStep: 2,
    data: workflowData,
    savedAt: new Date().toISOString(),
  };
  localStorage.setItem('workflow-state', JSON.stringify(state));
};
```

---

## Export/Import

### Export Template
```typescript
const exportTemplate = (template: WorkflowTemplate) => {
  const json = JSON.stringify(template, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `template-${template.id}.json`;
  link.click();
};
```

### Import Template
```typescript
const importTemplate = (file: File) => {
  const reader = new FileReader();
  reader.onload = (e) => {
    const template = JSON.parse(e.target?.result as string);
    // Use template
  };
  reader.readAsText(file);
};
```

---

## Integration

### Add to Navigation

```tsx
import { WorkflowSystem } from './components/workflows';

// In your router/navigation
<Route path="/workflows" element={<WorkflowSystem />} />
```

### Add to Dashboard

```tsx
import { WorkflowTemplates } from './components/workflows';

function Dashboard() {
  return (
    <div>
      <h1>Quick Actions</h1>
      <WorkflowTemplates
        onSelectTemplate={(template) => navigate(`/workflows/${template.id}`)}
      />
    </div>
  );
}
```

---

## Best Practices

1. **Template Design**
   - Keep workflows focused on single goals
   - Use 3-7 steps for optimal UX
   - Provide clear descriptions and help text
   - Set realistic duration estimates

2. **Step Components**
   - Keep components simple and focused
   - Use validation to ensure data quality
   - Provide helpful error messages
   - Support keyboard navigation

3. **Error Handling**
   - Validate inputs at each step
   - Provide recovery options
   - Log errors for debugging
   - Show user-friendly messages

4. **Performance**
   - Lazy-load step components
   - Debounce auto-save
   - Limit execution history (100 items)
   - Use compact mode for sidebars

5. **Accessibility**
   - Use semantic HTML
   - Provide keyboard shortcuts
   - Use ARIA labels
   - Support screen readers

---

## Troubleshooting

### Workflow won't resume
- Check LocalStorage for saved state
- Verify template ID matches
- Clear state and restart

### Step validation failing
- Check validation function logic
- Verify data structure
- Log data to console

### Template not appearing
- Verify template is in `WORKFLOW_TEMPLATES` array
- Check category/difficulty filters
- Search by template ID

---

## Future Enhancements

- [ ] Conditional branching (if/else workflows)
- [ ] Parallel step execution
- [ ] Workflow scheduling
- [ ] Version control for templates
- [ ] Collaborative workflows
- [ ] Visual workflow designer
- [ ] Template marketplace
- [ ] Analytics and insights
- [ ] API integration for external tools
- [ ] Mobile-responsive design

---

## Examples

### Example 1: Simple Training Workflow

```tsx
const trainingWorkflow: WorkflowTemplate = {
  id: 'simple-training',
  name: 'Simple Training',
  description: 'Train adapter with minimal configuration',
  category: 'training',
  estimatedDuration: '3 minutes',
  difficulty: 'beginner',
  tags: ['training', 'simple'],
  requiredInputs: [
    { id: 'dataset', label: 'Dataset', type: 'file', required: true },
    { id: 'name', label: 'Name', type: 'text', required: true },
  ],
  steps: [
    {
      id: 'configure',
      title: 'Configure',
      description: 'Set training parameters',
      component: 'TrainingConfig',
      config: { defaults: { rank: 8, epochs: 3 } },
    },
    {
      id: 'train',
      title: 'Train',
      description: 'Start training job',
      component: 'TrainingStarter',
      config: { showProgress: true },
    },
  ],
};
```

### Example 2: Custom Validation

```tsx
const step: WorkflowStep = {
  id: 'validate-adapter',
  title: 'Validate Adapter',
  description: 'Run validation checks',
  component: 'AdapterValidator',
  config: { checks: ['manifest', 'weights'] },
  validation: {
    type: 'custom',
    message: 'All validation checks must pass',
    validate: (data: any) => {
      return data.validationResults?.every((r: any) => r.passed);
    },
  },
};
```

---

## License

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Support

For issues or questions, refer to:
- [AGENTS.md](/Users/star/Dev/aos/AGENTS.md) - Developer guide
- [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md) - Architecture docs
