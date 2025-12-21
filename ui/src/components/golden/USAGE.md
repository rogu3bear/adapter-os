# PolicyCheckDisplay Component Usage Guide

## Overview

The `PolicyCheckDisplay` component visualizes policy validation results during the promotion workflow. It shows the status of AdapterOS's 23 canonical policies and allows admins to override non-critical policies with justification.

## Components

### Main Component: `PolicyCheckDisplay`

The primary component for displaying policy checks.

```tsx
import { PolicyCheckDisplay, usePolicyChecks } from './golden';

function PromotionWorkflow({ cpid, user }) {
  const { policies, loading, error, overridePolicy } = usePolicyChecks({ cpid });

  return (
    <PolicyCheckDisplay
      cpid={cpid}
      policies={policies}
      loading={loading}
      onOverride={overridePolicy}
      blockPromotion={policies.some(p => p.status === 'failed')}
      allowAdmin={user.role === 'admin'}
      userRole={user.role}
    />
  );
}
```

### Sub-Components

#### `PolicyCheckItem`
Displays individual policy check with status badge and brief details.

#### `PolicyDetails`
Expanded view showing detailed validation info, remediation steps, and documentation links.

#### `PolicyOverride`
Admin-only form for overriding policies with justification (required for critical policies).

## Props

### PolicyCheckDisplay

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `cpid` | string | Yes | Plan/CPID identifier |
| `policies` | PolicyCheck[] | Yes | Array of policy check results |
| `loading` | boolean | No | Show loading state |
| `onOverride` | function | No | Callback for policy override |
| `blockPromotion` | boolean | No | Force block promotion |
| `allowAdmin` | boolean | No | Show admin overrides |
| `userRole` | string | No | Current user role |

### PolicyCheck Data Model

```typescript
interface PolicyCheck {
  // Identity
  id: string;                    // e.g., "egress", "determinism"
  name: string;                  // Display name
  description: string;           // Brief description

  // Validation result
  status: 'passed' | 'failed' | 'warning' | 'pending';
  message?: string;              // Validation result message

  // Categorization
  category: 'security' | 'quality' | 'compliance' | 'performance';
  severity: 'critical' | 'high' | 'medium' | 'low';

  // Remediation
  remediation?: string;          // How to fix
  documentationUrl?: string;     // Link to docs

  // Detailed validation info
  details?: {
    expectedValue?: string | number;
    actualValue?: string | number;
    threshold?: string | number;
    componentAffected?: string[];
  };

  // Override capability
  canOverride?: boolean;
  overrideReason?: string;
}
```

## Usage Patterns

### Pattern 1: Simple Integration with Hook

```tsx
import { PolicyCheckDisplay, usePolicyChecks } from './golden';

export function PromotionPage({ cpid }) {
  const { policies, loading, error, overridePolicy, refetch } = usePolicyChecks({ cpid });

  if (error) {
    return <ErrorBoundary error={error} onRetry={refetch} />;
  }

  return (
    <PolicyCheckDisplay
      cpid={cpid}
      policies={policies}
      loading={loading}
      onOverride={overridePolicy}
      blockPromotion={true}
    />
  );
}
```

### Pattern 2: Custom Policy Data

```tsx
import { PolicyCheckDisplay } from './golden';

export function CustomPolicyChecker({ cpid, customPolicies }) {
  return (
    <PolicyCheckDisplay
      cpid={cpid}
      policies={customPolicies}
      allowAdmin={true}
      userRole="admin"
      onOverride={async (id, reason) => {
        // Custom override logic
        await myCustomAPI.overridePolicy(cpid, id, reason);
      }}
    />
  );
}
```

### Pattern 3: Blocking Promotion Flow

```tsx
export function PromotionWorkflow() {
  const { policies } = usePolicyChecks({ cpid });

  const hasCriticalFailures = policies.some(
    p => p.status === 'failed' && p.severity === 'critical'
  );

  return (
    <>
      <PolicyCheckDisplay
        cpid={cpid}
        policies={policies}
        blockPromotion={hasCriticalFailures}
      />

      <Button
        onClick={promoteNow}
        disabled={hasCriticalFailures}
      >
        Promote Plan
      </Button>
    </>
  );
}
```

## 23 Canonical Policies

The component supports all 23 policies from AdapterOS:

### Security (7 policies)
- Egress Control (zero network egress in production)
- Input Validation
- Tenant Isolation
- Memory Safety (no unsafe blocks in app crates)
- Artifact Signature (ED25519)
- Secrets Rotation
- Rate Limiting

