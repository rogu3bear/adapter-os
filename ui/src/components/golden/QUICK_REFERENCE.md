# PolicyCheckDisplay - Quick Reference Card

## Import

```typescript
import { PolicyCheckDisplay, usePolicyChecks } from './golden';
```

## Basic Usage (5 minutes)

```tsx
function MyComponent({ cpid, userRole }) {
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

## Components

| Component | Purpose | When to Use |
|-----------|---------|------------|
| `PolicyCheckDisplay` | Main policy display | Always (wraps everything) |
| `PolicyCheckItem` | Individual policy row | Automatically rendered |
| `PolicyDetails` | Expanded details | Automatically rendered |
| `PolicyOverride` | Admin override form | Automatically rendered |

## Hook: usePolicyChecks

```typescript
const {
  policies,      // PolicyCheck[]
  loading,       // boolean
  error,         // string | null
  refetch,       // () => Promise<void>
  overridePolicy // (id, reason) => Promise<void>
} = usePolicyChecks({ cpid, autoFetch: true });
```

## Policy Data Model

```typescript
interface PolicyCheck {
  id: string;                           // e.g., "egress"
  name: string;                         // Display name
  description: string;                  // Brief description
  status: 'passed'|'failed'|'warning'|'pending';
  category: 'security'|'quality'|'compliance'|'performance';
  severity: 'critical'|'high'|'medium'|'low';
  message?: string;                     // Validation result
  remediation?: string;                 // How to fix
  documentationUrl?: string;            // Link to docs
  details?: {                           // Detailed info
    expectedValue?: string | number;
    actualValue?: string | number;
    threshold?: string | number;
    componentAffected?: string[];
  };
  canOverride?: boolean;
  overrideReason?: string;
}
```

## Props Reference

### PolicyCheckDisplay

```typescript
interface PolicyCheckDisplayProps {
  cpid: string;                    // Required: Plan ID
  policies: PolicyCheck[];         // Required: Policy results
  loading?: boolean;               // Default: false
  onOverride?: (id, reason) => Promise<void>;  // Optional
  blockPromotion?: boolean;        // Default: false
  allowAdmin?: boolean;            // Default: false
  userRole?: string;               // Default: 'viewer'
}
```

## 23 Canonical Policies Quick Map

### Security (7)
- `egress` - Network isolation
- `input-validation` - Input safety
- `tenant-isolation` - Data isolation
- `memory-safety` - No unsafe code
- `artifact-signature` - ED25519 signatures
- `secrets-rotation` - Key rotation
- `rate-limiting` - Endpoint limits

### Quality (8)
- `determinism` - HKDF randomness
- `router` - K-sparse routing
- `typed-errors` - Result<T> errors
- `kernel-hash` - Hash consistency
- `lifecycle-state` - State validation
- `adapter-quality` - Quality threshold
- `latency-p95` - Latency target
- `throughput` - Throughput target

### Compliance (6)
- `evidence` - Evidence tracking
- `telemetry` - Canonical JSON
- `naming` - Semantic naming
- `audit-logging` - Audit trail
- `data-retention` - Retention policy
- `itar-compliance` - ITAR handling

### Performance (2)
- `memory-headroom` - Memory target
- `control-matrix` - Control coverage

## Common Use Cases

### Block Promotion on Failures
```tsx
<PolicyCheckDisplay
  cpid={cpid}
  policies={policies}
  blockPromotion={true}
/>

<Button
  disabled={policies.some(p => p.status === 'failed')}
  onClick={promote}
>
  Promote
</Button>
```

### Admin Override Workflow
```tsx
<PolicyCheckDisplay
  cpid={cpid}
  policies={policies}
  allowAdmin={user.role === 'admin'}
  userRole={user.role}
  onOverride={async (id, reason) => {
    await api.overridePolicy(cpid, id, reason);
  }}
/>
```

### Custom Data Source
```tsx
const [policies, setPolicies] = useState<PolicyCheck[]>([]);

useEffect(() => {
  myCustomAPI.getPolicies(cpid).then(setPolicies);
}, [cpid]);

