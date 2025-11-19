# Golden Components

Policy validation, diff visualization, and promotion workflow components for AdapterOS.

## Overview

The golden components provide a comprehensive UI for validating and approving plan promotions through a 23-policy validation gate, plus text comparison tools for golden run verification. These components handle:

- Policy status visualization
- Failure detection and remediation guidance
- Admin policy overrides (with audit logging)
- Detailed validation reports
- Integration with promotion workflows
- Side-by-side text diff visualization
- Golden run output comparison
- Multi-mode diff viewing (side-by-side, unified, split)

## Components

### Policy Components

#### `PolicyCheckDisplay` (Main)

The primary component for displaying policy validation results.

**Features:**
- Visual pass/fail/warning status indicators
- Grouped by policy category (security, quality, compliance, performance)
- Searchable and filterable
- Detailed failure info with remediation steps
- Export policy report as JSON
- Admin override workflow (critical policies require justification)
- Blocks promotion if critical policies fail

**Props:**
```typescript
interface PolicyCheckDisplayProps {
  cpid: string;                    // Plan identifier
  policies: PolicyCheck[];         // Policy check results
  loading?: boolean;               // Show loading state
  onOverride?: (id, reason) => Promise<void>;  // Override handler
  blockPromotion?: boolean;        // Force block
  allowAdmin?: boolean;            // Show admin overrides
  userRole?: string;               // User role for permissions
}
```

#### `PolicyCheckItem`

Individual policy check row with status badge and severity indicator.

#### `PolicyDetails`

Expandable section showing:
- Detailed validation results
- Expected vs actual values
- Affected components
- Remediation steps
- Documentation links

#### `PolicyOverride`

Admin-only form for overriding policies:
- Requires justification (20+ chars for critical)
- Shows risk warnings for critical policies
- Audit trail notification
- Controlled submission

### Diff Visualization Components

#### `DiffVisualization` (Main)

Side-by-side text comparison component for golden run verification.

**Features:**
- Multiple view modes (side-by-side, unified, split)
- Character-level highlighting
- Virtualization for large diffs (10K+ lines)
- Navigation between changes
- Statistics panel (similarity score, line counts)
- Export (HTML, text, clipboard)
- Dark mode support
- Color-blind friendly colors
- Keyboard shortcuts

**Props:**
```typescript
interface DiffVisualizationProps {
  goldenText: string;                  // Golden baseline text
  currentText: string;                 // Current run text
  mode?: DiffViewMode;                 // View mode
  className?: string;                  // Additional CSS classes
  onModeChange?: (mode) => void;       // Mode change callback
  showLineNumbers?: boolean;           // Show line numbers
  contextLines?: number;               // Lines of context (-1 = all)
  enableVirtualization?: boolean;      // Enable virtualization
}
```

**Usage:**
```tsx
import { DiffVisualization } from './golden';

<DiffVisualization
  goldenText={goldenOutput}
  currentText={currentOutput}
  contextLines={3}
/>
```

#### `DiffVisualizationWithNav`

Enhanced version with keyboard navigation:
- N: Next change
- P: Previous change
- U: Toggle view mode
- Cmd/Ctrl+C: Copy to clipboard

#### `DiffVisualizationExample`

Demo component with example texts and controls. Useful for testing and documentation.

### Utilities

#### `diffUtils.ts`

Helper functions for diff operations:
- `calculateSimilarity(str1, str2)`: Calculate similarity score (0-100)
- `levenshteinDistance(str1, str2)`: Compute edit distance
- `createUnifiedDiff(text1, text2)`: Generate unified diff format
- `extractDiffRegions(text1, text2)`: Extract changed regions with context
- `formatDiffStats(stats)`: Format statistics as readable string
- `truncateDiff(text, maxLines)`: Truncate long diffs for preview
- `isDiffTooLarge(text1, text2)`: Check if diff exceeds size threshold
- `optimizedDiff(text1, text2)`: Optimize diff for large texts

#### `useDiffKeyboardNav.ts`

Custom hook for keyboard navigation in diff components.

## Hook: `usePolicyChecks`

Manages policy fetching and override submission.

```typescript
const { policies, loading, error, refetch, overridePolicy } = usePolicyChecks({
  cpid: 'plan-123',
  autoFetch: true,  // Fetch on mount
});
```

## Types

### Core Types (in `policyTypes.ts`)

```typescript
type PolicyStatus = 'passed' | 'failed' | 'warning' | 'pending';
type PolicyCategory = 'security' | 'quality' | 'compliance' | 'performance';
type PolicySeverity = 'critical' | 'high' | 'medium' | 'low';

interface PolicyCheck {
  id: string;
  name: string;
  description: string;
  status: PolicyStatus;
  category: PolicyCategory;
  severity: PolicySeverity;
  message?: string;
  remediation?: string;
  documentationUrl?: string;
  details?: PolicyCheckDetails;
  canOverride?: boolean;
  overrideReason?: string;
}
```

## 23 Canonical Policies

See [docs/ARCHITECTURE_INDEX.md](../../docs/ARCHITECTURE_INDEX.md) for detailed descriptions.

**Security (7)**
- Egress Control
- Input Validation
- Tenant Isolation
- Memory Safety
- Artifact Signature
- Secrets Rotation
- Rate Limiting

**Quality (8)**
- Determinism
- Router Policy
- Typed Error Handling
- Kernel Hash Match
- Lifecycle State
- Adapter Quality
- Latency P95
- Throughput

**Compliance (6)**
- Evidence Tracking
- Telemetry
- Semantic Naming
- Audit Logging
- Data Retention
- ITAR Compliance

**Performance (2)**
- Memory Headroom
- Control Matrix

## Usage Examples

