# Requirements: adapterOS

**Defined:** 2026-02-28
**Core Value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable

## v1.1.16 Requirements (Completed)

Milestone focus: deterministic and operationally safe adapter training execution for the primary 27B path.

### Training Worker and Enqueue Safety

- [x] **TRN-46-01**: Training start fails closed with explicit API error when no healthy training worker is available.

### Determinism and Version Compatibility

- [x] **DET-46-01**: Training preflight validates dataset algorithm versions (HKDF/parser/path-normalization) against runtime constants before enqueue.

### Primary Model Runtime Consistency

- [x] **OPS-46-01**: Primary model resolution is canonicalized so training and model status reference the same active model identity/path.

### Documentation and Error Contract Grounding

- [x] **DOC-46-01**: Training failure paths expose actionable terminal reason in job status/log APIs and are citation-grounded in phase artifacts.

## v1.1.15 Requirements (Completed)

Milestone focus: operator-visible command timeline, command-deck adapter operation parity, and dataset-version-aware typed training submission.

### AdapterOps Timeline Continuity

- [x] **UX-41-01**: Adapter detail shows repository command timeline history (latest-first) so operators can inspect recent promote/checkout transitions.

### Command Deck AdapterOps Parity

- [x] **UX-41-02**: Command deck provides adapter operation actions (`Run Promote`, `Run Checkout`, `Feed Dataset`) in adapter/update-center contexts.
- [x] **A11Y-41-01**: Command intent routing remains explicit and assistive-friendly when command actions open Update Center.

### Dataset Version Contract Continuity

- [x] **VC-41-01**: Training wizard submits via typed `CreateTrainingJobRequest` and includes `dataset_version_id` when available.

### Documentation and Grounding Discipline

- [x] **DOC-41-01**: Phase artifacts include concrete code citations and best-practice references for command and contract claims.

## Future Requirements

### Adapter Operations UX (Deferred)

- **UX-42-01**: Multi-repo aggregated adapter command timeline with server-side filtering.
- **UX-42-02**: In-command-deck version selector for direct promote/checkout execution without navigation.

## Accepted External Debt

- `GOV-16` remains accepted external debt from governance milestones until canonical required-check API capability changes from `blocked_external`.

## Out of Scope (v1.1.15)

| Feature | Reason |
|---------|--------|
| Backend schema changes for timeline storage | Existing timeline endpoint contract was sufficient for required UX closure. |
| New standalone adapter operations page | Must reuse Dashboard/Update Center/Detail surfaces to avoid parallel UI paths. |
| Governance capability debt closure (`GOV-16`) | Tracked as accepted external debt outside this milestone scope. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| TRN-46-01 | Phase 46 | Complete |
| DET-46-01 | Phase 46 | Complete |
| OPS-46-01 | Phase 46 | Complete |
| DOC-46-01 | Phase 46 | Complete |
| UX-41-01 | Phase 43 | Complete |
| UX-41-02 | Phase 44 | Complete |
| A11Y-41-01 | Phase 44 | Complete |
| VC-41-01 | Phase 45 | Complete |
| DOC-41-01 | Phase 45 | Complete |

**Coverage:**
- v1.1.16 requirements: 4 total (completed)
- v1.1.16 requirements mapped to phases: 4
- v1.1.16 requirements unmapped: 0

---
*Requirements defined: 2026-02-28*
*Last updated: 2026-02-28 after v1.1.16 phase-46 completion reconciliation*
