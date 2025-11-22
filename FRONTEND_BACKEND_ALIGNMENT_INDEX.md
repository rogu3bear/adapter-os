# Frontend-Backend Alignment Documentation Index

**Analysis Date:** 2025-11-22
**Status:** Complete and Ready for Implementation

---

## Quick Links

| Document | Purpose | Audience | Read Time |
|----------|---------|----------|-----------|
| **FRONTEND_BACKEND_ALIGNMENT.md** | Comprehensive technical specification | Developers, Architects | 30-45 min |
| **FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md** | Quick reference decision table | Product Leads, Managers | 10-15 min |
| **ENDPOINT_VERIFICATION_CHECKLIST.md** | Implementation & testing checklist | QA, DevOps | 15-20 min |
| **FRONTEND_BACKEND_ALIGNMENT_INDEX.md** | This file - Navigation guide | Everyone | 5 min |

---

## What This Analysis Covers

### ✅ Completed Analysis
- Scanned all 88 endpoints called by frontend (`ui/src/api/client.ts`)
- Verified 60 endpoints already implemented in backend
- Identified 28 missing/unmapped endpoints
- Categorized by priority and effort
- Provided specific decisions for each endpoint
- Outlined 4-phase implementation plan

### 📋 Documents Provided
1. **Complete specification** with detailed sections
2. **Executive summary** with decision matrix
3. **Verification checklist** with testing strategy
4. **This index** for navigation

---

## Using These Documents

### For Project Managers / Product Leads
**Start with:** `FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md`
- Quick overview of gaps (28 endpoints)
- Priority matrix
- 4-phase timeline (3 weeks estimated)
- File modification checklist

### For Developers / Architects
**Start with:** `FRONTEND_BACKEND_ALIGNMENT.md`
- Sections 1-3 for detailed specifications
- Section 5 for implementation priority matrix
- Appendix A for endpoint details by category
- Appendix B for frontend code change examples

### For QA / Test Engineers
**Start with:** `ENDPOINT_VERIFICATION_CHECKLIST.md`
- Section on testing strategy
- Success criteria checklist
- Pre/post-implementation verification commands
- Test templates

### For DevOps / Infrastructure
**Reference:** `FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md`
- Handler distribution across files
- Timeline for implementation
- No infrastructure changes needed

---

## Key Statistics

| Metric | Value |
|--------|-------|
| Total endpoints analyzed | 88 |
| Already implemented | 60 (68%) |
| Missing/not wired | 28 (32%) |
| High priority | 14 |
| Medium priority | 10 |
| Low priority | 4 |
| Backend handlers to add | 14 |
| Frontend methods to update | 10 |
| Frontend methods to remove | 6 |
| Handlers to extend | 1 |
| Conditional routes | 1 |

---

## Implementation Timeline

```
Week 1 (Phase 1)      Week 2 (Phase 2)    Week 3 (Phase 3)    Week 4 (Phase 4)
├─ Model list         ├─ Training ctrl    ├─ Eviction        ├─ Cleanup
├─ Mappings           ├─ Auth ext         ├─ Snapshots       └─ Docs
├─ Verification       ├─ Stack validation ├─ Status          
└─ Testing setup      └─ Testing          └─ Workspaces      
```

---

## Decision Categories

### 🟢 IMPLEMENT (14 endpoints)
Backend handlers to create:
- 5 model endpoints (list, status, download, config, imports)
- 2 training controls (pause, resume)
- 3 auth extensions (logout-all, token, config)
- 1 memory eviction
- 1 stack validation
- 1 workspaces filter
- 1 metrics snapshot
- 1 status aggregate

### 🔄 MAP/ALIAS (5 endpoints)
Frontend-only updates:
- Training sessions → jobs endpoints
- Memory usage → system/memory
- Token rotate → refresh
- Anomaly status → monitoring/anomalies

### 📝 EXTEND (1 endpoint)
Enhance existing response:
- `/v1/auth/me` - Add profile data

### ❌ REMOVE (6 endpoints)
Delete from frontend:
- Orchestration (3 methods)
- Admin users (1 method)
- Security tests (1 method)