### Basic Usage

```tsx
import { PolicyCheckDisplay, usePolicyChecks } from './golden';

function MyPromotionFlow({ cpid, userRole }) {
  const { policies, loading, error, overridePolicy } = usePolicyChecks({ cpid });

  return (
    <PolicyCheckDisplay
      cpid={cpid}
      policies={policies}
      loading={loading}
      onOverride={overridePolicy}
      blockPromotion={true}
      allowAdmin={userRole === 'admin'}
      userRole={userRole}
    />
  );
}
```

### With Promotion Workflow

```tsx
import { PromotionWorkflowExample } from './golden/PromotionWorkflowExample';

export function PromotionPage() {
  return (
    <PromotionWorkflowExample
      cpid={cpid}
      user={currentUser}
      selectedTenant={tenant}
      onPromote={promoteHandler}
    />
  );
}
```

### Custom Data

```tsx
function CustomPolicyCheck() {
  const [policies, setPolicies] = useState<PolicyCheck[]>([]);

  useEffect(() => {
    // Load from custom source
    fetchCustomPolicies().then(setPolicies);
  }, []);

  return <PolicyCheckDisplay cpid="test" policies={policies} />;
}
```

## Integration with Backend

Update the `usePolicyChecks` hook to call your API endpoints:

```typescript
// Fetch policies
const response = await apiClient.getPolicies(cpid);
// Expected: { policies: PolicyCheck[], summary: ... }

// Override policy
await apiClient.overridePolicy(cpid, policyId, { reason });
// Expected: { overriddenBy, overriddenAt, auditId }
```

## Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    PromotionWorkflow                          │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   usePolicyChecks Hook                        │
│  - Fetches policies from /v1/policies/{cpid}                 │
│  - Handles overrides via POST /v1/policies/{cpid}/override   │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              PolicyCheckDisplay Component                     │
│  - Displays policies grouped by category                      │
│  - Filters by status/search                                   │
│  - Shows failure details and remediation                      │
└───────────────────────────┬─────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
    ┌────────────┐  ┌──────────────┐  ┌──────────────┐
    │ PolicyItem │  │ PolicyDetails│  │PolicyOverride│
    │  (status)  │  │(remediation) │  │(admin form)  │
    └────────────┘  └──────────────┘  └──────────────┘
```

## Styling

- Uses shadcn/ui components (Badge, Accordion, Alert, Card)
- Respects theme tokens and dark mode
- Color-coded by severity and status
- Responsive grid layout
- Accessible keyboard navigation

## Files

```
golden/
├── PolicyCheckDisplay.tsx        # Main policy component
├── PolicyCheckItem.tsx           # Individual policy row
├── PolicyDetails.tsx             # Expanded details view
├── PolicyOverride.tsx            # Admin override form
├── PromotionWorkflowExample.tsx  # Integration example
├── DiffVisualization.tsx         # Main diff component
├── DiffVisualizationWithNav.tsx  # Diff with keyboard nav
├── DiffVisualizationExample.tsx  # Diff demo component
├── diffUtils.ts                  # Diff utilities
├── useDiffKeyboardNav.ts         # Keyboard nav hook
├── index.ts                      # Exports
├── README.md                     # This file
└── USAGE.md                      # Detailed usage guide

../../api/
├── policyTypes.ts                # Policy data models
└── types.ts                      # Extends with Policy types

../../hooks/
└── usePolicyChecks.ts            # Hook for policy management
```

## API Contract

### Fetch Policies

**Request:**
```
GET /v1/policies/{cpid}
```

**Response:**
```json
{
  "cpid": "plan-123",
  "policies": [
    {
      "id": "egress",
      "name": "Egress Control",
      "status": "passed",
      "category": "security",
      "severity": "critical"
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

### Override Policy

**Request:**
```
POST /v1/policies/{cpid}/override
Content-Type: application/json

{
  "policyId": "determinism",
  "reason": "Using deterministic_seeded RNG with validated HKDF"
}
```

**Response:**
```json
{
  "cpid": "plan-123",
  "policyId": "determinism",
  "overriddenBy": "user@example.com",
  "overriddenAt": "2025-01-19T10:30:00Z",
  "auditId": "audit-456"
}
```

## Testing

```bash
# Test component rendering
npm test golden/PolicyCheckDisplay.test.tsx

# Test hook
npm test hooks/usePolicyChecks.test.ts

# Integration test
npm test components/__integration__/PromotionFlow.test.tsx
```

## Performance

- Memoized filtering and grouping
- Lazy-rendered expandable sections
- Efficient category aggregation
- Minimal re-renders on policy updates

## Accessibility

- ARIA labels and roles
- Keyboard navigation
- Color + icons (not color alone)
- High contrast badges
- Semantic HTML

## Error Handling

- Network error recovery with retry
- Detailed error messages
- Graceful degradation if API unavailable
- Validation error feedback

## Future Enhancements

- [ ] Policy version history and rollback
- [ ] Batch override approval workflow
- [ ] Policy dependency visualization
- [ ] Historical compliance reports
- [ ] Custom policy definitions
- [ ] Policy impact analysis
- [ ] A/B testing framework for policy changes

## References

- CLAUDE.md: Policy pack definitions
- docs/ARCHITECTURE_PATTERNS.md: Promotion workflow patterns
- docs/RBAC.md: Role-based access control
- ui/src/components/Promotion.tsx: Existing promotion component

## Maintenance

- Update CANONICAL_POLICIES list when new policies added
- Keep policyTypes.ts in sync with backend API
- Update USAGE.md with new features/patterns
- Test with all policy states (passed/failed/warning/pending)

## Questions?

See USAGE.md for detailed integration guide and troubleshooting.
