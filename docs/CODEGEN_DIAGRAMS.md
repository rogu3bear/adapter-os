# TypeScript Code Generation - Visual Diagrams

## 1. Full Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Developer Workflow                        │
│                                                              │
│  1. Make backend API changes                                │
│  2. Run: make codegen                                        │
│  3. Commit both Rust and generated TypeScript               │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
        ┌───────────────────────────────┐
        │   Dependency Check (500ms)    │
        │ ✓ Rust/Cargo                  │
        │ ✓ Node.js 18+                 │
        │ ✓ pnpm                        │
        │ ✓ openapi-typescript CLI      │
        └───────────────┬───────────────┘
                        │
                        ▼
        ┌──────────────────────────────────────┐
        │ Build Server & Export OpenAPI (30-60s)│
        │                                       │
        │ cargo build --release                 │
        │   -p adapteros-server-api             │
        │                                       │
        │ utoipa annotation processing:         │
        │ → Extract endpoint definitions        │
        │ → Extract schema definitions          │
        │ → Generate OpenAPI 3.0 spec           │
        │ → Output: target/codegen/openapi.json│
        └───────────────┬──────────────────────┘
                        │
                        ▼
        ┌──────────────────────────────────┐
        │ Generate TypeScript Types (100ms)│
        │                                  │
        │ pnpm exec openapi-typescript     │
        │   input: openapi.json            │
        │   output: types.generated.ts     │
        │                                  │
        │ ✓ Pure TypeScript types          │
        │ ✓ Zero runtime dependencies      │
        │ ✓ All endpoints covered          │
        │ ✓ Deterministic output           │
        └───────────────┬──────────────────┘
                        │
                        ▼
        ┌──────────────────────────────┐
        │ Format & Validate (500ms)    │
        │                              │
        │ pnpm exec prettier           │
        │   → Format output            │
        │   → Type consistency check   │
        │   → Endpoint verification    │
        └───────────────┬──────────────┘
                        │
                        ▼
        ┌──────────────────────────────┐
        │ Report & Summary             │
        │                              │
        │ ✓ 4 steps completed          │
        │ ✓ Total time: 40-80s         │
        │ ✓ Generated: 30-50 KB file   │
        │ ✓ Ready for commit           │
        └──────────────────────────────┘
```

---

## 2. Tool Comparison Overview

```
                     Performance vs Bundle Size

    100ms │ openapi-typescript ●
          │
   500ms+ │    @hey-api/openapi-ts ●
          │
     1-3s │         quicktype ●
          │
    2-5s  │              openapi-generator-cli ●
          │
    ──────┼──────────────────────────────────
         5KB   10KB      50KB     200KB    500KB
         (gzip bundle impact)

    ✓ Recommended: openapi-typescript
    • Fast: <100ms generation
    • Light: 5-10 KB gzipped
    • Simple: Types only
    • Perfect: Existing API client
```

---

## 3. Data Flow Diagram

```
┌──────────────────────────────────┐
│  Rust Backend (adapteros-server) │
│                                  │
│  pub async fn list_adapters() {  │
│    #[utoipa::path(...)]          │
│    → Endpoint metadata           │
│    → Request schema              │
│    → Response schema             │
│  }                               │
└──────────────┬───────────────────┘
               │
               │ [utoipa processes during build]
               ▼
        ┌────────────────┐
        │  utoipa Macro  │
        │  Extraction    │
        └────────┬───────┘
                 │
                 ▼
        ┌──────────────────────────┐
        │ OpenAPI Specification    │
        │ (JSON 3.0)               │
        │                          │
        │ {                        │
        │   "openapi": "3.0.0",    │
        │   "paths": {             │
        │     "/v1/adapters": {    │
        │       "get": { ... }     │
        │     }                    │
        │   },                     │
        │   "components": {        │
        │     "schemas": { ... }   │
        │   }                      │
        │ }                        │
        └────────┬─────────────────┘
                 │
                 │ [openapi-typescript processes]
                 ▼
        ┌────────────────────────────────────┐
        │ TypeScript Type Definitions        │
        │ (types.generated.ts)               │
        │                                    │
        │ export type paths = {              │
        │   "/api/v1/adapters": {            │
        │     get: operations["listAdapters"]│
        │   }                                │
        │ }                                  │
        │                                    │
        │ export interface ListAdaptersReq { │
        │   // request params               │
        │ }                                  │
        │                                    │
        │ export interface Adapter {         │
        │   id: string;                      │
        │   name: string;                    │
        │   // ... more fields              │
        │ }                                  │
        └────────┬─────────────────────────┘
                 │
                 │ [TypeScript compiler uses]
                 ▼
        ┌──────────────────────────────┐
        │ React Frontend (ui)           │
        │                              │
        │ import { Schema } from       │
        │   './types.generated'        │
        │                              │
        │ async getAdapters() {        │
        │   // Type-safe types         │
        │   return this.get<           │
        │     Schema.components        │
        │       .schemas.AdapterList   │
        │   >('/v1/adapters')          │
        │ }                            │
        └──────────────────────────────┘
