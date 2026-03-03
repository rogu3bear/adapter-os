# Timeline (UTC)

- 2026-03-02T07:40:04Z: Drill started.
- 2026-03-02T07:40:04Z: Baseline disk posture captured (`df -h var/`, `du -sh var/*/ | sort -hr | head -20`, WAL size listing).
- 2026-03-02T07:40:04Z: Synthetic file-size-limited write executed to trigger disk-write failure semantics (`synthetic_disk_full_exit=153`).
- 2026-03-02T07:40:04Z: SQLite WAL checkpoint executed (`PRAGMA wal_checkpoint(TRUNCATE);`).
- 2026-03-02T07:40:04Z: Cleanup and post-check disk posture captured.
