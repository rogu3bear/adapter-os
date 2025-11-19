# Workflow Templates System - Quick Start Guide

**5-Minute Setup Guide for AdapterOS Workflow Automation**

---

## What is it?

A complete workflow automation system that provides:
- **6 pre-built templates** for common tasks
- **Step-by-step wizards** for guided execution
- **Progress tracking** with resume capability
- **Execution history** with replay and export
- **Custom templates** with visual editor

---

## Quick Start

### 1. Import and Use

```tsx
import { WorkflowSystem } from './components/workflows';

function App() {
  return <WorkflowSystem />;
}
```

That's it! The system is fully self-contained.

---

## Available Templates

### 🚀 Quick Training (5 min)
Train an adapter with minimal configuration.
```tsx
import { getTemplateById } from './components/workflows/templates';
const template = getTemplateById('quick-training');
```

### 🎯 Production Deployment (15 min)
Full validation and promotion workflow with gates.
```tsx
const template = getTemplateById('production-deployment');
```

### 🧪 Experimental Training (10 min)
Rapid prototyping with ephemeral adapters.
```tsx
const template = getTemplateById('experimental-training');
```

### 📊 Golden Comparison (8 min)
Compare adapters against golden baselines.
```tsx
const template = getTemplateById('golden-comparison');
```

### 📚 Stack Creation (8 min)
Compose multi-adapter stacks.
```tsx
const template = getTemplateById('stack-creation');
```

### 🔧 Adapter Maintenance (10 min)
Clean up expired and unused adapters.
```tsx
const template = getTemplateById('adapter-maintenance');
```

---

## Usage Patterns

### Pattern 1: Full System (Recommended)
```tsx
import { WorkflowSystem } from './components/workflows';

// Includes template browser, executor, and history
<WorkflowSystem />
```

### Pattern 2: Embedded Template Browser
```tsx
import { WorkflowTemplates } from './components/workflows';

<WorkflowTemplates
  onSelectTemplate={(template) => {
    // Handle template selection
    startWorkflow(template);
  }}
/>
```

### Pattern 3: Direct Execution
```tsx
import { WorkflowExecutor, getTemplateById } from './components/workflows';

const template = getTemplateById('quick-training');

<WorkflowExecutor
  template={template}
  initialInputs={{ datasetPath: '/data/train.json' }}
  onComplete={(execution) => console.log('Done!', execution)}
  onCancel={() => console.log('Cancelled')}
/>
```

### Pattern 4: History Only
```tsx
import { WorkflowHistory, useWorkflowPersistence } from './components/workflows';

const { executions } = useWorkflowPersistence({ storageKey: 'workflows' });

<WorkflowHistory
  executions={executions}
  onReplay={(exec) => replayWorkflow(exec)}
/>
```

---

## Key Features

### ✨ Automatic State Persistence
- Auto-saves every 5 seconds
- Resume from any step
- No configuration needed

### 🔍 Search & Filter
- Search by name, description, tags
- Filter by category, difficulty
- Favorite frequently used templates

### 📈 Progress Tracking
- Real-time step-by-step status
- Elapsed time tracking
- Success/failure indicators
- Compact sidebar mode

### 📜 Execution History
- Last 100 executions saved
- Search and filter history
- Replay past workflows
- Export to JSON

### 🎨 Template Customization
- Clone and modify templates
- Add/remove/reorder steps
- Import/export as JSON
- Visual editor

---

## Creating Custom Templates

### Step 1: Define Template
```typescript
import { WorkflowTemplate } from './components/workflows/types';

const myTemplate: WorkflowTemplate = {
  id: 'my-workflow',
  name: 'My Custom Workflow',
  description: 'Does something useful',
  category: 'training',
  estimatedDuration: '5 minutes',
  difficulty: 'beginner',
  tags: ['custom'],
  requiredInputs: [
    { id: 'input1', label: 'Input 1', type: 'text', required: true },
  ],
  steps: [
    {
      id: 'step1',
      title: 'First Step',
      description: 'Do something',
      component: 'MyComponent',
      config: {},
    },
  ],
};
```

### Step 2: Add to Templates
```typescript
// In templates.ts
export const WORKFLOW_TEMPLATES = [
  // ... existing templates
  myTemplate,
];
```

