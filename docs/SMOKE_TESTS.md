# Smoke Test Harness

Single command smoke checks for build, SQLx offline validity, UI build, and key API endpoints.

## Run

```bash
./scripts/mvp_smoke.sh
```

## Optional toggles

```bash
# Include clippy gate
MVP_SMOKE_CLIPPY=1 ./scripts/mvp_smoke.sh

# Run authenticated endpoint checks with a bearer token
MVP_SMOKE_TOKEN="..." ./scripts/mvp_smoke.sh
# Or reuse an existing token variable
AOS_AUTH_TOKEN="..." ./scripts/mvp_smoke.sh

# Use dev-bypass auth (debug builds with dev-bypass feature)
MVP_SMOKE_DEV_BYPASS=1 ./scripts/mvp_smoke.sh

# Point at a different API base or report path
AOS_API_URL="http://localhost:8080/api" MVP_REPORT_PATH="var/mvp_report.md" ./scripts/mvp_smoke.sh
```

## Output

- Console pass/fail for each step, plus a summary.
- Report written to `var/mvp_report.md` by default (based on `scripts/mvp_report.md`).

## Notes

- The script does not start services; ensure the API and DB are running for endpoint checks.
- Without auth, endpoint checks accept 200/401/403 as "reachable" for protected endpoints.
- Tests run with `--test-threads=1` to avoid SQLite locking.
- Requires `cargo`, `pnpm`, and `curl` in `PATH`.
