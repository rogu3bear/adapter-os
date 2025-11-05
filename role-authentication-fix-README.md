# Role Authentication Fix - Patch Documentation

## Patch File
`role-authentication-fix-clean.patch`

## Problem Summary
Authentication was failing with "invalid role" errors due to role string case mismatch between backend and frontend.

**Error**: `Error: invalid role at ApiClient.request (http://localhost:3300/src/api/client.ts:111:27)`

## Root Cause
- Backend `Role` enum expects lowercase strings: `"admin"`, `"operator"`, `"compliance"`, `"viewer"`
- Frontend and several backend paths were using capitalized: `"Admin"`, `"Operator"`, etc.
- `Role::from_str()` failed when parsing capitalized strings, causing authentication to fail

## Files Changed

### Backend (Rust) - 3 files
1. `crates/adapteros-server-api/src/handlers.rs`
   - Fixed dev-bypass endpoint to return `"role": "admin"` instead of `"role": "Admin"`

2. `crates/adapteros-server-api/src/middleware.rs`
   - Fixed middleware dev-bypass to use `"admin"` instead of `"Admin"` in Claims

3. `crates/adapteros-db/src/lib.rs`
   - Fixed database seed data to use lowercase role strings: `"admin"`, `"operator"`, `"sre"`, `"viewer"`

### Frontend (TypeScript) - 9 files
1. `ui/src/api/types.ts`
   - Updated `UserRole` type to lowercase: `'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer'`

2. `ui/src/components/dashboard/BaseModelWidget.tsx`
   - Changed role check: `user?.role === 'admin'` (was `'Admin'`)

3. `ui/src/components/Tenants.tsx`
   - Changed role check: `user.role !== 'admin'` (was `'Admin'`)

4. `ui/src/providers/CoreProviders.tsx`
   - Removed role string normalization (no longer capitalizing role strings)
   - Direct assignment: `role: data.role as UserRole`

5. `ui/src/data/role-guidance.ts`
   - Updated all role keys to lowercase: `'admin'`, `'operator'`, `'compliance'`, `'viewer'`, `'sre'`, `'auditor'`

6. `ui/src/components/ContextualHelp.tsx`
   - Updated all role keys in `pageGuidanceMap` to lowercase

7. `ui/src/components/Dashboard.tsx`
   - Updated `dashboardLayouts` role keys to lowercase

8. `ui/src/components/WorkflowWizard.tsx`
   - Updated `workflowsByRole` role keys to lowercase

9. `ui/src/config/routes.ts`
   - Updated `requiredRoles` to lowercase: `['admin']` (was `['Admin']`)

10. `ui/FEATURE_OVERVIEW.md`
    - Updated documentation examples to use lowercase roles

## How to Apply

```bash
# From repository root
git apply role-authentication-fix-clean.patch

# Or with 3-way merge (if conflicts)
git apply --3way role-authentication-fix-clean.patch
```

## Verification

After applying, verify:

1. **Backend compiles**:
   ```bash
   cargo check --workspace
   ```

2. **Frontend compiles**:
   ```bash
   cd ui && npm run build
   ```

3. **Authentication works**:
   - Dev bypass should return `{"role": "admin"}` (lowercase)
   - Login should work without "invalid role" errors
   - Role-based access controls should function correctly

## Testing Checklist

- [ ] Dev bypass endpoint returns lowercase role
- [ ] Middleware dev bypass accepts lowercase role
- [ ] Database seed users have lowercase roles
- [ ] Frontend role checks use lowercase
- [ ] Admin users can access admin routes
- [ ] Non-admin users are properly restricted
- [ ] All role-based UI components render correctly

## Impact

**Before**: Authentication failures, role checks failing, users unable to access features

**After**: Consistent lowercase role strings throughout, authentication working, role-based authorization functioning

## Related Issues

This fix addresses the authentication error:
```
Error: invalid role
    at ApiClient.request (http://localhost:3300/src/api/client.ts:111:27)
    at async http://localhost:3300/src/components/dashboard/BaseModelWidget.tsx:90:32
```

## Notes

- All role strings are now consistently lowercase across backend and frontend
- The `Role` enum in Rust uses PascalCase (`Role::Admin`) but serializes to lowercase (`"admin"`)
- Display names (like "Administrator") remain capitalized for UI presentation
- Only the internal role strings used for authentication/authorization are lowercase

