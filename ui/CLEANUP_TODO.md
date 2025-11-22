# UI Cleanup TODO

This document tracks dead code, unused exports, and items marked for future cleanup.

**Last Updated:** 2025-11-22

---

## Files Deleted

The following page files were confirmed unused (not imported anywhere, not in routes.ts) and have been deleted:

| File | Reason |
|------|--------|
| `ui/src/pages/HelpPage.tsx` | Not imported anywhere, not in routes configuration |
| `ui/src/pages/PlansPage.tsx` | Not imported anywhere, not in routes configuration |
| `ui/src/pages/InferencePlaygroundPage.tsx` | Not imported anywhere, not in routes configuration (InferencePage.tsx is the active inference page) |

---

## API Methods Marked as Deprecated

The following methods in `ui/src/api/client.ts` have been marked with `@deprecated` comments. They are currently unused and are candidates for removal in a future cleanup:

### Domain Adapter API (lines 1291-1377)

**Active (in use):**
- `listDomainAdapters()` - Used by `DomainAdapterManager.tsx`
- `testDomainAdapter()` - Used by `DomainAdapterManager.tsx`

**Deprecated (unused):**

| Method | Line | Status |
|--------|------|--------|
| `getDomainAdapter(adapterId)` | ~1302 | Unused - candidate for removal |
| `createDomainAdapter(data)` | ~1310 | Unused - candidate for removal |
| `loadDomainAdapter(adapterId, config?)` | ~1321 | Unused - candidate for removal |
| `unloadDomainAdapter(adapterId)` | ~1332 | Unused - candidate for removal |
| `getDomainAdapterManifest(adapterId)` | ~1354 | Unused - candidate for removal |
| `executeDomainAdapter(adapterId, inputData)` | ~1362 | Unused - candidate for removal |
| `deleteDomainAdapter(adapterId)` | ~1373 | Unused - candidate for removal |

**Note:** These methods correspond to backend endpoints that exist (`/v1/domain-adapters/*`). The frontend simply hasn't implemented UI for these operations yet. Consider removing the deprecated markers if UI is added, or removing the methods entirely if the feature is deprecated backend-side.

---

## Schemas Review

The schemas in `ui/src/schemas/` were reviewed. Most exports are used directly or referenced in documentation files:

### Actively Used Schemas

| Schema | Used By |
|--------|---------|
| `DatasetConfigSchema` | `TrainingWizard.tsx`, `DatasetBuilder.tsx` |
| `TrainingConfigSchema` | `TrainingWizard.tsx`, documentation |
| `InferenceRequestSchema` | `InferencePlayground.tsx` |
| `BatchPromptSchema` | `InferencePlayground.tsx` |
| `LoginFormSchema` | `LoginForm.tsx` |
| `formatValidationError` | Multiple hooks and components |
| `validateField` | `useFormValidation.ts` |
| Various adapter schemas | `AdapterRegisterPage.tsx` |

### Documentation-Only References

Several schema exports are referenced only in documentation files (`QUICK_REFERENCE.md`, `EXAMPLES.md`, `VALIDATION_GUIDE.md`, `README.md`). These are intentionally exported for developer reference and should not be removed:

- `AdapterNameSchema`, `AdapterNameUtils`
- `StartTrainingRequestSchema`
- `StreamingInferenceRequestSchema`
- `InferencePresets`
- `TrainingTemplates`
- `CreateAdapterStackRequestSchema`
- Various common schemas (`TenantIdSchema`, `RepositoryIdSchema`, etc.)

**Recommendation:** No schema changes needed. The exports serve documentation and future extensibility purposes.

---

## Cleanup Recommendations

### Immediate (Safe to do now)
- [x] Delete unused page files (completed)
- [x] Add deprecation comments to unused API methods (completed)

### Future Consideration
- [ ] Remove deprecated domain adapter methods if the feature is confirmed deprecated
- [ ] Review if `DomainAdapterManager.tsx` component itself is actively used or can be simplified
- [ ] Consider consolidating adapter-related functionality under the main adapters system

### Not Recommended
- Do NOT remove schema exports - they provide type safety and documentation value
- Do NOT remove API methods without confirming backend changes

---

## How to Verify Usage

```bash
# Check if a page is imported anywhere
grep -r "PageName" ui/src/ --include="*.ts" --include="*.tsx"

# Check if an API method is called
grep -r "methodName" ui/src/ --include="*.ts" --include="*.tsx"

# Check if a schema is imported
grep -r "SchemaName" ui/src/ --include="*.ts" --include="*.tsx"
```

---

## Maintenance

When adding new pages or API methods:
1. Always add to `routes.ts` if it's a navigable page
2. Always add imports in relevant components
3. Update this document if deprecated items are removed or reactivated
