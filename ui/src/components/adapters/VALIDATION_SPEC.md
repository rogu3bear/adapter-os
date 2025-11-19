# Stack Preview and Validation Specification

## Overview

The Stack Preview and Validation system provides comprehensive validation, testing, and composition tools for adapter stacks. It enforces policy compliance, compatibility checks, and provides actionable feedback to users.

## Components

### 1. StackPreview (StackPreview.tsx)

**Purpose:** Display stack execution preview, run validations, and test inference

**Key Features:**
- Visual validation status with error-first display
- Collapsible sections for each validation category
- Compatibility checks (framework, rank, tier)
- Policy validation (semantic naming, lifecycle, deprecation)
- Router compliance verification
- Stack metrics (parameters, memory, latency)
- Test inference interface
- Dismissible issues with explanations

**Props:**
```typescript
interface StackPreviewProps {
  adapters: StackAdapter[];
  stackName?: string;
  stackId?: string;
  onValidation?: (report: ValidationReport) => void;
  onTestInference?: (result: InferenceTestResult) => void;
}
```

**Validation Rules Implemented:**
- Framework compatibility (warning for mixed frameworks)
- Rank compatibility (warning for high variance)
- Tier alignment (info for mixed tiers)
- Semantic naming (error/warning for non-compliant names)
- Router compliance (error for 0 adapters, warning for >10)
- Policy compliance (error for retired, warning for deprecated)

**Output:**
- Validation report with issues categorized by level
- Compatibility score (0-100%)
- Stack metrics (parameters, memory, latency)
- Inference test results

### 2. AdapterStackComposer (AdapterStackComposer.tsx)

**Purpose:** Allow users to build and save adapter stacks with real-time validation

**Key Features:**
- Stack name and description input
- Dynamic adapter addition/removal via dropdown
- Drag-and-drop reordering
- Real-time validation feedback
- Preview modal integration
- Save/update stack functionality
- Toggle adapter enabled/disabled state

**Props:**
```typescript
interface AdapterStackComposerProps {
  onStackCreated?: (stackId: string, stackName: string) => void;
  onStackUpdated?: (stackId: string, adapters: StackAdapter[]) => void;
  initialStackId?: string;
  initialStackName?: string;
  initialAdapters?: StackAdapter[];
}
```

**Workflow:**
1. Enter stack name (semantic naming format validated)
2. Add adapters from dropdown (filtered to unused adapters)
3. Reorder via drag-and-drop
4. Toggle adapters on/off
5. Preview and validate
6. Save stack

### 3. SortableAdapterItem (SortableAdapterItem.tsx)

**Purpose:** Individual adapter item in stack with drag-and-drop support

**Key Features:**
- Drag handle for reordering
- Order number display
- Adapter metadata badges (category, state, lifecycle, rank, tier, memory, framework)
- Enable/disable toggle
- Quick remove button
- Visual indicators for disabled state

**Metadata Badges:**
- **Category:** code, framework, codebase, ephemeral (with icon)
- **State:** unloaded, cold, warm, hot, resident (color-coded)
- **Lifecycle:** draft, active, deprecated, retired (color-coded)
- **Rank & Tier:** numerical values
- **Memory:** formatted in MB
- **Framework:** if applicable
- **Activation Count:** if > 0

### 4. useStackValidation Hook (useStackValidation.ts)

**Purpose:** Provide reusable validation logic with memoization and categorization

**Functions:**
- `validateFrameworkCompatibility()` - Check for mixed frameworks
- `validateRankCompatibility()` - Check rank variance
- `validateTierAlignment()` - Check tier mixing
- `validateSemanticNaming()` - Check naming conventions
- `validateRouterCompliance()` - Check router constraints
- `validatePolicyCompliance()` - Check lifecycle and policy rules

**Return Value:**
```typescript
{
  report: ValidationReport;
  isValid: boolean;
  issues: ValidationIssue[];
  errors: ValidationIssue[];
  warnings: ValidationIssue[];
  infos: ValidationIssue[];
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

## Validation Rules

### Framework Compatibility
- **Level:** Warning
- **Rule:** Stack should not contain adapters from different frameworks
- **Trigger:** Multiple frameworks detected
- **Message:** `"Stack uses multiple frameworks: {list}"`
- **Suggestion:** Use adapters from the same framework for optimal performance

### Rank Compatibility
- **Level:** Warning
- **Rule:** Rank variance should not exceed 16
- **Trigger:** `max_rank - min_rank > 16`
- **Message:** `"Rank variance is {diff} (min: {min}, max: {max})"`
- **Suggestion:** Use adapters with similar ranks (max variance: 16)

### Tier Alignment
- **Level:** Info
- **Rule:** Different tiers are allowed but should be noted
- **Trigger:** `min_tier != max_tier`
- **Message:** `"Stack contains adapters from different tiers ({min}-{max})"`
- **Suggestion:** Consider using adapters from the same tier for consistency

### Semantic Naming
- **Level:** Error (for reserved names) / Warning (for non-compliant format)
- **Rules:**
  - Stack name cannot use reserved tenants: system, admin, root, default, test
  - Stack name cannot use reserved domains: core, internal, deprecated
  - Adapter names should follow: `{tenant}/{domain}/{purpose}/{revision}`
  - Revision should be in format: r### (e.g., r001, r042)
- **Suggestion:** Use semantic naming format for consistency

### Router Compliance
- **Level:** Error (for 0 adapters) / Warning (for >10 adapters)
- **Rules:**
  - Stack must have at least 1 enabled adapter
  - Stack should not exceed 10 adapters (K-sparse routing limit)
- **Suggestion:** Keep stack size <= 10 for optimal routing performance

### Policy Compliance
- **Rules:**
  - Adapters with no activation history: Info
  - Deprecated adapters: Warning
  - Retired adapters: Error (blocking)
  - Pinned ephemeral adapters: Warning
- **Suggestion:** Use active adapters, avoid deprecated/retired

## Validation Display

### Status Indicators
- **Valid Stack:** Green checkmark, shows compatibility score
- **Issues Present:** Red alert icon, shows issue summary
- **Dismissible Issues:** X button to temporarily hide non-blocking issues

### Issue Cards
```
[Icon] Category
       Message
       Suggestion: ...
       [X Dismiss]
