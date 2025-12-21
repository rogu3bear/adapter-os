# Integration Guide - Adapter Stack Components

Quick start guide for integrating the stack preview and validation components into existing pages.

## Quick Start (5 minutes)

### Option 1: Use AdapterStackComposer (Recommended)

Full workflow component - handles everything:

```typescript
import { AdapterStackComposer } from '@/components/adapters';

export function CreateStackPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Create Adapter Stack</h1>
      <AdapterStackComposer
        onStackCreated={(stackId, name) => {
          console.log(`Stack created: ${name} (${stackId})`);
          // Navigate or refresh
        }}
      />
    </div>
  );
}
```

### Option 2: Use Components Separately

Compose your own workflow:

```typescript
import {
  StackPreview,
  SortableAdapterItem,
  useStackValidation
} from '@/components/adapters';

export function StackDashboard() {
  const [adapters, setAdapters] = useState([]);
  const { isValid, summary } = useStackValidation(adapters);

  return (
    <div>
      {/* Your adapter list */}
      {adapters.map((item) => (
        <SortableAdapterItem
          key={item.adapter.adapter_id}
          item={item}
          onRemove={() => {}}
          onToggle={() => {}}
        />
      ))}

      {/* Preview and validation */}
      <StackPreview adapters={adapters} />
    </div>
  );
}
```

## Integration Patterns

### Pattern 1: In Adapters Page (Recommended)

```typescript
// ui/src/components/Adapters.tsx

import { AdapterStackComposer } from '@/components/adapters';

export function Adapters({ user, selectedTenant }: AdaptersProps) {
  const [activeTab, setActiveTab] = useState('registry');

  return (
    <Tabs value={activeTab} onValueChange={setActiveTab}>
      <TabsList>
        <TabsTrigger value="registry">Registry</TabsTrigger>
        <TabsTrigger value="stacks">Stacks</TabsTrigger>
      </TabsList>

      <TabsContent value="stacks">
        <AdapterStackComposer
          onStackCreated={() => {
            // Refresh stacks list
          }}
        />
      </TabsContent>
    </Tabs>
  );
}
```

### Pattern 2: Dedicated Page

```typescript
// ui/src/pages/StackComposerPage.tsx

import { AdapterStackComposer } from '@/components/adapters';
import { useNavigate } from 'react-router-dom';

export function StackComposerPage() {
  const navigate = useNavigate();

  return (
    <div className="container mx-auto py-6">
      <div className="mb-6">
        <h1 className="text-3xl font-bold">Compose Adapter Stack</h1>
        <p className="text-muted-foreground mt-2">
          Combine adapters to create powerful inference pipelines
        </p>
      </div>

      <AdapterStackComposer
        onStackCreated={(stackId) => {
          navigate(`/stacks/${stackId}`);
        }}
      />
    </div>
  );
}
```

### Pattern 3: Modal/Dialog

```typescript
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { AdapterStackComposer } from '@/components/adapters';

export function CreateStackModal({ open, onClose }) {
  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Create Adapter Stack</DialogTitle>
        </DialogHeader>

        <AdapterStackComposer
          onStackCreated={(stackId, name) => {
            console.log(`Created: ${name}`);
            onClose();
          }}
        />
      </DialogContent>
    </Dialog>
  );
}
```

## Step-by-Step Integration

### Step 1: Import Component

```typescript
import { AdapterStackComposer } from '@/components/adapters';
```

### Step 2: Add Component to JSX

```typescript
return (
  <div>
    <h2>Create Stack</h2>
    <AdapterStackComposer
      onStackCreated={(stackId, name) => {
        // Handle creation
      }}
    />
  </div>
);
```

### Step 3: Verify API Endpoints

Ensure these endpoints exist on your backend:

```
GET /api/adapters
POST /api/adapter-stacks
PUT /api/adapter-stacks/{id}
POST /api/inference/test
```

### Step 4: Test the Integration

1. Navigate to page with component
2. Select adapters from dropdown
3. Drag to reorder
4. Enter stack name
5. Click "Preview & Test"
6. Click "Save Stack"
7. Verify success message

## Advanced Usage

### Pre-populate with Adapters

```typescript
const initialAdapters = [
  {
    adapter: adapterA,
    order: 0,
    enabled: true
  },
  {
    adapter: adapterB,
    order: 1,
    enabled: true
  }
];

<AdapterStackComposer
  initialAdapters={initialAdapters}
  initialStackName="my-stack"
/>
```

### Handle Validation Results

```typescript
<StackPreview
  adapters={adapters}
  stackName={stackName}
  onValidation={(report) => {
    if (!report.isValid) {
      console.log('Errors:', report.issues.filter(i => i.level === 'error'));
    }
    console.log('Score:', report.summary.compatibilityScore);
  }}
/>
```

### Use Validation Hook Only

```typescript
import { useStackValidation } from '@/components/adapters';

function MyComponent({ adapters, stackName }) {
  const { isValid, errors, warnings, summary } = useStackValidation(
    adapters,
    stackName
  );

  if (!isValid) {
    return <ErrorDisplay errors={errors} />;
  }

  return (
    <div>
      <p>Score: {summary.compatibilityScore}%</p>
      <p>Memory: {summary.totalMemory} bytes</p>
    </div>
  );
}
```

