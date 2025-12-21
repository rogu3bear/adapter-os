# RBAC Quick Start Guide

Fast reference for implementing role-based access control in AdapterOS UI.

## TL;DR

1. Add `requiredRoles` to route config
2. Use `RoleGuard` for components
3. Use `useRBAC()` hook for permissions
4. Everything else is automatic

## 30-Second Setup

### Protect a Route

```typescript
// In ui/src/config/routes.ts
{
  path: '/sensitive-page',
  component: MyPage,
  requiresAuth: true,
  requiredRoles: ['admin', 'operator'],  // ← Add this line
  navGroup: 'Operations',
  navTitle: 'Sensitive Page',
}
```

That's it! The route is now:
- Protected by RouteGuard
- Hidden from unauthorized users in navigation
- Shows "Access Denied" page if accessed without permission

### Conditional Component

```typescript
import { RoleGuard } from '@/components/ui/role-guard';

<RoleGuard allowedRoles={['admin', 'operator']}>
  <AdminControls />
</RoleGuard>
```

### Permission Checking

```typescript
import { useRBAC } from '@/hooks/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';

export function MyComponent() {
  const { can } = useRBAC();

  return (
    <>
      {can(PERMISSIONS.ADAPTER_REGISTER) && (
        <button>Register Adapter</button>
      )}
    </>
  );
}
```

## Common Tasks

### Show button only to admins

```typescript
<RoleGuard allowedRoles={['admin']}>
  <DeleteButton />
</RoleGuard>
```

### Show different UI by role

```typescript
const { userRole } = useRBAC();

{userRole === 'admin' && <AdminPanel />}
{userRole === 'operator' && <OperatorPanel />}
{userRole === 'viewer' && <ViewerPanel />}
```

### Check if user can do something

```typescript
const { can, canAny } = useRBAC();

if (can(PERMISSIONS.ADAPTER_DELETE)) {
  // Can delete
}

if (canAny([PERMISSIONS.ADAPTER_DELETE, PERMISSIONS.ADAPTER_UNLOAD])) {
  // Can do either action
}
```

### Get helpful error message

```typescript
import { useAccessDenialReason } from '@/hooks/useRBAC';

const reason = useAccessDenialReason(['admin']);
if (reason) {
  return <ErrorBox>{reason}</ErrorBox>;
}
```

## 6 Roles

| Role | Access Level | Use Case |
|------|--------------|----------|
| **admin** | Full | All operations, system admin |
| **operator** | High | Training, inference, adapters |
| **sre** | Medium | Debugging, infrastructure |
| **compliance** | Medium | Audit, policy review |
| **auditor** | Low | Audit logs only |
| **viewer** | Lowest | View adapters only |

## Protected Routes

Already protected (15 routes):

```
Training Operations:
  /trainer       ['admin', 'operator']
  /training      ['admin', 'operator']
  /testing       ['admin', 'operator']
  /golden        ['admin', 'operator']
  /promotion     ['admin'] ← Most restrictive

Monitoring:
  /routing       ['admin', 'operator', 'sre']

Operations:
  /inference     ['admin', 'operator']
  /telemetry     ['admin', 'operator', 'sre']
  /replay        ['admin', 'operator', 'sre']

Compliance:
  /policies      ['admin', 'operator']
  /audit         ['admin', 'sre', 'compliance']

Admin:
  /admin         ['admin']
  /reports       ['admin', 'operator']
  /tenants       ['admin']
```

## Files

**New Files:**
- `src/utils/rbac.ts` - Permission definitions
- `src/hooks/useRBAC.ts` - React hooks
- `RBAC_IMPLEMENTATION_GUIDE.md` - Full guide
- `RBAC_IMPLEMENTATION_SUMMARY.md` - What was done
- `RBAC_CHECKLIST.md` - Verification
- `CHANGES.md` - Change log

**Modified Files:**
- `src/config/routes.ts` - Added requiredRoles
- `src/components/route-guard.tsx` - Better error UI
- `src/components/ui/role-guard.tsx` - Case-insensitive matching

**No Changes Needed:**
- `main.tsx` - Already uses RouteGuard
- `RootLayout.tsx` - Already filters navigation
- Auth system - Already provides roles