### ⚡ CONDITIONAL (1 endpoint)
Dev-only route:
- `/v1/auth/dev-bypass` - Feature flag gated

---

## Handler Module Assignments

| Module | New Handlers | Details |
|--------|--------------|---------|
| `models.rs` | 5 | list, status-all, download, cursor-config, imports |
| `training.rs` | 2 | pause, resume |
| `auth_enhanced.rs` | 3 | logout-all, token, config |
| `adapters.rs` | 1 | evict |
| `adapter_stacks.rs` | 1 | validate-name |
| `workspaces.rs` | 1 | get_user_workspaces |
| `auth.rs` | 0 | (extend existing) |
| Main handlers | 1 | status, metrics-snapshot |

---

## Files to Modify

### Backend Changes
```
crates/adapteros-server-api/src/
├── routes.rs                    [+11 routes]
└── handlers/
    ├── models.rs               [+5 handlers]
    ├── training.rs             [+2 handlers]
    ├── auth_enhanced.rs        [+3 handlers]
    ├── adapters.rs             [+1 handler]
    ├── adapter_stacks.rs       [+1 handler]
    ├── workspaces.rs           [+1 handler]
    ├── auth.rs                 [extend 1]
    └── handlers.rs or lib.rs   [+1 handler]
```

### Frontend Changes
```
ui/src/api/
├── client.ts                   [+15 methods, -6 methods, ~10 updates]
└── types.ts                    [possible response type additions]
```

---

## Quick Reference: 28 Missing Endpoints

### By Category
- **Models:** 5 endpoints
- **Training:** 4 endpoints  
- **Auth:** 6 endpoints
- **Memory:** 2 endpoints
- **Metrics & Stacks:** 3 endpoints
- **Workspaces:** 1 endpoint
- **System:** 1 endpoint
- **Deprecated:** 5 endpoints (should remove)

### By Action
- **Implement:** 14
- **Map/Alias:** 5
- **Extend:** 1
- **Remove:** 6
- **Conditional:** 1

---

## Testing Strategy Summary

### Test Coverage Required
- [ ] Unit tests per handler (14 new)
- [ ] Integration tests with auth (14 new)
- [ ] Error case handling (404, 401, 422)
- [ ] Permission verification
- [ ] OpenAPI/Swagger generation
- [ ] Frontend integration tests (10 new)
- [ ] Regression tests for existing 60 endpoints
- [ ] E2E tests for critical workflows

### Success Criteria
- All 28 missing endpoints addressed
- Zero breaking changes to existing endpoints
- All tests passing
- Documentation updated
- No regressions in 60 existing endpoints

---

## Known Already-Wired Endpoints

### Verified as Working ✓
- `/v1/adapter-stacks/deactivate` - Wired at routes.rs:751-754
- All 60 endpoints listed in ENDPOINT_VERIFICATION_CHECKLIST.md

### Need Mapping Only (No Backend Work)
- `/v1/training/sessions` → `/v1/training/start`
- `/v1/memory/usage` → `/v1/system/memory`
- `/v1/auth/token/rotate` → `/v1/auth/refresh`

---

## Risk Assessment

### Low Risk (Safe to implement immediately)
- Model list endpoints
- Metrics snapshot
- Workspaces filter
- Stack name validation
- Status aggregate

### Medium Risk (Careful implementation needed)
- Training pause/resume
- Memory eviction
- Auth extensions
- Model download (async)

### High Risk (Security critical)
- Auth logout-all (session invalidation)
- Auth config (MFA/security settings)

---

## FAQ & Common Questions

**Q: Should we implement all 28 endpoints?**
A: Not necessarily. High priority (14) are critical. Medium/Low (14) can be phased.

**Q: Can we skip training sessions?**
A: Yes! They can be mapped to existing jobs endpoints (saves 4 handlers).

**Q: What's the easiest win?**
A: Model list endpoints - straightforward CRUD operations.

**Q: What's the most complex?**
A: Model download - needs async job handling and progress tracking.

**Q: Can this be done in parallel?**
A: Yes! Model and training handlers can be done simultaneously.

**Q: Do we need DB migrations?**
A: Unlikely - most handlers use existing schema. Model imports may need tweaks.

---

## Next Actions (Step-by-Step)

