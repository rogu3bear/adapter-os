# Production Backup, Restore, and DR Runbook

## Objectives and Success Criteria
- Keep prod backups fresh (<24h) with integrity logs for audit.
- Weekly restore drills succeed end-to-end.
- Post-restore health verified (API up, DB integrity ok).

## Assets and Paths to Capture
- Control-plane DB: `AOS_DATABASE_URL` (default `sqlite://var/aos-cp.sqlite3`).
- KV store: `AOS_KV_PATH` (default `var/aos-kv.redb`).
- KV search indexes: `AOS_KV_TANTIVY_PATH` (`var/aos-kv-index`) and `AOS_TANTIVY_PATH` (`var/aos-search`).
- Adapters: `AOS_ADAPTERS_DIR` (default `var/adapters/repo`).
- Artifacts/logits: `AOS_ARTIFACTS_DIR` (default `var/artifacts`).
- Model cache + tokenizer: `AOS_MODEL_CACHE_DIR` (default `var/model-cache`).
- Control-plane config: `AOS_CONFIG_PATH` (default `configs/cp.toml`).

Use absolute paths in production (e.g., `/opt/adapteros/var/...`).

## Key Management
Store the encryption key at `/etc/aos/backup.key` (mode 600). Generate if missing:
```bash
sudo install -m 600 /dev/null /etc/aos/backup.key
sudo openssl rand -hex 64 | sudo tee /etc/aos/backup.key >/dev/null
```

## Backup Workflow (encrypted, deterministic)
Run nightly or ad-hoc:
```bash
/opt/adapteros/scripts/backup/backup.sh
```
Environment knobs:
- `AOS_BACKUP_ROOT` (default `/var/backups/aos`)
- `AOS_DATA_ROOT` (default `<repo>/var`) and `AOS_CONFIG_PATH`
- `AOS_BACKUP_KEY_PATH` (default `/etc/aos/backup.key`) and `AOS_BACKUP_KEY_ID` (name in backup filename)
- `AOS_BACKUP_RETENTION_DAYS` (default `7`) and optional `AOS_BACKUP_RETENTION_BYTES` (prune oldest to size cap)
- `AOS_BACKUP_OFFSITE_ROOT` (optional second rsync target for offsite/NFS/WORM)
- Optional signing: `AOS_BACKUP_SIGN_KEY`, `AOS_BACKUP_SIGN_KEY_ID`, and `AOS_BACKUP_VERIFY_PUBKEY` on verify/restore
- Optional hooks: `AOS_BACKUP_HOOK_PRE` / `AOS_BACKUP_HOOK_POST` to quiesce services or emit metrics
- Enforcement knobs: `AOS_BACKUP_REQUIRE_OFFSITE=1`, `AOS_BACKUP_REQUIRE_SIGNING=1`, `AOS_BACKUP_REQUIRE_QUIESCE=1`, `AOS_BACKUP_REQUIRE_SIGNATURE=1` (verify/restore) to fail fast when guardrails are missing.

Behavior: SQLite `.backup`, rsync of KV/indexes/adapters/artifacts/model-cache/config, SHA256 manifest, `metadata.json`, AES-256-GCM encryption, retention pruning, JSON logs to stdout/stderr for scraping.

## Integrity Verification (daily/CI)
```bash
/opt/adapteros/scripts/backup/verify-backups.sh
```
Checks: decrypts latest backup, recomputes checksums, runs `PRAGMA integrity_check` on `db/aos-cp.sqlite3`. Exit non-zero on failure for CI alarms.
If `AOS_BACKUP_VERIFY_PUBKEY` is set and a `.sig` is present beside the backup, the signature is verified before decrypting.

## Restore Drill (weekly)
```bash
/opt/adapteros/scripts/backup/test-restore.sh
# optional overrides:
# AOS_TARGET_ROOT=/var/tmp/aos-restore-test AOS_BACKUP_ROOT=/var/backups/aos
```
What it does: decrypts latest backup, integrity-checks SQLite, restores into `AOS_TARGET_ROOT` (default mktemp), runs server health probe on port `18080` when `adapteros-server` and `curl` are available (uses `AOS_DEV_NO_AUTH=1`, isolated data paths). Logs JSON; keeps restored data for inspection under `AOS_TARGET_ROOT`.
If `AOS_BACKUP_VERIFY_PUBKEY` is set and a `.sig` exists, signature is verified pre-decrypt.

## CI/Preflight
- `scripts/backup/ci-smoke.sh` creates a minimal temp dataset, runs `backup.sh`, `verify-backups.sh`, and `test-restore.sh` end-to-end. Use in pipelines to satisfy acceptance tests without real prod data.
- In CI, set `AOS_BACKUP_REQUIRE_SIGNATURE=1` and provide `AOS_BACKUP_VERIFY_PUBKEY` when signatures are expected.

## Scheduling and Monitoring
- Cron template: `scripts/backup/cron.example` (nightly backup 02:15 UTC, daily verify 09:00 UTC). Update paths to your install prefix (`/opt/adapteros` recommended) and log to `/var/log/adapteros/backup.log`.
- Alerts: page on missing backup newer than 24h, verify failure, or restore drill failure. Track metrics: backup age, verify status, last drill timestamp, duration.
- Health export: ship JSON logs from scripts to your log pipeline; add dashboard panels for freshness and last-OK statuses.
- Offsite/WORM: set `AOS_BACKUP_OFFSITE_ROOT` for a second copy; prefer append-only/immutable storage where available.
- Key rotation: change `AOS_BACKUP_KEY_PATH` + `AOS_BACKUP_KEY_ID`; keep old keys to decrypt older backups until expired.
- Quiesce/snapshot: in production, set `AOS_BACKUP_REQUIRE_QUIESCE=1` and use `AOS_BACKUP_HOOK_PRE/POST` to pause writes or take FS snapshots during backup.

## Post-Restore Verification Checklist
- `sqlite3 <restored>/var/aos-cp.sqlite3 "PRAGMA integrity_check;"` returns `ok`.
- KV files present (`aos-kv.redb`, indexes) and adapters directory populated.
- `curl -fs http://127.0.0.1:18080/health` succeeds when `test-restore.sh` runs with server binary available.
- Validate cp.toml in restored configs matches expected tenant/egress settings.

## Incident Runbook (backup/restore)
- Backup missing/stale: rerun `backup.sh`, check key perms, disk space, and cron status.
- Verify failures: inspect `checksums.current` diff, re-run `backup.sh`, consider key rotation and disk checks.
- Restore drill failure: capture `${TMPDIR}/server.log` from `test-restore.sh`, rerun with `AOS_TARGET_ROOT` on a clean path, ensure `adapteros-server` binary present, and rerun health probe.
- Audit trail: retain `metadata.json` from restored bundle and backup JSON logs in your incident ticket.

## Acceptance Gates
- CI jobs run `scripts/backup/verify-backups.sh` (integrity) and `scripts/backup/test-restore.sh` (drill + health) with the latest artifact.
- Operations confirm weekly drill completion and backup freshness <24h.

MLNavigator Inc 2025-12-06.

