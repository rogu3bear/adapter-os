# Disk Full

Disk exhausted, write failures. SEV-2.

---

## Symptoms

- "No space left on device"
- SQLite: "database or disk is full"
- Log rotation fails

---

## Diagnosis

```bash
df -h var/
du -sh var/*/ | sort -hr | head -20
ls -lh var/aos-cp.sqlite3-wal
```

---

## Quick Fix

```bash
# Compress old logs
find var/logs/ -name "*.log" -mtime +7 -exec gzip {} \;

# WAL checkpoint
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Delete old telemetry
find var/telemetry/ -name "*.jsonl" -mtime +30 -delete
```

---

## Prevention

- Configure log rotation
- Set `wal_autocheckpoint`
- See [VAR_STRUCTURE.md](../VAR_STRUCTURE.md)
