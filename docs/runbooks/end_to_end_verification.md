# Adapter/Package Chat E2E Verification Checklist

Short, repeatable flow to validate adapter training, packaging, domain install, chat behavior, citations, and telemetry across CLI and UI.

## Preconditions
- Control plane + worker running (dev mode is fine) with an admin JWT (or `AOS_DEV_NO_AUTH=1`); UI running via `pnpm dev` in `ui/`.
- `aosctl` on PATH; database at `./var/aos-cp.sqlite3`; adapters root writable.
- Export manifest hash for routing determinism: `export AOS_MANIFEST_HASH=756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e` (default from `docs/ENVIRONMENT_SETUP.md`).
- Choose a tenant ID and base model ID (e.g., default Qwen2.5-7B).

## Steps + Expected Results
1) Prepare a tiny training file  
   - Create `tmp/smoke/docs.md` with 3–5 factual lines (e.g., product facts).  
   - Expected: file exists; content will be used for citations.

2) Train + package adapter from the file  
   - Run: `aosctl train-docs --docs-dir tmp/smoke --output var/adapters/smoke-docs.aos --adapter-id smoke-docs --revision smoke --register --tenant-id <TENANT> --base-model-id <MODEL>`  
   - Expected: command completes; `.aos` written to `var/adapters/smoke-docs.aos`; adapter registered for the tenant.

3) Inspect the `.aos`  
   - Run: `aosctl adapter inspect --path var/adapters/smoke-docs.aos --json | jq '{scope_path: .manifest.scope_path, canonical_segment: .manifest.metadata.canonical_segment, scope_hierarchy: .manifest.metadata.scope_hierarchy, lora_strength: .manifest.metadata.lora_strength}'`  
   - Expected: canonical_segment present, scope_path/scope_hierarchy match docs path, lora_strength is present (default).

4) UI adapter detail strength change  
   - Open Adapter Detail for `smoke-docs`; move strength Light → Strong.  
   - Expected: UI slider updates; worker log/telemetry shows `lora_strength` change event for adapter_id `smoke-docs`.

5) Playground deterministic vs adaptive  
   - In Inference Playground select the adapter; send identical prompt twice with Deterministic then Adaptive.  
   - Expected: responses differ only when Adaptive; telemetry/logs emit `routing_determinism_mode` toggling between deterministic/adaptive.

6) Create package with default strength  
   - POST to API: `curl -X POST http://127.0.0.1:8080/v1/packages -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d '{"name":"smoke-docs-pkg","adapters":[{"adapter_id":"smoke-docs"}],"domain":"aerospace"}'`  
   - GET verify: `curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/v1/packages | jq '.packages[] | select(.name==\"smoke-docs-pkg\")'`  
   - Expected: package exists, lists adapter_strengths with smoke-docs default strength; stack_id populated.

7) Install as domain package  
   - POST: `curl -X POST http://127.0.0.1:8080/v1/tenants/<TENANT>/packages/<PKG_ID>/install -H "Authorization: Bearer $TOKEN"`  
   - GET: `curl -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:8080/v1/tenants/<TENANT>/packages?domain=aerospace"`  
   - Expected: package shows `installed:true` and appears in tenant package list for the domain.

8) Chat with the package  
   - In UI start a new chat, select `smoke-docs-pkg`.  
   - Expected: sidebar shows active adapter(s) with strengths; chat header determinism toggle present and working per-message.

9) Citations on training content  
   - Ask a direct factual question from `docs.md`.  
   - Expected: response includes at least one citation; clicking opens the snippet showing the correct line from the training file.

10) Telemetry/log verification  
   - Run: `aosctl telemetry list --database ./var/aos-cp.sqlite3 --limit 5 --json | jq '.bundles[0]'` (or inspect latest worker/server logs).  
   - Expected: at least one chat turn event contains metadata fields: adapter_id, backend, scope_path, segment_id, lora_strength, routing_determinism_mode.

11) Acceptance check  
   - Confirm all steps completed without errors; behaviors match expectations above; no missing UX/runtime pieces for the goal.

MLNavigator Inc 2025-12-08.