```

---

## 4. Development Workflow

```
┌─────────────────────────────────────────────────────┐
│              Developer Workday Flow                 │
└─────────────────────────────────────────────────────┘

Morning: Feature Development
┌──────────────────┐
│ Backend Changes  │
│ (Rust code)      │
└────────┬─────────┘
         │
         ▼
┌───────────────────────┐
│ make codegen          │
│ (auto-regenerates)    │
└────────┬──────────────┘
         │
         ▼
┌─────────────────────────────┐
│ Frontend Development         │
│ pnpm dev                    │
│ (watch mode: auto-reload)   │
└────────┬────────────────────┘
         │
         ▼
        ┌─────────────────────────────────────┐
        │ Optional: Watch Mode                 │
        │ Terminal 1: cd ui && pnpm codegen:watch │
        │ Terminal 2: pnpm dev                 │
        │ Terminal 3: cargo run server         │
        └─────────────────────────────────────┘

Commit Time
┌────────────────────────────────┐
│ make codegen (re-generate)     │
│ git add [API + generated types]│
│ git commit "api: [description]"│
└────────────────────────────────┘

CI/CD Check
┌────────────────────────────────┐
│ GitHub Actions                 │
│ → make codegen                 │
│ → git diff --exit-code (check) │
│ → If changed: FAIL             │
│ → If same: PASS                │
└────────────────────────────────┘
```

---

## 5. Bundle Size Comparison

```
UI Bundle Size Impact

                    Without Codegen
    ┌────────────────────────────────────────┐
    │ UI Code:        500 KB                 │
    │ Manual Types:   5 KB                   │
    │ External Deps:  200 KB                 │
    │ ────────────────────────────────        │
    │ Total:          705 KB (gzipped: 150KB)│
    └────────────────────────────────────────┘

                    With openapi-typescript
    ┌────────────────────────────────────────┐
    │ UI Code:        500 KB                 │
    │ Generated Types: 5 KB  ← All generated │
    │ External Deps:  200 KB                 │
    │ ────────────────────────────────────    │
    │ Total:          705 KB (gzipped: 150KB)│
    │ CHANGE:         0 KB (types replace)   │
    └────────────────────────────────────────┘

                    With @hey-api/openapi-ts
    ┌────────────────────────────────────────┐
    │ UI Code:        500 KB                 │
    │ Generated SDK:  100 KB ← Includes HTTP │
    │ External Deps:  200 KB                 │
    │ ────────────────────────────────────    │
    │ Total:          800 KB (gzipped: 175KB)│
    │ CHANGE:         +25 KB (bad)           │
    └────────────────────────────────────────┘

    Winner: openapi-typescript (zero bundle change)
```

---

## 6. Type Generation Timeline

```
Before Codegen                After Codegen
(Separate Development)        (Synchronized)

Backend Types              Backend Types
(Rust)                    (Rust)
   │                         │
   ├─ Endpoint 1             ├─ Endpoint 1
   ├─ Endpoint 2             ├─ Endpoint 2
   └─ Endpoint 3             └─ Endpoint 3
   
   ↓                         ↓
[Manual sync               [Auto sync via
 effort]                   utoipa + openapi-ts]
   ↓                         ↓

Frontend Types             Frontend Types
(TypeScript)              (TypeScript)
   │                         │
   ├─ ?Endpoint 1 ✗           ├─ Endpoint 1 ✓
   ├─ ?Endpoint 2 ✗           ├─ Endpoint 2 ✓
   └─ ?Endpoint 3 ✗           └─ Endpoint 3 ✓
   
   Risk: Desync!            Guaranteed sync!
