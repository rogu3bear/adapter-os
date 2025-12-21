# RBAC Documentation Index

Complete navigation guide for role-based access control implementation in AdapterOS UI.

## Quick Navigation

**I want to...**

| Goal | Document | Time |
|------|----------|------|
| Get started quickly | [RBAC_QUICK_START.md](#quick-start) | 5 min |
| Learn the implementation | [RBAC_IMPLEMENTATION_GUIDE.md](#full-guide) | 30 min |
| See what was done | [RBAC_IMPLEMENTATION_SUMMARY.md](#what-was-done) | 15 min |
| Verify everything | [RBAC_CHECKLIST.md](#checklist) | 10 min |
| Understand changes | [CHANGES.md](#changelog) | 20 min |
| Use code examples | [RBAC_IMPLEMENTATION_GUIDE.md#common-patterns](#examples) | 15 min |

## Document Descriptions

### Quick Start
**File:** `RBAC_QUICK_START.md`
**Read Time:** 5 minutes
**Level:** Beginner
**Contains:**
- TL;DR summary
- 30-second setup
- Common tasks with code
- Quick reference

**When to read:** You want to get started immediately

### Full Guide
**File:** `RBAC_IMPLEMENTATION_GUIDE.md`
**Read Time:** 30 minutes
**Level:** Intermediate
**Contains:**
- RBAC overview
- All 6 roles explained
- All 15 protected routes
- Using RBAC in code (all patterns)
- Architecture & data flow
- Best practices
- Testing strategies
- Troubleshooting
- Common patterns
- Adding new protected routes

**When to read:** You need to understand the full system

### Implementation Summary
**File:** `RBAC_IMPLEMENTATION_SUMMARY.md`
**Read Time:** 15 minutes
**Level:** Intermediate
**Contains:**
- Completion status
- What was implemented
- Security architecture
- Key features
- File changes summary
- Usage examples
- Integration points
- Testing procedures
- Compliance checklist
- Next steps

**When to read:** You want to understand the project status

### Checklist
**File:** `RBAC_CHECKLIST.md`
**Read Time:** 10 minutes
**Level:** Beginner
**Contains:**
- Implementation status
- Changes made
- Security validation
- Testing checklist
- File locations
- Roles & permissions summary
- Integration points
- Deployment readiness

**When to read:** You need to verify everything is complete

### Changelog
**File:** `CHANGES.md`
**Read Time:** 20 minutes
**Level:** Intermediate
**Contains:**
- Summary of changes
- All modified files (before/after)
- All created files
- Integration status
- Backward compatibility
- Testing status
- Security features
- Performance impact

**When to read:** You need to understand what changed

## File Locations

All files are in `/Users/star/Dev/aos/ui/`:

### Documentation Files
```
ui/
├── RBAC_INDEX.md                    ← You are here
├── RBAC_QUICK_START.md              ← Start here (5 min)
├── RBAC_IMPLEMENTATION_GUIDE.md      ← Full guide (30 min)
├── RBAC_IMPLEMENTATION_SUMMARY.md    ← What was done (15 min)
├── RBAC_CHECKLIST.md                ← Verification (10 min)
└── CHANGES.md                       ← Changelog (20 min)
```

### Source Files

**Modified Files:**
```
ui/src/
├── config/
│   └── routes.ts                    (UPDATED)
├── components/
│   ├── route-guard.tsx              (ENHANCED)
│   └── ui/
│       └── role-guard.tsx           (ENHANCED)
```

**New Files:**
```
ui/src/
├── utils/
│   └── rbac.ts                      (NEW)
└── hooks/
    └── useRBAC.ts                   (NEW)
```

**Verified Files (no changes):**
```
ui/src/
├── main.tsx
├── utils/
│   └── navigation.ts
└── layout/
    └── RootLayout.tsx
```

## Implementation Scope

### What's Protected (15 Routes)

**Training Operations** (4 routes)
- `/trainer` - Single-file trainer
- `/training` - Training jobs
- `/testing` - Testing operations
- `/golden` - Golden runs

**Promotion** (1 route)
- `/promotion` - Promotion controls (admin-only)

**Monitoring** (1 route)
- `/routing` - Routing inspector

**Operations** (3 routes)
- `/inference` - Inference execution
- `/telemetry` - Telemetry viewing
- `/replay` - Replay sessions

**Compliance** (2 routes)
- `/policies` - Policy management
- `/audit` - Audit logging

**Administration** (3 routes)
- `/admin` - Admin panel
- `/reports` - System reports
- `/tenants` - Tenant management

### What's Supported (6 Roles)

1. **admin** - Full system access
2. **operator** - Training, inference, adapters
3. **sre** - Infrastructure, debugging
4. **compliance** - Audit, policy
5. **auditor** - Read-only audit
6. **viewer** - Read-only viewing

### What's Included

**Route Protection:**
- Automatic via RouteGuard
- User-friendly access denied page
- Shows required roles

**Navigation Filtering:**
- Hidden from unauthorized users
- Dynamic based on role
- Works with all 6 roles

**Component Protection:**
- RoleGuard component
- useRBAC hooks
- useCanAccess hook
- useAccessDenialReason hook

**Permission System:**
- 20+ granular permissions
- Role-to-permission mapping
- Permission helper functions

## Code Examples

### Protect a Route
```typescript
{
  path: '/sensitive-page',
  component: Page,
  requiredRoles: ['admin', 'operator'],
}
```

### Conditional Component
```typescript
<RoleGuard allowedRoles={['admin']}>
  <AdminContent />
</RoleGuard>
```

### Permission Check
```typescript
const { can } = useRBAC();
if (can(PERMISSIONS.ADAPTER_REGISTER)) { /* ... */ }
```

## Learning Path

**For New Developers:**
1. Read RBAC_QUICK_START.md (5 min)
2. Look at code examples in RBAC_IMPLEMENTATION_GUIDE.md
3. Copy patterns from existing routes
4. Done!

**For Code Reviewers:**
1. Read RBAC_IMPLEMENTATION_SUMMARY.md (15 min)
2. Review CHANGES.md (20 min)
3. Check RBAC_CHECKLIST.md (10 min)
4. Done!

**For Architects:**
1. Read RBAC_IMPLEMENTATION_SUMMARY.md (15 min)
2. Review architecture in RBAC_IMPLEMENTATION_GUIDE.md
3. Check security features in RBAC_CHECKLIST.md
4. Done!

**For QA/Testing:**
1. Read RBAC_CHECKLIST.md (10 min)
2. Review testing strategies in RBAC_IMPLEMENTATION_GUIDE.md
3. Test all 15 routes with all 6 roles
4. Done!

## Key Concepts

### How It Works

```
User logs in with role (e.g., 'operator')
    ↓
Route configuration checked
    ↓
requiredRoles includes 'operator'?
    ↓
Yes: Route rendered
No: Access denied page shown
```

### Three-Layer Defense

1. **Route Level:** RouteGuard checks requiredRoles
2. **Navigation Level:** generateNavigationGroups filters by role
3. **Component Level:** RoleGuard, useRBAC hooks

### Multiple Role Support

Routes can require multiple roles (OR logic):
```typescript
requiredRoles: ['admin', 'operator', 'sre']
// User needs at least ONE of these roles
```

### Case-Insensitive Matching

All comparisons normalize to lowercase:
```
'admin' = 'Admin' = 'ADMIN'  ← All equivalent
```

## Frequently Asked Questions

**Q: Do I need to change main.tsx?**
A: No. Routes are automatically protected through RouteGuard.

**Q: How do I add a new protected route?**
A: Add `requiredRoles` to the route config. That's it!

**Q: Can I check multiple permissions?**
A: Yes, use `canAny()` for OR logic or `canAll()` for AND logic.

**Q: Why isn't my route showing in navigation?**
A: Add `requiredRoles` to the route configuration.

**Q: What if user's role changes?**
A: Navigation and permissions update automatically on next render.

**Q: Is this backward compatible?**
A: Yes. 100% backward compatible. No breaking changes.

**Q: How many roles can a route require?**
A: As many as needed. Any number of roles in the array.

**Q: What if I want to protect just a component?**
A: Use RoleGuard. Route doesn't need protection.

**Q: How do I log access denials?**
A: RouteGuard automatically logs. Check browser console.

**Q: Can I customize the access denied page?**
A: Yes. Modify the JSX in route-guard.tsx.

## Performance Notes

- **Minimal overhead:** O(n) where n = number of required roles
- **Typical:** O(1) to O(3) for most routes (1-3 roles)
- **No extra API calls**
- **No external dependencies**
- **Local computation only**

## Security Notes

- Routes are protected by default
- Navigation hides unauthorized routes
- Case-insensitive to prevent bypasses
- Comprehensive logging for audit
- Type-safe implementation

## Next Steps

1. **Start Using:**
   - Add `requiredRoles` to your new sensitive routes
   - Use RoleGuard in components
   - Use useRBAC for permissions

2. **Learn More:**
   - Read RBAC_IMPLEMENTATION_GUIDE.md for full details
   - Check code comments for usage examples
   - Review existing protected routes

3. **Test:**
   - Test with each role
   - Verify navigation filtering
   - Check access denied pages

4. **Deploy:**
   - No special deployment steps
   - Backward compatible
   - Ready to go live

## Support

**For Quick Answers:**
- RBAC_QUICK_START.md

**For Detailed Help:**
- RBAC_IMPLEMENTATION_GUIDE.md

**For Verification:**
- RBAC_CHECKLIST.md

**For Specific Examples:**
- See "Common Patterns" section in RBAC_IMPLEMENTATION_GUIDE.md

## Summary

**What was done:**
- Protected 15 sensitive routes
- Supported 6 RBAC roles
- Created flexible permission system
- Added comprehensive documentation

**What you can do:**
- Protect routes by adding `requiredRoles`
- Control component visibility with RoleGuard
- Check permissions with useRBAC hooks
- Get helpful error messages

**What's automatic:**
- Route protection via RouteGuard
- Navigation filtering
- Access denied pages
- Permission checking

**What's next:**
- Add `requiredRoles` to your routes
- Use RoleGuard in components
- Check permissions in code
- Test with different roles

---

**Status:** Production Ready
**Documentation Updated:** 2025-11-19
**All Files Location:** `/Users/star/Dev/aos/ui/`

Start with RBAC_QUICK_START.md for the fastest way to get going!