```

### Color Coding
- **Errors:** Red (bg-red-50/border-red-200)
- **Warnings:** Yellow (bg-yellow-50/border-yellow-200)
- **Info:** Blue (bg-blue-50/border-blue-200)

## Metrics Calculation

### Total Parameters
Estimation: `sum(adapter.rank * 1000)` per enabled adapter

### Total Memory
Sum of all adapter memory bytes

### Estimated Latency
`(adapter_count * 2.5) + 5` milliseconds

### Compatibility Score (0-100)
- Start: 100
- Retired adapters: -50
- Deprecated adapters: -20
- Oversized stack (>10): -15
- High rank variance (>16): -10

## Test Inference

### Workflow
1. User enters test prompt
2. Click "Test with Stack"
3. Request sent to `/api/inference/test` with:
   - prompt
   - adapter_ids (enabled only)
   - stack_id
4. Display results:
   - Success/failure status
   - Output text
   - Latency (ms)
   - Adapters applied count

### Constraints
- Test button disabled if:
  - Stack is invalid
  - Prompt is empty
  - Test is in progress
  - No adapters enabled

## API Integration

### Create Stack
```
POST /api/adapter-stacks
{
  name: string;
  description?: string;
  adapter_ids: string[];
  adapter_order: { adapter_id: string; order: number }[];
  workflow_type: 'sequential' | 'parallel';
}
```

### Update Stack
```
PUT /api/adapter-stacks/{stackId}
{
  name?: string;
  description?: string;
  adapter_ids?: string[];
  adapter_order?: { adapter_id: string; order: number }[];
  workflow_type?: string;
}
```

### Get Adapters
```
GET /api/adapters
```

### Test Inference
```
POST /api/inference/test
{
  prompt: string;
  adapter_ids: string[];
  stack_id?: string;
}
```

## Usage Examples

### In Component
```typescript
import { AdapterStackComposer } from './components/adapters';

export function MyComponent() {
  return (
    <AdapterStackComposer
      onStackCreated={(stackId, name) => console.log('Created:', stackId)}
      onStackUpdated={(stackId, adapters) => console.log('Updated:', stackId)}
    />
  );
}
```

### Using Validation Hook
```typescript
import { useStackValidation } from './components/adapters';

const { isValid, errors, warnings, summary } = useStackValidation(adapters, stackName);

if (!isValid) {
  console.log('Stack has errors:', errors);
}
```

### Using StackPreview Directly
```typescript
import { StackPreview } from './components/adapters';

<StackPreview
  adapters={adapters}
  stackName="production-code-review"
  onValidation={(report) => console.log(report)}
  onTestInference={(result) => console.log(result)}
/>
```

## State Management

### Component State
- `adapters`: StackAdapter[] - Current stack composition
- `stackName`: string - Stack name input
- `stackDescription`: string - Stack description
- `validationReport`: ValidationReport | null
- `testResult`: InferenceTestResult | null
- `expandedSections`: Record<string, boolean> - Collapsible state
- `dismissedIssues`: Set<string> - User-dismissed issues
- `selectedAdapter`: string - Selected adapter for addition
- `isLoading`: boolean - API loading state
- `isSaving`: boolean - Save operation state
- `isTestingInference`: boolean - Test inference state

## Performance Considerations

1. **Memoization:** Validation report is memoized using useMemo
2. **Debouncing:** Validation runs on adapters/stackName change only
3. **Virtualization:** Not needed (typical <10 adapters)
4. **Lazy Loading:** API calls use useState + useEffect pattern

## Accessibility

- Semantic HTML (button, input, form elements)
- ARIA labels on icons and disabled states
- Keyboard navigation for drag-and-drop (dnd-kit provides)
- Color contrast meets WCAG AA standards
- Focus visible states on all interactive elements

## Error Handling

- API errors caught and displayed in alerts
- Network failures show user-friendly messages
- Validation errors don't block UI, shown as cards
- Dismissed issues can't be re-dismissed (stored in Set)

## Testing Considerations

### Unit Tests (useStackValidation)
- Each validation rule function independently
- Edge cases (empty stacks, single adapters)
- Score calculation logic

### Integration Tests
- AdapterStackComposer full workflow
- Drag-and-drop reordering
- Add/remove adapters
- Save/update operations
- Validation report generation

### E2E Tests
- Create stack flow
- Edit existing stack
- Preview and test inference
- Error scenarios

## Future Enhancements

1. **Batch Operations:** Apply same settings to multiple adapters
2. **Stack Templates:** Pre-built stacks for common patterns
3. **A/B Testing:** Create stack variants to compare
4. **Version Control:** Stack version history and rollback
5. **Advanced Scheduling:** Time-based stack switching
6. **Performance Profiling:** Real-time metrics during inference
7. **Cost Analysis:** Estimated compute cost per stack
8. **Dependency Graphs:** Visualize adapter dependencies
