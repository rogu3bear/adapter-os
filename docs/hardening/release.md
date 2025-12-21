## Release Provenance & Build Hardening

- Purpose: prove binary identity, reduce supply-chain risk, and surface build provenance in evidence bundles.
- Scope: signed SBOM + build provenance, reproducible option, build hash captured in context manifests, CI release signing gate.

### Workflow
- Build (reproducible option): `SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) cargo build --release --workspace --locked`.
- Stage SBOM + provenance: `bash scripts/release/sbom.sh` (uses `b3sum`; artifacts pulled from `target/release/{adapteros-server,aos_worker,aosctl}` by default; override with `ARTIFACTS="path1 path2"`).
- Signing (Ed25519): set `RELEASE_SIGNING_KEY_PEM` (PEM private key) and `RELEASE_SIGNING_PUBKEY_HEX`; script emits `signature.sig`, `build_provenance.sig`, `public_key.hex`.
- Bundle layout under `target/release-bundle/`:
  - `sbom.json` with build block (`build_id`, `git_sha`, `workspace_hash`, `source_date_epoch`) and artifact hashes.
  - `build_provenance.json` mirroring build block + `sbom_hash`, `context_manifest_hash`.
  - `signature.sig`, `build_provenance.sig`, `public_key.hex`.
  - `artifacts/` copies of release binaries (mtime normalized to `SOURCE_DATE_EPOCH`).

### Verification
- Local: `aosctl verify bundle <bundle.tar.zst>` checks signatures (SBOM + build provenance), SBOM completeness, artifact hashes, and reports bundle hash.
- Provenance guardrails:
  - `sbom_hash` in `build_provenance.json` must match the computed BLAKE3 of `sbom.json`.
  - `build_id`/`git_sha` in SBOM and provenance must align; `build_git_sha` is embedded into context manifests for replay integrity.

### CI hook
- GitHub Actions `ci.yml` release build now installs `b3sum` and runs `scripts/release/sbom.sh` (secrets `RELEASE_SIGNING_KEY_PEM`/`RELEASE_SIGNING_PUBKEY_HEX` gate signing); outputs are uploaded as artifacts for audit.

### Notes
- Determinism: context manifest digest now incorporates `build_git_sha`; changes in git SHA or build ID alter manifest hash and replay receipts.
- Signing keys: keep PEM offline; CI secrets should be org-level and scoped to release branches/tags only.
- Air-gap: tar/zstd the `target/release-bundle/` directory for transfer; verification works offline.

MLNavigator Inc Dec 11, 2025.