return <PolicyCheckDisplay cpid={cpid} policies={policies} />;
```

## Status Indicators

| Status | Color | Icon | Meaning |
|--------|-------|------|---------|
| `passed` | Green | ✓ | Policy validated |
| `failed` | Red | ✗ | Blocks promotion |
| `warning` | Yellow | ⚠ | Review needed |
| `pending` | Blue | ⏳ | In progress |

## Severity Badges

| Severity | Color | Impact |
|----------|-------|--------|
| `critical` | Red | Blocks promotion |
| `high` | Orange | Important to fix |
| `medium` | Yellow | Should review |
| `low` | Blue | Nice to fix |

## Category Colors

| Category | Color | Focus |
|----------|-------|-------|
| `security` | Red | Data/network safety |
| `quality` | Yellow | Reliability/perf |
| `compliance` | Blue | Audit/regulatory |
| `performance` | Green | Latency/throughput |

## Override Workflow (Admin Only)

1. Find failed policy
2. Click "Override Policy" button
3. Enter justification (20+ chars for critical)
4. Review risk warnings
5. Submit to create audit log

## API Endpoints

```
GET /v1/policies/{cpid}
  Response: PolicyCheckResponse

POST /v1/policies/{cpid}/override
  Body: { policyId, reason }
  Response: PolicyOverrideResponse
```

## Testing

```typescript
// Test component rendering
render(<PolicyCheckDisplay cpid="test" policies={policies} />);

// Test hook
const { result } = renderHook(() => usePolicyChecks({ cpid }));
expect(result.current.policies).toBeDefined();

// Test filter
expect(screen.getByText('Policy Name')).toBeInTheDocument();
```

## Styling Classes

Components use shadcn/ui + Tailwind CSS:
- Badges: `variant="success|error|warning|info|neutral"`
- Alerts: `variant="default|destructive"`
- Buttons: `variant="default|outline"`
- Responsive grid layout
- Dark mode support automatic

## Error Handling

```typescript
const { error, refetch } = usePolicyChecks({ cpid });

if (error) {
  return (
    <>
      <Alert variant="destructive">{error}</Alert>
      <Button onClick={refetch}>Retry</Button>
    </>
  );
}
```

## Performance Tips

1. Memoize policy arrays passed as props
2. Use usePolicyChecks hook for caching
3. Filter happens on client-side (use pagination for 100+ policies)
4. Search/filter are optimized with useMemo

## Keyboard Navigation

- `Tab`: Move between elements
- `Enter`: Expand accordion items
- `Escape`: Close override form
- `Space`: Toggle buttons

## Accessibility

- ✓ ARIA labels on all interactive elements
- ✓ Color + icons (not color alone)
- ✓ High contrast badges
- ✓ Keyboard navigable
- ✓ Screen reader friendly

## File Locations

```
/Users/star/Dev/aos/ui/src/
├── components/golden/
│   ├── PolicyCheckDisplay.tsx
│   ├── PolicyCheckItem.tsx
│   ├── PolicyDetails.tsx
│   ├── PolicyOverride.tsx
│   ├── PromotionWorkflowExample.tsx
│   └── index.ts
├── hooks/usePolicyChecks.ts
└── api/policyTypes.ts
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Policies not loading | Check `usePolicyChecks` API endpoint |
| Override button hidden | Ensure `allowAdmin=true` and `userRole="admin"` |
| Styling looks wrong | Verify Tailwind/shadcn/ui installed |
| Search not working | Check policy names are populated |
| Accordion not expanding | Verify proper Accordion component import |

## Links to Full Docs

- **Full docs**: `./README.md`
- **Usage guide**: `./USAGE.md`
- **Integration example**: `./PromotionWorkflowExample.tsx`
- **Types reference**: `/Users/star/Dev/aos/ui/src/api/policyTypes.ts`
- **Hook reference**: `/Users/star/Dev/aos/ui/src/hooks/usePolicyChecks.ts`

## Export Feature

```typescript
// Automatically available in PolicyCheckDisplay
// User clicks "Export Report" button
// Downloads: policy-check-{cpid}-{timestamp}.json
// Contains: summary + all policy details
```

## What's Next

1. Update API endpoint in `usePolicyChecks`
2. Integrate with your Promotion component
3. Add unit tests
4. Deploy to production
5. Monitor with telemetry

---

**Last Updated:** 2025-11-19
**Status:** Ready for Production
**Component Version:** 1.0.0
