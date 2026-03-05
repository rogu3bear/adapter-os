# Action Logs

Operational guide for local action logs (`var/logs/actions/*`) and UDS tail access (`var/run/action-logs.sock`).

---

## Scope

Action logs capture control-plane actions (job/training/service lifecycle events) as JSON lines.

As of **March 3, 2026**, HTTP endpoints for service/training logs were removed:
- `/v1/services/{service_id}/logs`
- `/v1/training/jobs/{job_id}/logs`

Use `jobs.logs_path` plus local file/UDS diagnostics instead.

---

## Security Notes

- `jobs.logs_path` is an absolute host path intended for local-node diagnostics.
- Treat path values as sensitive operational metadata; do not expose them in untrusted channels.
- UDS tail requests accept only relative paths and reject traversal/absolute prefixes.
- UDS tail resolution canonicalizes paths and rejects symlink escapes outside `var/logs`.

---

## File Layout

Base directory:

```bash
var/logs/actions/
```

Current buckets:

- `var/logs/actions/jobs/<job_id>.log`
- `var/logs/actions/training/<job_id>.log`
- `var/logs/actions/services/<service_id>.log`

Each line is one JSON object:

```json
{"timestamp":"2026-03-03T06:30:15Z","actor":"user_123","action":"start_training","outcome":"accepted","message":"training job accepted via /v1/training/start"}
```

---

## Tail via File

```bash
tail -n 200 var/logs/actions/jobs/<job_id>.log
tail -n 200 var/logs/actions/training/<job_id>.log
tail -n 200 var/logs/actions/services/<service_id>.log
```

---

## Tail via UDS

Socket:

```bash
var/run/action-logs.sock
```

Request format (single line JSON):

```json
{"path":"actions/jobs/<job_id>.log","lines":200}
```

Example:

```bash
printf '%s\n' '{"path":"actions/jobs/job_123.log","lines":100}' \
  | socat - UNIX-CONNECT:var/run/action-logs.sock
```

Notes:
- `path` must be relative (absolute paths and traversal are rejected).
- `lines` is bounded to a safe max.

---

## Retention and Rotation

Action log writes apply size-based rotation and archive pruning.

Environment knobs:

- `AOS_ACTION_LOG_MAX_BYTES` (default `5242880`)
- `AOS_ACTION_LOG_KEEP_COUNT` (default `6`)

Archives use UTC timestamp suffixes and are pruned to the newest `KEEP_COUNT`.

---

## Failure Modes

1. `action-logs.sock` missing
   - Cause: backend not running or local log service failed startup.
   - Check: `scripts/service-manager.sh status`, `tail -n 200 var/logs/backend.log`.

2. Empty/no log file for expected entity
   - Cause: no action events yet, or write failures.
   - Check: `tail -n 200 var/logs/backend.log` for `failed to append ... action log`.

3. UDS request rejected (`ok=false`)
   - Cause: invalid relative `path` or bad JSON request.
   - Fix: use `actions/<bucket>/<id>.log` format.

4. Unexpected growth
   - Cause: retention env overrides too high or unset in runtime shell.
   - Check: `echo $AOS_ACTION_LOG_MAX_BYTES $AOS_ACTION_LOG_KEEP_COUNT`.
