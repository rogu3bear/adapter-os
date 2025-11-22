<!-- STRICT CHANGE CONTROL: DO NOT EDIT THIS DOCUMENT -->
# AdapterOS `.aos` Runtime Handling Charter

## Purpose
- Establish a permanent, canonical reference for how AdapterOS treats `.aos` artifacts.
- Shield downstream teams and agents from re-litigating packaging decisions.
- Ensure runtime consumers operate on a uniform, verifiable adapter medium.

## Non-Negotiable Mandate
1. Adapter build outputs remain in expanded directory form (`<adapter_id>/manifest.json`, weights shards, lineage, training data).
2. A packaging stage **must** convert every deployable adapter into a signed `.aos` bundle using `AosWriter`.
3. All runtime pathways (control plane import, lifecycle loader, cache, UI downloads, CLI operations) **consume only `.aos`**.
4. No service may persist or distribute raw directories outside the controlled builder sandbox.
5. The unified `.aos` format is stable and versioning-free; future enhancements must maintain backward compatibility with the 64-byte header specification.

> ❗ **DO NOT MODIFY OR DELETE THIS DOCUMENT.** Escalate to the AdapterOS Architecture Council if updates are required.

## Glossary
- **Builder Sandbox**: Training or distillation environment that produces raw adapter directories.
- **Packaging Stage**: Tooling that validates, signs, and emits `.aos`.
- **Runtime Consumer**: Any service or workflow that loads adapters for serving or inspection.
- **CAS Root**: Content-addressable storage location for final artifacts.
- **Lifecycle Loader**: Component responsible for hot-swapping adapters in production nodes.

## Lifecycle Overview
```
┌────────────────┐    ┌────────────────────┐    ┌──────────────────────┐
│ Builder Sandbox│ -> │ create-aos (signing)│ -> │ Runtime Consumers    │
│ (Directory)    │    │ validation & package│    │ (always `.aos`)      │
└────────────────┘    └────────────────────┘    └──────────────────────┘
```

### Stage Responsibilities
- **Builder Sandbox**
  - Generate deterministic manifests, lineage, and weight groups.
  - Store outputs locally; never publish directories.
- **Packaging Stage**
  - Validate adapter manifest and weights data.
  - Emit `.aos` to CAS using `AosWriter` with signed metadata.
  - Record metadata (hash, signer key ID, timestamp).
- **Runtime Consumers**
  - Fetch `.aos`, optionally via CDN or distributed cache.
  - Load through `AosLoader` or `MmapAdapterLoader`.
  - Reject any non-`.aos` payloads.

## Implementation Directives
- Control plane import (`/v1/adapters/import`) shall refuse non-`.aos` uploads.
- Lifecycle loader must prefer `.aos`, perform signature verification, and load weights without materializing raw directories.
- UI flows (Service Panel, Trainer) should only surface `.aos` for download or upload.
- CLI commands (`aos create`, `aos load`, `aos verify`) must use the unified AOS format via `AosWriter` and `AosLoader`.
- Tests must construct adapters via helper builders that mirror the packaging stage, preventing regressions in serialization.

## Operations & Observability
- Telemetry should capture `.aos` hash and signature status upon load.
- Health checks must alert when unsigned archives or invalid signatures appear.
- CAS retention policies should key off `.aos` hashes; directory artifacts are disposable.

## Change Control
- Any amendment demands approval from:
  1. AdapterOS Architecture Council
  2. Runtime Platform Lead
  3. Security & Signing Owner
- Approved revisions must integrate versioned change logs without rewriting history.
- Prior to new format adoption, provide dual-read shims that maintain `.aos` output for all existing integrations.

## Validation Checklist
- [ ] `AosWriter` invoked on every produced adapter.
- [ ] `AosLoader::load` succeeds against stored artifact.
- [ ] Signature verified and recorded.
- [ ] 64-byte header format is valid (magic bytes, offsets, sizes).
- [ ] Observability pipeline logs hash and signature key ID.

## Forward Compatibility
- The unified `.aos` format uses a fixed 64-byte header; future enhancements may extend manifest metadata.
- Backward compatibility is guaranteed: new loaders must accept archives produced by older versions.
- Capability negotiation should occur via manifest metadata without changing the binary header format.

## Migration Playbook
1. Audit current storage to ensure every adapter has a `.aos` equivalent.
2. Deprecate directory ingestion endpoints; fail closed on new requests.
3. Update docs and SDKs with `.aos`-first examples.
4. Monitor runtime metrics for load latency and signature validation rates.
5. Schedule deletion of legacy directories after retention window expires.

## Contacts
- Architecture Lead: `architecture@adapteros.dev`
- Runtime Platform Owner: `runtime@adapteros.dev`
- Security & Signing: `signing@adapteros.dev`
- Observability: `metrics@adapteros.dev`

## Audit Trail Requirements
- Packaging pipeline must log adapter ID, hash, signer key ID, and timestamp to the immutable audit store.
- Runtime logs must include adapter hash and verification result on every load event.
- Observability exports should expose `.aos` adoption rates versus legacy formats.
- Quarterly reviews confirm no directory artifacts crossed trust boundaries.
- Retention of raw directories is limited to builder sandboxes with automated scrubbing.
- Emergency exceptions require Architecture Council sign-off within 24 hours.
- Council meeting minutes related to `.aos` policy live in the governance repository.
- Restore compliance status before closing any incident involving adapter artifacts.

<!-- STRICT CHANGE CONTROL: DO NOT EDIT THIS DOCUMENT -->