### Quality (8 policies)
- Determinism (HKDF-seeded RNG)
- Router Policy (K-sparse, Q15 gates)
- Typed Error Handling
- Kernel Hash Match
- Lifecycle State validation
- Adapter Quality (activation %, quality delta)
- Latency P95
- Throughput

### Compliance (6 policies)
- Evidence Tracking (min relevance/confidence)
- Telemetry (canonical JSON)
- Semantic Naming ({tenant}/{domain}/{purpose}/{revision})
- Audit Logging
- Data Retention
- ITAR Compliance

### Performance (2 policies)
- Memory Headroom (>= 15%)
- Control Matrix

## Features

### Status Visualization
- **Passed**: Green checkmark, success badge
- **Failed**: Red X, error badge (blocks promotion)
- **Warning**: Yellow triangle, warning badge
- **Pending**: Info badge (validation in progress)

### Filtering
- Filter by status: All, Failed, Warnings
- Search by policy name/description
- Group by category (security, quality, compliance, performance)

### Admin Overrides
- Override non-critical policies with justification
- Critical policies require detailed reason (20+ chars)
- Audit logging of all overrides
- Shows risk assessment requirements

### Export
- Export policy report as JSON
- Includes summary, detailed results, remediation steps
- Timestamped for audit trail

## Integration Points

### With PromotionWorkflow
```tsx
<Card>
  <CardHeader>
    <CardTitle>Promotion Workflow</CardTitle>
  </CardHeader>
  <CardContent className="space-y-6">
    <PolicyCheckDisplay cpid={cpid} policies={policies} />
    <PromotionActions cpid={cpid} disabled={hasFailures} />
  </CardContent>
</Card>
```

### With Backend API

The component expects:
- `GET /v1/policies/{cpid}` - Fetch policy checks
- `POST /v1/policies/{cpid}/override` - Override policy
- `GET /v1/policies/{cpid}/history` - Get override history

Update `usePolicyChecks` hook to call your actual API endpoints.

## Styling

Uses existing AdapterOS UI components:
- `Badge` for status indicators
- `Accordion` for expandable details
- `Alert` for critical failures
- `Card` for sections
- `Button` for actions

Respects dark mode and theme settings from `globals.css`.

## Error Handling

- Network errors show in alert with retry button
- Individual policy failures show remediation steps
- Override failures provide detailed error messages
- Graceful degradation if API unavailable

## Performance

- Memoized policy filtering (search, status)
- Lazy rendering of expanded sections
- Efficient category grouping
- Optimized re-renders on policy updates

## Accessibility

- Proper ARIA labels and roles
- Keyboard navigation support
- Color not sole indicator (icons + text)
- High contrast badges
- Semantic HTML structure

## Example Backend Response Format

```json
{
  "cpid": "plan-12345",
  "policies": [
    {
      "id": "egress",
      "name": "Egress Control",
      "description": "Zero network egress in production",
      "status": "passed",
      "category": "security",
      "severity": "critical",
      "message": "UDS socket configured correctly"
    },
    {
      "id": "determinism",
      "name": "Determinism",
      "status": "failed",
      "category": "quality",
      "severity": "high",
      "message": "HKDF seeding not applied to all random sources",
      "remediation": "Replace rand::thread_rng() with HKDF-seeded RNG",
      "details": {
        "componentAffected": ["router", "dropout"],
        "expectedValue": "hkdf_seeded",
        "actualValue": "thread_rng"
      },
      "canOverride": false
    }
  ],
  "summary": {
    "total": 23,
    "passed": 21,
    "failed": 1,
    "warnings": 1,
    "passRate": 91,
    "canPromote": false
  }
}
```

## Testing

```tsx
import { render, screen } from '@testing-library/react';
import { PolicyCheckDisplay } from './PolicyCheckDisplay';

test('displays pass rate correctly', () => {
  const policies = [
    { id: '1', name: 'Policy 1', status: 'passed', ... },
    { id: '2', name: 'Policy 2', status: 'failed', ... },
  ];

  render(<PolicyCheckDisplay cpid="test" policies={policies} />);

  expect(screen.getByText('50%')).toBeInTheDocument();
});
```

## Troubleshooting

**Q: Policies not loading?**
- Check that `usePolicyChecks` hook is calling correct API endpoint
- Verify API response matches `PolicyCheckResponse` interface

**Q: Override button not showing?**
- Set `allowAdmin={true}` and `userRole="admin"`
- Ensure `canOverride` is `true` on policy object

**Q: Search not working?**
- Verify policy names/descriptions are populated
- Check that search query is lowercase

For more details, see [docs/ARCHITECTURE.md#architecture-components](../../docs/ARCHITECTURE.md#architecture-components)
