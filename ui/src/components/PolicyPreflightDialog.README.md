# PolicyPreflightDialog Component

## Overview

Modal dialog component for displaying policy check results before executing adapter operations. Enforces AdapterOS's 23 canonical policies with admin override support for non-critical violations.

**Citation:** [2025-11-25†ui†policy-preflight-dialog]

## Features

- **Policy Check Display:** Shows all policy validation results with severity-based styling
- **Summary Statistics:** Total checks, passed/failed counts, error/warning breakdown
- **Admin Override:** Allows admins to override non-critical policy failures
- **Severity Levels:** Error (blocking), Warning (overridable), Info (informational)
- **Responsive Layout:** Clean, accessible UI with collapsible sections
- **Loading States:** Proper loading indicators during async operations

## Usage

### Basic Example

```tsx
import { PolicyPreflightDialog, PolicyCheck } from './components/PolicyPreflightDialog';

function AdapterLoader() {
  const [showDialog, setShowDialog] = useState(false);

  const checks: PolicyCheck[] = [
    {
      policy_id: 'egress-001',
      policy_name: 'Zero Network Egress',
      passed: true,
      severity: 'error',
      message: 'No network egress detected',
      can_override: false,
    },
    // ... more checks
  ];

  const canProceed = !checks.some(
    c => !c.passed && c.severity === 'error' && !c.can_override
  );

  return (
    <PolicyPreflightDialog
      open={showDialog}
      onOpenChange={setShowDialog}
      title="Load Adapter - Policy Validation"
      description="23 canonical policies will be enforced"
      checks={checks}
      canProceed={canProceed}
      onProceed={handleLoad}
      onCancel={() => setShowDialog(false)}
      isAdmin={userRole === 'admin'}
      isLoading={isLoading}
    />
  );
}
```

### Integration Points

1. **Adapter Loading** (`/adapters/:id`)
   - Validate policies before `POST /v1/adapters/:id/load`
   - Enforce Egress, Determinism, Tenant Isolation policies

2. **Stack Activation** (`/stacks`)
   - Validate policies before `POST /v1/adapter-stacks/:id/activate`
   - Enforce Router, Evidence, Telemetry policies

3. **Training Start** (`/training`)
   - Validate policies before `POST /v1/training/start`
   - Enforce Determinism, Naming, Input Validation policies

## PolicyCheck Interface

```typescript
interface PolicyCheck {
  policy_id: string;        // Unique policy identifier (e.g., "egress-001")
  policy_name: string;      // Human-readable name
  passed: boolean;          // Validation result
  severity: 'error' | 'warning' | 'info';
  message: string;          // Result message
  can_override?: boolean;   // Admin can override (default: false)
  details?: string;         // Additional context (shown in monospace)
}
```

## Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `open` | `boolean` | Yes | Dialog visibility state |
| `onOpenChange` | `(open: boolean) => void` | Yes | Dialog state change handler |
| `title` | `string` | Yes | Dialog title |
| `description` | `string` | No | Optional description text |
| `checks` | `PolicyCheck[]` | Yes | Policy validation results |
| `canProceed` | `boolean` | Yes | Whether operation can proceed |
| `onProceed` | `() => void` | Yes | Proceed button handler |
| `onCancel` | `() => void` | Yes | Cancel button handler |
| `isAdmin` | `boolean` | No | Enable admin override (default: false) |
| `isLoading` | `boolean` | No | Loading state (default: false) |

## 23 Canonical Policies

The component enforces AdapterOS's policy framework:

### Core Policies (Critical)
- **Egress:** Zero network egress in production (`can_override: false`)
- **Determinism:** Reproducible execution via HKDF seeding (`can_override: false`)
- **Tenant Isolation:** Strict tenant boundaries (`can_override: false`)

### Secondary Policies (Overridable)
- **Router:** K-sparse LoRA routing with Q15 quantization
- **Evidence:** Audit trail with quality thresholds
- **Telemetry:** Structured event logging
- **Naming:** Semantic adapter naming conventions
- **Input Validation:** Type and range validation

See [AGENTS.md](../../../../AGENTS.md) for complete policy documentation.

## API Integration

### Expected API Endpoints

**Adapter Policy Validation:**
```
GET /v1/adapters/:id/validate-policies
Response: {
  policies: Array<{
    id: string;
    name: string;
    status: 'passed' | 'failed';
    severity: 'error' | 'warning' | 'info';
    message: string;
    can_override: boolean;
    details?: string;
  }>
}
```

**Stack Policy Validation:**
```
GET /v1/adapter-stacks/:id/validate-policies
Response: { policies: [...] }
```

### Example Integration

```tsx
async function validateAndLoad(adapterId: string) {
  // Fetch policy checks
  const response = await fetch(`/api/v1/adapters/${adapterId}/validate-policies`);
  const data = await response.json();

  // Transform to PolicyCheck format
  const checks: PolicyCheck[] = data.policies.map(p => ({
    policy_id: p.id,
    policy_name: p.name,
    passed: p.status === 'passed',
    severity: p.severity,
    message: p.message,
    can_override: p.can_override,
    details: p.details,
  }));

  // Show preflight dialog
  setPolicyChecks(checks);
  setShowPreflight(true);
}
```

## Behavior

### Blocking Errors
- Errors with `can_override: false` disable the Proceed button
- Alert banner displays: "Cannot Proceed - Critical policy violations"
- Admin status has no effect on non-overridable errors

### Admin Override
- Admin users see "Override" buttons on overridable failures
- Override toggles change button text to "Undo Override"
- Active overrides shown in footer with warning icon
- Proceed button changes to "Proceed (Override)" when overrides active

### Loading State
- Proceed button shows "Loading..." spinner
- Both buttons disabled during loading
- Loading state resets on dialog close

## Styling

- **Errors:** Red border/background (`border-red-200 bg-red-50`)
- **Warnings:** Yellow border/background (`border-yellow-200 bg-yellow-50`)
- **Info:** Blue border/background (`border-blue-200 bg-blue-50`)
- **Passed:** Green border/background (collapsed by default)

## Accessibility

- Proper ARIA roles and labels
- Keyboard navigation support
- Focus management for dialog
- Screen reader friendly severity indicators
- Color-blind safe severity icons

## Testing

See `__tests__/PolicyPreflightDialog.test.tsx` for comprehensive test suite covering:
- Rendering and display
- Statistics calculation
- Admin override behavior
- Button states and handlers
- Loading states

## Files

- `PolicyPreflightDialog.tsx` - Main component
- `PolicyPreflightDialog.example.tsx` - Usage examples
- `PolicyPreflightDialog.README.md` - This documentation
- `__tests__/PolicyPreflightDialog.test.tsx` - Test suite

## References

- [AGENTS.md](../../../../AGENTS.md) - Policy Packs section
- [docs/RBAC.md](../../../../docs/RBAC.md) - Admin permissions
- [docs/ARCHITECTURE.md#architecture-components](../../../../docs/ARCHITECTURE.md#architecture-components) - Policy enforcement patterns
- `ui/src/components/golden/PolicyCheckDisplay.tsx` - Related component for promotion workflows
