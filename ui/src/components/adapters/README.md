# Adapter Stack Components

Comprehensive stack composition, validation, and testing system for the AdapterOS UI.

## Overview

The adapter stack system allows users to:
1. Compose stacks by combining multiple adapters
2. Reorder adapters via drag-and-drop
3. Validate compatibility and policy compliance
4. Test inference with the stack
5. Save and update stacks

## Components

### StackPreview

Display stack validation results, metrics, and test inference.

```typescript
import { StackPreview } from '@/components/adapters';

<StackPreview
  adapters={adapters}
  stackName="production-code-review"
  stackId="stack-123"
  onValidation={(report) => console.log(report)}
  onTestInference={(result) => console.log(result)}
/>
```

**Features:**
- Real-time validation (6 rule sets)
- Error-first display
- Collapsible sections
- Compatibility score
- Stack metrics
- Test inference interface
- Dismissible issues

### AdapterStackComposer

Full workflow for building and saving stacks.

```typescript
import { AdapterStackComposer } from '@/components/adapters';

<AdapterStackComposer
  onStackCreated={(stackId, name) => console.log('Created:', stackId)}
  onStackUpdated={(stackId, adapters) => console.log('Updated:', stackId)}
/>
```

**Features:**
- Stack name/description input
- Adapter selection dropdown
- Drag-and-drop reordering
- Enable/disable toggles
- Real-time validation
- Save/update functionality
- Preview modal

### SortableAdapterItem

Individual adapter item with drag support.

**Includes:**
- Drag handle
- Order number
- Metadata badges
- Enable/disable toggle
- Quick remove button

### useStackValidation Hook

Reusable validation logic with memoization.

```typescript
import { useStackValidation } from '@/components/adapters';

const {
  isValid,
  errors,
  warnings,
  summary
} = useStackValidation(adapters, stackName);
```

## Validation Rules

### Framework Compatibility (Warning)
Detects mixing of frameworks in stack.

### Rank Compatibility (Warning)
Ensures rank variance doesn't exceed 16.

### Tier Alignment (Info)
Notes when tiers are mixed.

### Semantic Naming (Error/Warning)
- Validates stack name format
- Checks reserved names (system, admin, root, default, test)
- Validates adapter naming convention
- Enforces {tenant}/{domain}/{purpose}/{revision} format

### Router Compliance (Error/Warning)
- Requires minimum 1 adapter
- Warns if > 10 adapters (K-sparse limit)

### Policy Compliance (Error/Warning/Info)
- Retired adapters: Error
- Deprecated adapters: Warning
- Zero activations: Info
- Pinned ephemeral: Warning

## Metrics

### Parameters
Estimation: sum(adapter.rank * 1000) per enabled adapter

### Memory
Total sum of adapter memory_bytes

### Latency
(adapter_count * 2.5ms) + 5ms base

### Compatibility Score
- Start: 100 points
- Retired adapters: -50
- Deprecated adapters: -20
- Oversized stack (>10): -15
- High rank variance (>16): -10

## Integration

### In AdaptersPage

```typescript
import { AdapterStackComposer } from '@/components/adapters';

export function AdaptersPage() {
  return (
    <AdapterStackComposer
      onStackCreated={(stackId, name) => {
        // Refresh stack list
      }}
    />
  );
}
```

### Direct Preview

```typescript
import { StackPreview } from '@/components/adapters';

<StackPreview
  adapters={selectedAdapters}
  stackName={stackName}
  onValidation={handleValidation}
/>
```

### Validation Only

```typescript
import { useStackValidation } from '@/components/adapters';

const { isValid, issues, summary } = useStackValidation(adapters, stackName);

if (!isValid) {
  // Show errors
}
```

## API Requirements

### Get Adapters
```
GET /api/adapters
Response: Adapter[]
```

### Create Stack
```
POST /api/adapter-stacks
Body: {
  name: string;
  description?: string;
  adapter_ids: string[];
  adapter_order: { adapter_id: string; order: number }[];
  workflow_type: 'sequential' | 'parallel';
}
Response: { id: string; ... }
```

### Update Stack
```
PUT /api/adapter-stacks/{stackId}
Body: Same as create
Response: { id: string; ... }
```

### Test Inference
```
POST /api/inference/test
Body: {
  prompt: string;
  adapter_ids: string[];
  stack_id?: string;
}
Response: {
  output: string;
  latency_ms: number;
  adapters_applied: string[];
}
```

## Types

### StackAdapter
```typescript
interface StackAdapter {
  adapter: Adapter;      // Full adapter object from API
  order: number;         // Position in execution order (0-based)
  enabled: boolean;      // Whether adapter is active
}
```

### ValidationReport
```typescript
interface ValidationReport {
  isValid: boolean;
  issues: ValidationIssue[];
  summary: {
    totalAdapters: number;
    enabledAdapters: number;
    totalParameters: number;
    totalMemory: number;
    estimatedLatency: number;
    compatibilityScore: number;
  };
}
```

### ValidationIssue
```typescript
interface ValidationIssue {
  level: 'error' | 'warning' | 'info';
  category: string;
  message: string;
  adapter?: string;
  suggestion?: string;
}
```

