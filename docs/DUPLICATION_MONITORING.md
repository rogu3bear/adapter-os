Duplication Monitoring (jscpd)
================================

Overview
--------
- Uses `jscpd` to scan for duplicate code across the repository (Rust, Swift, TS/JS, etc.).
- Writes timestamped reports to `var/reports/jscpd/<YYYYMMDD-HHMMSS>/` as JSON, HTML, and Markdown.
- Runs on pull requests and posts a summary comment. An optional input can enforce failure.

Local Usage
-----------
- Run a scan: `make dup`
- Reports appear under: `var/reports/jscpd/<timestamp>/`

Configuration
-------------
- Core config: `configs/jscpd.config.json`
  - `minTokens` (default 70) controls the minimum token length for clones.
  - `ignore` excludes build outputs, caches, and dependencies.
- Override tokens temporarily: `JSCPD_MIN_TOKENS=90 make dup`

CI Integration
--------------
- Workflow: `.github/workflows/duplication.yml`
  - Auto-runs on PRs and attaches artifacts; comments a summary in the PR.
  - Manual enforcement: trigger the workflow with `enforce: true` to fail if any clones are found.
  - Repo-wide enforcement: set repository variable `JSCPD_ENFORCE=true`.

Guidance
--------
- Treat the report as a prompt to refactor: extract shared functions/modules or remove dead code.
- If you must temporarily accept duplication, land the PR and then plan a refactor to remove it.
- Keep generated files, build outputs, and vendored deps excluded to avoid noise.

Requirements
------------
- Node.js (for `npx jscpd` and to summarize JSON reports).
- Network access on first run to fetch the `jscpd` package via `npx`.

Optional Hooks
--------------
- Install git hooks: `bash scripts/install_git_hooks.sh`
- Enforce locally (block commits on clones): `export JSCPD_ENFORCE=1`