### Step 3: Implement Components
Create React components for each step referenced in `component` field.

---

## API Reference

### Components

| Component | Purpose |
|-----------|---------|
| `WorkflowSystem` | Main orchestrator with full UI |
| `WorkflowTemplates` | Template browser and selector |
| `WorkflowExecutor` | Wizard-based execution engine |
| `WorkflowProgress` | Progress tracker (compact mode available) |
| `WorkflowHistory` | Execution history viewer |
| `TemplateCustomizer` | Visual template editor |

### Hooks

| Hook | Purpose |
|------|---------|
| `useWorkflowPersistence` | State persistence and history |

### Helper Functions

| Function | Purpose |
|----------|---------|
| `getTemplateById(id)` | Get template by ID |
| `getTemplatesByCategory(category)` | Filter by category |
| `getTemplatesByTag(tag)` | Filter by tag |
| `searchTemplates(query)` | Search templates |

---

## File Structure

```
ui/src/components/workflows/
├── index.tsx                # Main entry point
├── types.ts                 # TypeScript definitions
├── templates.ts             # Pre-built templates
├── WorkflowTemplates.tsx    # Template browser
├── WorkflowExecutor.tsx     # Execution engine
├── WorkflowProgress.tsx     # Progress tracker
├── WorkflowHistory.tsx      # History viewer
├── TemplateCustomizer.tsx   # Template editor
└── README.md               # Full documentation

ui/src/hooks/
└── useWorkflowPersistence.ts # Persistence hook
```

---

## Common Recipes

### Recipe 1: Add to Navigation
```tsx
import { WorkflowSystem } from './components/workflows';

// In your router
<Route path="/workflows" element={<WorkflowSystem />} />
```

### Recipe 2: Dashboard Quick Actions
```tsx
import { WorkflowTemplates } from './components/workflows';

function Dashboard() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Quick Actions</CardTitle>
      </CardHeader>
      <CardContent>
        <WorkflowTemplates
          onSelectTemplate={(t) => navigate(`/workflows/execute/${t.id}`)}
        />
      </CardContent>
    </Card>
  );
}
```

### Recipe 3: Contextual Workflows
```tsx
import { WorkflowExecutor, getTemplateById } from './components/workflows';

function AdapterDetailsPage({ adapterId }) {
  const [showDeployment, setShowDeployment] = useState(false);
  
  return (
    <div>
      <Button onClick={() => setShowDeployment(true)}>
        Deploy to Production
      </Button>
      
      {showDeployment && (
        <WorkflowExecutor
          template={getTemplateById('production-deployment')}
          initialInputs={{ adapterId }}
          onComplete={(exec) => {
            console.log('Deployed!', exec);
            setShowDeployment(false);
          }}
          onCancel={() => setShowDeployment(false)}
        />
      )}
    </div>
  );
}
```

---

## Troubleshooting

### Workflow won't resume
```typescript
// Clear saved state
localStorage.removeItem('workflow-state-my-workflow');
```

### Template not found
```typescript
import { WORKFLOW_TEMPLATES } from './components/workflows/templates';
console.log('Available templates:', WORKFLOW_TEMPLATES.map(t => t.id));
```

### Step validation failing
```typescript
// Log step data to console
const step = template.steps[currentStep];
console.log('Step data:', workflowData[step.id]);
```

---

## Next Steps

1. **Read Full Documentation**
   - [README.md](/Users/star/Dev/aos/ui/src/components/workflows/README.md)

2. **Implement Step Components**
   - Create React components for each step type
   - Use existing patterns from TrainingWizard

3. **Integrate with APIs**
   - Connect to backend services
   - Add real functionality to template steps

4. **Customize Templates**
   - Modify existing templates
   - Create new templates for your use cases

5. **Add to Navigation**
   - Include in main app router
   - Add shortcuts in dashboard

---

## Support

- Full documentation: `ui/src/components/workflows/README.md`
- Developer guide: `CLAUDE.md`
- Implementation summary: `AGENT_25_WORKFLOW_TEMPLATES_SUMMARY.md`

---

**Ready to use!** No additional configuration required.

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