### InferenceTestResult
```typescript
interface InferenceTestResult {
  success: boolean;
  prompt: string;
  output: string;
  latency: number;
  adaptersApplied: string[];
  error?: string;
}
```

## Constants

```typescript
const RESERVED_TENANTS = ['system', 'admin', 'root', 'default', 'test'];
const RESERVED_DOMAINS = ['core', 'internal', 'deprecated'];
const MAX_ADAPTERS_PER_STACK = 10;
const MAX_RANK_VARIANCE = 16;
```

## Styling

All components use Tailwind CSS with existing utility classes:
- Color scheme: Consistent with app theme
- Icons: lucide-react
- UI components: Reusable system components
- Responsive: Mobile-first design

## Accessibility

- Semantic HTML
- ARIA labels
- Keyboard navigation support
- High contrast colors
- Focus visible states
- Descriptive error messages

## Performance

- Validation memoized with useMemo
- Lazy loading via useState + useEffect
- No unnecessary re-renders
- dnd-kit optimized drag-and-drop
- Transform-based animations

## Testing

### Unit Tests
- Each validation rule independently
- Edge cases (empty, single, many adapters)
- Metrics calculation accuracy

### Integration Tests
- Full composer workflow
- Drag-and-drop reordering
- Add/remove/toggle operations
- Validation report generation

### E2E Tests
- Create new stack
- Edit existing stack
- Preview and validate
- Test inference
- Save and update

## File Structure

```
ui/src/components/adapters/
├── StackPreview.tsx                  # Main preview component
├── AdapterStackComposer.tsx          # Composer component
├── SortableAdapterItem.tsx           # Draggable item
├── useStackValidation.ts             # Validation hook
├── index.ts                          # Public exports
├── VALIDATION_SPEC.md                # Detailed spec
├── IMPLEMENTATION_SUMMARY.md         # Implementation notes
└── README.md                         # This file
```

## Dependencies

### UI Components (Existing)
- Card, Badge, Button
- Input, Label, Textarea
- Alert, Progress
- Dialog, Tabs, Select
- Collapsible

### External Libraries
- @dnd-kit/core, @dnd-kit/sortable
- lucide-react
- React hooks

### Internal
- apiClient from ui/src/api/client
- Adapter type from ui/src/api/types

## Usage Examples

### Create and Save Stack
```typescript
import { AdapterStackComposer } from '@/components/adapters';

function CreateStackPage() {
  return (
    <AdapterStackComposer
      onStackCreated={(stackId, name) => {
        navigate(`/stacks/${stackId}`);
      }}
    />
  );
}
```

### Edit Existing Stack
```typescript
import { AdapterStackComposer } from '@/components/adapters';

function EditStackPage({ stackId }) {
  const [stack, setStack] = useState(null);

  useEffect(() => {
    // Load stack from API
  }, [stackId]);

  return (
    <AdapterStackComposer
      initialStackId={stackId}
      initialStackName={stack?.name}
      initialAdapters={stack?.adapters}
      onStackUpdated={(id) => {
        // Refresh or navigate
      }}
    />
  );
}
```

### Preview Only
```typescript
import { StackPreview } from '@/components/adapters';

function StackDetailsPage({ adapters, stackName }) {
  return (
    <StackPreview
      adapters={adapters}
      stackName={stackName}
      onValidation={(report) => {
        console.log('Compatibility score:', report.summary.compatibilityScore);
      }}
    />
  );
}
```

### Validation and Metrics
```typescript
import { useStackValidation } from '@/components/adapters';

function StackSummary({ adapters, stackName }) {
  const { isValid, summary, errors } = useStackValidation(adapters, stackName);

  return (
    <div>
      <p>Status: {isValid ? 'Valid' : 'Invalid'}</p>
      <p>Compatibility: {summary.compatibilityScore}%</p>
      <p>Memory: {summary.totalMemory} bytes</p>
      {errors.length > 0 && (
        <div>
          {errors.map((err) => (
            <div key={err.message}>{err.message}</div>
          ))}
        </div>
      )}
    </div>
  );
}
```

## Future Enhancements

1. **Batch Operations:** Apply settings to multiple adapters
2. **Stack Templates:** Pre-built stacks for common patterns
3. **A/B Testing:** Create stack variants
4. **Version Control:** Stack version history
5. **Advanced Scheduling:** Time-based stack switching
6. **Performance Profiling:** Real-time metrics during inference
7. **Cost Analysis:** Estimated compute cost
8. **Dependency Graphs:** Visualize adapter dependencies

## Troubleshooting

### Stack Not Valid
Check:
- No adapters enabled (must have at least 1)
- Retired adapters present (must be removed)
- Reserved names in stack name
- Non-compliant adapter naming format

### Test Inference Fails
Check:
- Stack is valid (no errors)
- Prompt is not empty
- API endpoint is accessible
- Adapters are loaded

### Drag-and-Drop Not Working
Check:
- Browser supports pointer events
- dnd-kit is properly installed
- No CSS conflicts with z-index

## Support

For issues or feature requests, refer to:
- VALIDATION_SPEC.md - Detailed validation rules
- IMPLEMENTATION_SUMMARY.md - Implementation details
- Main project CONTRIBUTING.md - Contribution guidelines
