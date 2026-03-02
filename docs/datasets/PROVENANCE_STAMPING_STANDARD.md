# Provenance Stamping Standard

## Purpose
Define a deterministic, auditable provenance stamp for dataset manifests and dataset metadata used by training. This standard is normative for all new dataset versions and mandatory for `full-tune` jobs.

## Scope
Applies to:
- Normalized dataset manifest payloads (row-level/content summary + manifest hash surfaces).
- Dataset version metadata persisted at registration/training boundaries.
- Training job snapshot fields derived from dataset provenance (`dataset_hash_b3`, `dataset_version_trust`, `data_spec_hash`).

Out of scope:
- Inference-time provenance chain serialization.
- Legacy dataset versions created before this standard.

## Canonical Stamp Object
Every dataset version MUST carry a top-level `provenance_stamp` object in both manifest-adjacent metadata and persisted dataset-version metadata.

```json
{
  "provenance_stamp": {
    "schema_version": "1.0",
    "stamped_at": "2026-02-28T00:00:00Z",
    "stamp_source": "control-plane",
    "repo": {
      "commit": "<40-char git sha1>",
      "tree_dirty": false,
      "origin": "git@... or https://..."
    },
    "api_contract": {
      "openapi_sha256": "<64-char lowercase hex>",
      "openapi_path": "docs/api/openapi.json"
    },
    "algorithms": {
      "normalizer_version": "<semver or release tag>",
      "manifest_builder_version": "<semver or release tag>",
      "lineage_scorer_version": "<semver or release tag>",
      "trust_evaluator_version": "<semver or release tag>"
    },
    "trust_snapshot": [
      {
        "dataset_version_id": "<id>",
        "trust_status": "trusted|provisional|blocked|revoked",
        "semantic_status": "pass|warn|fail|unknown",
        "safety_status": "pass|warn|fail|unknown",
        "lineage_quality": "gold|silver|bronze|unknown",
        "captured_at": "2026-02-28T00:00:00Z"
      }
    ],
    "integrity": {
      "manifest_hash_b3": "<blake3 hex>",
      "data_spec_hash": "<sha256 hex>",
      "stamp_hash_b3": "<blake3 hex of canonical provenance_stamp without this field>"
    }
  }
}
```

## Field Requirements
Required for all stamps:
- `schema_version`: literal `1.0`.
- `stamped_at`: RFC3339 UTC timestamp.
- `stamp_source`: non-empty (`control-plane`, `importer`, or `migration`).
- `repo.commit`: exact 40-char lowercase hex git SHA.
- `api_contract.openapi_sha256`: SHA-256 of `docs/api/openapi.json` bytes at stamp time.
- `algorithms.normalizer_version`.
- `algorithms.manifest_builder_version`.
- `algorithms.trust_evaluator_version`.
- `integrity.manifest_hash_b3`.
- `integrity.data_spec_hash`.
- `integrity.stamp_hash_b3`.

Conditionally required:
- `algorithms.lineage_scorer_version`: required when lineage scoring ran for the dataset.
- `trust_snapshot[*]`: required when dataset is eligible for training or included in any training selector; may be empty only for inert draft versions.

Optional but recommended:
- `repo.origin`.
- `repo.tree_dirty` (MUST be `false` for production stamping; see fail-closed policy).

## Canonicalization and Hashing Rules
1. Serialization format for hash inputs MUST be canonical JSON:
- UTF-8 encoding.
- Lexicographically sorted keys at all levels.
- No insignificant whitespace.
- Arrays preserve semantic order; `trust_snapshot` sorted by `dataset_version_id` ascending before hashing.

2. `integrity.stamp_hash_b3` is computed over `provenance_stamp` with `integrity.stamp_hash_b3` omitted.

3. `integrity.manifest_hash_b3` MUST match the normalized manifest hash used by dataset version APIs and training snapshots.

4. `integrity.data_spec_hash` MUST match training job `data_spec_hash` when the dataset is selected for a training job.

## Stamping Lifecycle Rules
1. Stamp on dataset version creation:
- Create `provenance_stamp` immediately after normalization and manifest hash materialization.
- Persist stamp atomically with dataset version metadata.

2. Restamp triggers (new stamp, same dataset_version_id):
- Trust status change for the version.
- Algorithm version change that modifies normalized output or trust evaluation semantics.
- API contract hash change when compatibility-impacting provenance fields are affected.

3. Immutable invariants per dataset version:
- `repo.commit` for a given stamp is immutable.
- `api_contract.openapi_sha256` is immutable within a stamp.
- `integrity.manifest_hash_b3` and `integrity.data_spec_hash` must never be edited in place; create a new stamp instead.

4. Training snapshot capture:
- At training start, snapshot `trust_snapshot` into training job provenance (`dataset_version_trust`).
- Preserve dataset selector order in job payloads; trust snapshot must remain sortable and reproducible.

## Full-Tune Fail-Closed Policy (Mandatory)
For `full-tune`, provenance validation is hard-gated before job start. Any failure below MUST block execution.

Hard reject conditions:
- Missing `provenance_stamp`.
- Any required field absent, empty, or malformed.
- `repo.tree_dirty=true`.
- `api_contract.openapi_sha256` mismatch against runtime `docs/api/openapi.json` hash.
- `integrity.manifest_hash_b3` mismatch against fetched normalized manifest.
- `integrity.data_spec_hash` mismatch against computed training `data_spec_hash`.
- `trust_snapshot` missing for any selected dataset version.
- Any selected dataset with `trust_status` in `{blocked, revoked}`.
- `integrity.stamp_hash_b3` verification failure.

Required behavior on rejection:
- Return non-retryable validation error.
- Emit structured error code: `TRAIN_E_PROVENANCE_STAMP_INVALID`.
- Include first failing field path and expected vs observed value.
- Do not enqueue worker execution.

## Compatibility and Migration
- Legacy dataset versions without `provenance_stamp` remain readable.
- Legacy versions are ineligible for `full-tune` until stamped via migration/import flow.
- Migration MUST stamp using `stamp_source="migration"` and current `schema_version`.

## Verifier Checklist
A provenance verifier MUST perform, in order:
1. Schema + required field validation.
2. Canonical hash recomputation (`stamp_hash_b3`).
3. Manifest hash and data spec hash parity checks.
4. OpenAPI hash parity check.
5. Trust snapshot completeness and policy checks.
6. Full-tune hard-gate decision.

## Implementation Notes
- Prefer deriving versions from build metadata (release tag + commit) rather than free-form strings.
- Keep algorithm version keys stable; add new keys without renaming existing ones.
- Any schema change MUST bump `schema_version` and include forward/backward compatibility notes.
