# Runbook Drill Evidence

Each scenario directory must capture:
- `timeline.md`
- `detection_signal.md`
- `action_taken.md`
- `recovery_proof.md`
- `post_check.log`

Use `bash scripts/ci/check_runbook_drill_evidence.sh` to validate structure.
Use `RUNBOOK_DRILL_STRICT=1` to enforce non-placeholder evidence for prod go/no-go.