## Imports Reference

```typescript
// Routes
import { routes, canAccessRoute } from '@/config/routes';

// Components
import { RouteGuard } from '@/components/route-guard';
import { RoleGuard } from '@/components/ui/role-guard';

// Hooks
import { useRBAC, useCanAccess, useAccessDenialReason } from '@/hooks/useRBAC';

// Utils
import { PERMISSIONS, ROLES, hasPermission, hasRole } from '@/utils/rbac';
```

## Examples

### Protect Multiple Routes

```typescript
// ui/src/config/routes.ts
const sensitiveRoutes = [
  {
    path: '/feature-1',
    component: Feature1,
    requiredRoles: ['admin', 'operator'],
    // ... other config
  },
  {
    path: '/feature-2',
    component: Feature2,
    requiredRoles: ['admin'],
    // ... other config
  },
];
```

### Conditional Feature Display

```typescript
export function FeatureList() {
  return (
    <div>
      <RoleGuard
        allowedRoles={['admin', 'operator']}
        fallback={<p>Requires admin or operator role</p>}
      >
        <TrainingControls />
      </RoleGuard>

      <RoleGuard allowedRoles={['admin']}>
        <AdminOnlyFeature />
      </RoleGuard>
    </div>
  );
}
```

### Complex Permission Logic

```typescript
export function AdapterActions() {
  const { can, canAny, userRole } = useRBAC();

  // Multi-condition check
  if (!can(PERMISSIONS.ADAPTER_VIEW)) {
    return <p>No permission to view adapters</p>;
  }

  return (
    <div>
      {can(PERMISSIONS.ADAPTER_REGISTER) && (
        <button>Register New Adapter</button>
      )}

      {canAny([
        PERMISSIONS.ADAPTER_DELETE,
        PERMISSIONS.ADAPTER_UNLOAD
      ]) && (
        <button>Remove Adapter</button>
      )}

      <p>Your role: {userRole}</p>
      <p>Your permissions: {/* List them */}</p>
    </div>
  );
}
```

## Security Notes

- Routes are protected by default when `requiredRoles` is set
- Navigation hides unauthorized routes
- Access denied page shows friendly error
- All comparisons are case-insensitive
- Logs access denials for audit trail

## Testing

### Test a Protected Route

```bash
# Login as operator
# Try to access /promotion
# Should see "Access Denied" page

# Login as admin
# Should see promotion controls
```

### Test Navigation Filtering

```bash
# Login as operator
# Should NOT see /promotion in sidebar
# Should NOT see /admin in sidebar
# SHOULD see /training, /inference, etc.

# Login as admin
# Should see all routes
```

## Troubleshooting

### Route appears in navigation but access denied

Add `requiredRoles` to the route:

```typescript
{
  path: '/your-route',
  component: YourComponent,
  requiredRoles: ['admin', 'operator'],  // Add this
}
```

### RoleGuard shows fallback unexpectedly

Check role names match (case-insensitive):

```typescript
// These work the same:
<RoleGuard allowedRoles={['admin']}>
<RoleGuard allowedRoles={['Admin']}>
<RoleGuard allowedRoles={['ADMIN']}>
```

### useRBAC hook returns no permissions

Verify user is logged in:

```typescript
const { isAuthenticated, getPermissions } = useRBAC();

if (!isAuthenticated()) {
  return <p>Please log in</p>;
}

const permissions = getPermissions();
```

## More Info

- **Full Guide:** RBAC_IMPLEMENTATION_GUIDE.md
- **Architecture:** RBAC_IMPLEMENTATION_SUMMARY.md
- **Checklist:** RBAC_CHECKLIST.md
- **Changes:** CHANGES.md

## Key Takeaways

1. **Routes:** Add `requiredRoles` array to protect
2. **Components:** Use `RoleGuard` for visibility
3. **Permissions:** Use `useRBAC()` for fine-grained control
4. **Automatic:** Navigation filtering and route protection work automatically
5. **Tested:** All 15 protected routes working, 6 roles supported
6. **Documented:** Full guides and examples provided

## That's It!

The RBAC system is ready to use. Start protecting sensitive routes and components today.