1. **Review Documents (1 hour)**
   - [ ] Read FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md
   - [ ] Skim FRONTEND_BACKEND_ALIGNMENT.md sections 1-5
   - [ ] Review this index

2. **Plan & Assign (1-2 hours)**
   - [ ] Review priority matrix with team
   - [ ] Assign Phase 1 handlers
   - [ ] Create Jira/GitHub issues per endpoint

3. **Prepare Environment (30 mins)**
   - [ ] Create feature branches
   - [ ] Set up test scaffolding
   - [ ] Prepare test fixtures

4. **Implement Phase 1 (3-5 days)**
   - [ ] Model list handlers
   - [ ] Training session mappings
   - [ ] Memory endpoint mappings
   - [ ] Core testing

5. **Implement Remaining Phases**
   - [ ] Follow Phase 2-4 timeline
   - [ ] Maintain test coverage
   - [ ] Regular code reviews

6. **Final Verification**
   - [ ] Run full test suite
   - [ ] Check for regressions
   - [ ] Update documentation
   - [ ] Deploy

---

## Document Navigation

### Main Documents

**[FRONTEND_BACKEND_ALIGNMENT.md](./FRONTEND_BACKEND_ALIGNMENT.md)**
- Comprehensive 400+ line specification
- Detailed per-endpoint decisions
- Implementation plan
- Risk assessment
- Full appendices

**[FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md](./FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md)**
- Executive summary
- Quick reference table
- Phase overview
- File checklist
- Contact info

**[ENDPOINT_VERIFICATION_CHECKLIST.md](./ENDPOINT_VERIFICATION_CHECKLIST.md)**
- All 88 endpoints verified
- Testing strategy
- Success criteria
- Verification commands

### Related Files in Codebase

**Frontend API Client:**
- `ui/src/api/client.ts` - All 88 endpoint calls
- `ui/src/api/types.ts` - Request/response types

**Backend Routes & Handlers:**
- `crates/adapteros-server-api/src/routes.rs` - Route definitions
- `crates/adapteros-server-api/src/handlers/` - Handler implementations
- `crates/adapteros-server-api/src/handlers/models.rs` - Model handlers
- `crates/adapteros-server-api/src/handlers/training.rs` - Training handlers
- `crates/adapteros-server-api/src/handlers/auth_enhanced.rs` - Auth handlers

---

## Glossary

| Term | Meaning |
|------|---------|
| **Endpoint** | API route that frontend calls (e.g., `/v1/models`) |
| **Handler** | Rust function that implements endpoint logic |
| **Route** | Wiring of endpoint to handler in `routes.rs` |
| **Mapped** | Frontend calls existing endpoint instead of new one |
| **Implemented** | Backend handler exists and is wired |
| **RBAC** | Role-Based Access Control (auth permission checks) |
| **OpenAPI** | Machine-readable API specification (auto-generated) |
| **SSE** | Server-Sent Events (streaming responses) |

---

## Support & Questions

For questions about:
- **Specifications**: See Section 1-3 of FRONTEND_BACKEND_ALIGNMENT.md
- **Priorities**: See Section 5 (Summary Table)
- **Implementation**: See Section 6-7 (Implementation Plan)
- **Testing**: See ENDPOINT_VERIFICATION_CHECKLIST.md
- **Frontend changes**: See Appendix B of FRONTEND_BACKEND_ALIGNMENT.md

---

## Document Maintenance

**Last Updated:** 2025-11-22
**Maintained By:** Analysis System
**Version:** 1.0

**Review After:**
- Phase 1 completion (Week 1)
- Full implementation (Week 3)
- Production deployment

---

## Archive & History

**v1.0** (2025-11-22)
- Initial comprehensive analysis
- 88 endpoints analyzed
- 28 gaps identified
- 4-phase implementation plan

---

**Ready to begin implementation? Start with:**
1. Review summary: `FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md`
2. Assign work from Phase 1
3. Reference details as needed from `FRONTEND_BACKEND_ALIGNMENT.md`
4. Use `ENDPOINT_VERIFICATION_CHECKLIST.md` for testing

**Status: ✅ ANALYSIS COMPLETE - READY FOR DEVELOPMENT**