```

---

## 7. Integration Complexity

```
Integration Effort vs Feature Completeness

        
  High  │ openapi-generator-cli •
  Effort│           openapi-ts-codegen •
        │     @hey-api/openapi-ts •
        │              quicktype •
        │  
        │ openapi-typescript ●  ← BEST FOR ADAPTERIOS
   Low  │
        └──────────────────────────────
         Basic  Standard  Advanced  Complete
                 Feature Set


Selection Matrix:
┌─────────────────────────────────────────────┐
│ Need Types Only? YES   → openapi-typescript │
│ Need Full SDK?   YES   → @hey-api/openapi-ts│
│ Multi-language?  YES   → openapi-generator  │
│ JSON Modeling?   YES   → quicktype          │
└─────────────────────────────────────────────┘
```

---

## 8. Codegen Decision Tree

```
Start: Need TypeScript types from OpenAPI?
│
├─ YES: Do you have existing HTTP client?
│  │
│  ├─ YES: Use openapi-typescript ✓
│  │       └─ Types only, zero overhead
│  │
│  └─ NO: Do you need HTTP client too?
│     │
│     ├─ YES: Use @hey-api/openapi-ts
│     │       └─ Includes axios/fetch client
│     │
│     └─ NO: Use openapi-typescript ✓
│           └─ Manual client works fine
│
└─ NO: Need something else?
   │
   ├─ Multi-language SDK? → openapi-generator ✓
   ├─ JSON modeling? → quicktype ✓
   └─ Something else? → Evaluate case by case
```

---

## 9. Performance Metrics Graph

```
Generation Performance Comparison
(Milliseconds)

5000 │
4000 │                          openapi-generator
3000 │                      ▲
2000 │              quicktype    (Java startup)
1500 │           ▲
1000 │
500  │      @hey-api/openapi-ts
250  │    ▲
100  │ openapi-typescript
50   │ ●
0    └────────────────────────
     Seconds →

Production implication:
✓ <100ms: Zero perceived latency
  500ms: Barely noticeable
  1-5s:  Slightly slow
  >5s:   Noticeable delay
  
Winner: openapi-typescript (not even visible)
```

---

## 10. Git Workflow Integration

```
┌─────────────────────────────────────────┐
│   Local Development Workflow             │
└─────────────────────────────────────────┘

1. Make API changes
   git edit crates/adapteros-server-api/src/

2. Generate types
   make codegen
   └─ Generates ui/src/api/types.generated.ts

3. Update UI (if needed)
   git edit ui/src/components/

4. Stage all changes
   git add crates/adapteros-server-api/src/
   git add ui/src/api/types.generated.ts
   git add ui/src/components/

5. Commit together
   git commit -m "api: description of changes"
   └─ API + Types + Component changes = atomic

6. Push to remote
   git push origin feature-branch

7. CI Verification
   GitHub Actions:
   → make codegen
   → Check if types match current code
   → If different: FAIL (force re-sync)
   → If same: PASS (all good)
```

---

## 11. Maintenance Schedule

```
TypeScript Codegen Maintenance Calendar

Weekly (Per Release)
├─ After API changes
├─ Run: make codegen
└─ Commit: types + API changes together

Monthly
├─ Update dependencies
│  └─ cd ui && pnpm up openapi-typescript
├─ Check for deprecations
└─ Review generated file size

Quarterly
├─ Check upstream updates
├─ Review performance trends
└─ Gather team feedback

Annually
├─ Full dependency audit
├─ Consider tool alternatives
└─ Plan major upgrades
```

---

## 12. Success Metrics

```
✓ Successful Implementation Indicators

Functional:
  ✓ make codegen runs without errors
  ✓ types.generated.ts created (30-50 KB)
  ✓ TypeScript compiles without errors
  ✓ No type conflicts with existing code
  ✓ Generated exports are correct

Performance:
  ✓ Generation time < 100s (mostly build)
  ✓ TypeScript generation < 200ms
  ✓ Bundle size impact < 1 KB (types replace manual)
  ✓ No regression in build times

Quality:
  ✓ All endpoints covered
  ✓ All schemas represented
  ✓ No manual type edits needed
  ✓ Types match API specification
  ✓ Deterministic generation

Operational:
  ✓ Team understands workflow
  ✓ CI/CD validates sync
  ✓ Documentation is clear
  ✓ No breaking changes in UI
  ✓ Gradual migration possible
```

---

**Document Version:** 1.0
**Created:** November 19, 2024
**Status:** Reference Guide