## API Endpoints

### Required

```typescript
// Get all adapters
GET /api/adapters
Response: {
  data: Adapter[]
}

// Create stack
POST /api/adapter-stacks
Request: {
  name: string
  description?: string
  adapter_ids: string[]
  adapter_order: { adapter_id: string; order: number }[]
  workflow_type: 'sequential'
}
Response: {
  data: { id: string; ... }
}

// Update stack
PUT /api/adapter-stacks/{stackId}
Request: Same as create
Response: { data: { id: string; ... } }

// Test inference
POST /api/inference/test
Request: {
  prompt: string
  adapter_ids: string[]
  stack_id?: string
}
Response: {
  data: {
    output: string
    latency_ms: number
    adapters_applied: string[]
  }
}
```

## Type Definitions

Add to your types if needed:

```typescript
import type {
  ValidationReport,
  ValidationIssue,
  InferenceTestResult
} from '@/components/adapters';
```

## Styling

Components use existing Tailwind classes:
- `space-y-4` - Spacing between sections
- `bg-red-50`, `bg-yellow-50`, `bg-blue-50` - Alert backgrounds
- `border-red-200`, `border-yellow-200` - Alert borders
- `text-red-700`, `text-yellow-700`, `text-blue-700` - Alert text

No additional CSS needed - all styling uses Tailwind utilities.

## Troubleshooting

### Component Not Showing
- Check import path is correct
- Verify @/components/adapters exports the component
- Check browser console for errors

### Adapters List Empty
- Verify /api/adapters endpoint returns data
- Check network tab in DevTools
- Ensure apiClient is configured correctly

### Validation Not Working
- Check adapters have required fields (rank, tier, etc.)
- Verify stackName format is correct
- Check console for validation rule errors

### Drag-and-Drop Not Working
- Check @dnd-kit is installed
- Verify browser supports pointer events
- Check for CSS z-index conflicts

### Save Not Working
- Check /api/adapter-stacks endpoint exists
- Verify stack name is not empty
- Ensure at least one adapter is enabled
- Check validation passes (no errors)

### Test Inference Fails
- Verify /api/inference/test endpoint exists
- Check prompt is not empty
- Ensure stack is valid
- Check adapters are actually loaded

## Performance Tips

1. **Lazy load adapters list** - Don't fetch if tab not visible
2. **Memoize validation** - Component already does this
3. **Debounce search** - If adding search functionality
4. **Virtual scroll** - If adapter list becomes very large
5. **Code split** - Use dynamic import for large pages

## Accessibility Checklist

- [x] Component uses semantic HTML
- [x] All buttons have labels/titles
- [x] Colors not sole indicator of status
- [x] Keyboard navigation supported
- [x] Focus visible states present
- [x] Error messages descriptive
- [x] Icons accompanied by text

## Testing Checklist

- [ ] Component renders without errors
- [ ] Adapters load from API
- [ ] Drag-and-drop reorders correctly
- [ ] Add/remove adapter works
- [ ] Toggle enabled/disabled works
- [ ] Validation displays correctly
- [ ] Test inference works
- [ ] Stack saves successfully
- [ ] Error handling works
- [ ] Mobile responsive

## Deployment Checklist

- [ ] All API endpoints implemented
- [ ] Error handling configured
- [ ] Logging in place
- [ ] Performance tested
- [ ] Accessibility verified
- [ ] Cross-browser tested
- [ ] Mobile tested
- [ ] Documentation updated

## Support

For detailed information, see:
- **README.md** - Component overview and usage
- **VALIDATION_SPEC.md** - Detailed validation rules
- **IMPLEMENTATION_SUMMARY.md** - Implementation details
- **COMPLETION_REPORT.md** - Project summary

## Examples

### Create page with preview

```typescript
export function StackCreatorPage() {
  const [stackName, setStackName] = useState('');
  const [selectedAdapters, setSelectedAdapters] = useState([]);
  const { isValid, summary } = useStackValidation(selectedAdapters, stackName);

  return (
    <div className="max-w-6xl mx-auto space-y-6">
      <div>
        <h1 className="text-3xl font-bold mb-2">Create Stack</h1>
        <p className="text-muted-foreground">Compose adapters for your workflow</p>
      </div>

      <div className="grid grid-cols-3 gap-6">
        <div className="col-span-2">
          <AdapterStackComposer
            onStackCreated={() => {
              // Refresh or navigate
            }}
          />
        </div>

        <div>
          <Card>
            <CardHeader>
              <CardTitle>Preview</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <p className="text-sm text-muted-foreground">Status</p>
                <p className="text-lg font-bold">
                  {isValid ? '✓ Valid' : '✗ Invalid'}
                </p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Score</p>
                <p className="text-lg font-bold">{summary.compatibilityScore}%</p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Adapters</p>
                <p className="text-lg font-bold">{summary.enabledAdapters}</p>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
```

This integration guide should help you get started quickly. For any issues, refer to the other documentation files in this directory.
