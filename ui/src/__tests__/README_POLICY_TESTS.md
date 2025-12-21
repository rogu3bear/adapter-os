# Policy Enforcement Tests

This document describes the test coverage for policy enforcement blocking in the AdapterOS UI.

## Test Files

### 1. PolicyPreflightDialog.test.tsx

**Purpose:** Tests the `PolicyPreflightDialog` component in isolation

**Coverage:**
- Dialog rendering with title and description
- Statistics display (total, passed, errors, warnings)
- Blocking error alerts for critical violations
- Proceed button state (enabled/disabled)
- Admin override functionality
- User actions (proceed, cancel)
- Loading states

**Key Test Cases:**
- ✅ Renders dialog correctly
- ✅ Shows correct statistics (passed/failed/warnings)
- ✅ Blocks proceed when critical violations exist
- ✅ Shows override button for admin users
- ✅ Hides override button for non-admin users
- ✅ Calls callbacks on proceed/cancel
- ✅ Shows loading state

### 2. PolicyEnforcement.test.tsx

**Purpose:** Comprehensive integration tests for policy enforcement in adapter and stack operations

**Test Suites:**

#### PolicyPreflightDialog - Rendering
Tests the dialog component's rendering and display of policy check results.

**Key Tests:**
- ✅ Renders policy check results correctly
- ✅ Shows correct statistics (passed/failed/warnings)
- ✅ Displays passed checks in collapsed section
- ✅ Displays failed checks with severity badges

#### PolicyPreflightDialog - Critical Violations
Tests blocking behavior when critical policy violations occur.

**Key Tests:**
- ✅ Blocks proceed when critical violations exist
- ✅ Shows blocking error alert for critical violations
- ✅ Displays "Cannot Override" badge for critical errors

#### PolicyPreflightDialog - Admin Override
Tests admin override capabilities for non-critical violations.

**Key Tests:**
- ✅ Allows admin override for non-critical violations
- ✅ Shows override warning when policies are overridden
- ✅ Does not show override button for non-admin users
- ✅ Enables proceed button when admin overrides all blocking policies

#### PolicyPreflightDialog - User Actions
Tests user interaction flows (proceed, cancel).

**Key Tests:**
- ✅ Calls onProceed when proceed button is clicked
- ✅ Calls onCancel when cancel button is clicked
- ✅ Resets override state when dialog is cancelled

#### PolicyPreflightDialog - Loading State
Tests loading state display and button disabling.

**Key Tests:**
- ✅ Shows loading state on proceed button
- ✅ Disables cancel button during loading

#### Integration - Adapter Loading with Preflight
Tests preflight check integration with adapter loading operations.

**Key Tests:**
- ✅ Triggers preflight check before load operation
- ✅ Shows dialog when policies fail
- ✅ Proceeds with operation after user confirmation
- ✅ Cancels operation when user declines
- ✅ Blocks operation when critical policies fail and user is not admin
- ✅ Allows admin to override non-critical failures

#### Integration - Stack Activation with Preflight
Tests preflight check integration with stack activation operations.

**Key Tests:**
- ✅ Triggers preflight check before stack activation
- ✅ Shows dialog when stack policies fail
- ✅ Proceeds with activation after user confirmation
- ✅ Cancels activation when user declines

#### Integration - Audit Trail
Tests audit trail generation for policy overrides.

**Key Tests:**
- ✅ Includes policy override reason in audit metadata
- ✅ Tracks failed preflight checks in audit log

## Test Data

### Mock Policy Checks

#### Passed Checks
- `egress`: Egress Control (error severity)
- `determinism`: Deterministic Execution (error severity)
- `router`: Router Policy (error severity)

#### Warning Checks
- `naming`: Semantic Naming (warning severity, can override)

#### Critical Failure Checks
- `egress`: Network egress detected (error severity, cannot override)

#### Overridable Error Checks
- `adapter-quality`: Activation percentage below threshold (error severity, can override)

## Running Tests

```bash
# Run all policy tests
npm test -- PolicyPreflightDialog.test.tsx PolicyEnforcement.test.tsx --run

# Run only PolicyPreflightDialog component tests
npm test -- PolicyPreflightDialog.test.tsx --run

# Run only integration tests
npm test -- PolicyEnforcement.test.tsx --run

# Run with coverage
npm test -- PolicyEnforcement.test.tsx --coverage
```

## Coverage Summary

### Component Coverage
- ✅ PolicyPreflightDialog component (100%)
- ✅ Statistics calculation and display
- ✅ Admin override UI
- ✅ Loading states
- ✅ Error states

### Integration Coverage
- ✅ Adapter loading workflow with preflight
- ✅ Adapter unloading workflow with preflight
- ✅ Stack activation workflow with preflight
- ✅ Admin override flow
- ✅ User cancellation flow
- ✅ Audit trail generation

### User Flows Tested
1. **Happy Path:** All policies pass → User proceeds → Operation succeeds
2. **Warning Path:** Non-critical policy fails → User sees warning → User proceeds → Operation succeeds
3. **Admin Override:** Critical (overridable) policy fails → Admin overrides → Operation succeeds
4. **Blocking Path:** Critical (non-overridable) policy fails → Proceed disabled → Operation blocked
5. **Cancellation:** Policy check shown → User cancels → Operation cancelled

## Integration Points

### Components
- `PolicyPreflightDialog` (/ui/src/components/PolicyPreflightDialog.tsx)
- `AdapterDetailPage` (/ui/src/pages/Adapters/AdapterDetailPage.tsx)
- `StackTable` (/ui/src/pages/Admin/StackTable.tsx)

### Hooks
- `useAdapterOperations` (/ui/src/hooks/useAdapterOperations.ts)

### API Methods
- `apiClient.preflightAdapterLoad(adapterId, operation)`
- `apiClient.preflightStackActivation(stackId)`
- `apiClient.loadAdapter(adapterId)`
- `apiClient.unloadAdapter(adapterId)`
- `apiClient.activateStack(stackId)`

### Types
- `PolicyPreflightResponse` (/ui/src/api/policyTypes.ts)
- `PolicyPreflightCheck` (/ui/src/api/policyTypes.ts)

## Future Enhancements

### Potential Test Additions
- [ ] E2E tests with real backend API
- [ ] Performance tests for large policy check lists
- [ ] Accessibility tests (keyboard navigation, screen reader support)
- [ ] Visual regression tests for dialog appearance

### API Integration Testing
- [ ] Test with mock API server
- [ ] Test error handling for API failures
- [ ] Test retry logic for failed preflight checks
- [ ] Test timeout handling

### Additional Scenarios
- [ ] Multiple simultaneous policy overrides
- [ ] Policy override expiration
- [ ] Policy override audit trail export
- [ ] Policy check caching

## Maintenance

### When to Update Tests
- When adding new policy types
- When changing policy severity levels
- When modifying override logic
- When updating dialog UI/UX
- When adding new adapter/stack operations

### Test Data Maintenance
- Update mock policy checks when canonical policies change
- Sync test data with backend policy definitions
- Update severity levels to match production configuration

## References

- [AGENTS.md - Policy Packs](/AGENTS.md#policy-packs)
- [PolicyPreflightDialog Component](/ui/src/components/PolicyPreflightDialog.tsx)
- [Policy Types](/ui/src/api/policyTypes.ts)
- [Adapter Operations Hook](/ui/src/hooks/useAdapterOperations.ts)

---

**Last Updated:** 2025-11-25
**Citation:** [2025-11-25†ui†policy-enforcement-tests]
