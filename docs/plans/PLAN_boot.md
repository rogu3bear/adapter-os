# PLAN

## Objective
- Make the first-run dev path deterministic: one documented command boots the backend, UI loads at `/`, dashboard renders, and the chat dock shows an explicit blank state.

## Dependencies
- Rust toolchain (`cargo`) for server builds.
- `trunk` for UI build output in `crates/adapteros-server/static`.
- Optional: `wasm-opt` for UI size optimization (not required for first-run).
- SQLite tooling if manual inspection is needed (server can create DB file).

## Decisions
- Bootstrap: wrapper script `scripts/dev-up.sh` (locked).

## Current blocker
- None.

## Checklist
1) Completed: Baseline repository + guidance review.
   Evidence:
   - Command: `ls`
     Output (excerpt): `AGENTS.md`, `start`, `scripts/`, `crates/`, `configs/`
   - Command: `cat AGENTS.md`
     Output (excerpt): repository structure and start/test guidance
   - Paths touched: `AGENTS.md`
   - Reason: confirms current guidance and entry points before changes.

2) Completed: Identify UI asset gap in server boot path.
   Evidence:
   - Command: `ls static`
     Output (excerpt): `No such file or directory`
   - Command: `sed -n '1,200p' crates/adapteros-server/src/assets.rs`
     Output (excerpt): `UI not built. Run: cd crates/adapteros-ui && trunk build --release`
   - Command: `sed -n '700,820p' scripts/service-manager.sh`
     Output (excerpt): `UI is served by the backend from static/`
   - Paths touched: `crates/adapteros-server/src/assets.rs`, `scripts/service-manager.sh`
   - Reason: confirms `/` returns 503 unless UI is built; `./start` does not build UI.

3) Completed: Verify UI has explicit chat blank states and dock.
   Evidence:
   - Command: `sed -n '1,160p' crates/adapteros-ui/src/components/layout.rs`
     Output (excerpt): `ChatDockPanel` and `MobileChatOverlay` rendered in `Shell`
   - Command: `sed -n '240,520p' crates/adapteros-ui/src/components/chat_dock.rs`
     Output (excerpt): `"Start a conversation"` empty state
   - Command: `sed -n '220,520p' crates/adapteros-ui/src/pages/chat.rs`
     Output (excerpt): `"No messages yet. Start the conversation!"`
   - Paths touched: `crates/adapteros-ui/src/components/layout.rs`, `crates/adapteros-ui/src/components/chat_dock.rs`, `crates/adapteros-ui/src/pages/chat.rs`
   - Reason: confirms blank states exist once UI loads.

4) Completed: Update AGENTS.md with agent scope and done definition.
   Evidence:
   - Command: `sed -n '45,80p' AGENTS.md`
     Output (excerpt): `## Agent Role & Scope (Alignment)` section
   - Paths touched: `AGENTS.md`
   - Reason: required alignment artifact per instructions.

5) Completed: Choose bootstrap strategy for the single-command dev boot (Option 2: `scripts/dev-up.sh`).
   Evidence:
   - Path created: `scripts/dev-up.sh`
   - Reason: canonical, idempotent dev bootstrap without changing `./start`.

6) Completed: Implement bootstrap flow to ensure server binary, DB migrations, and UI static assets exist before startup.
   Evidence:
   - Command: `sed -n '1,200p' scripts/dev-up.sh`
     Output (excerpt): preflight, build, UI asset validation, migrations, backend checks
   - Paths touched: `scripts/dev-up.sh`
   - Reason: required for `/` to load from a fresh clone.

7) Completed: Update docs to name the single command and describe readiness.
   Evidence:
   - Command: `sed -n '70,130p' QUICKSTART.md`
     Output (excerpt): `./scripts/dev-up.sh` dev bootstrap instructions
   - Paths touched: `QUICKSTART.md`
   - Reason: primary objective requires a documented command.

8) Completed: Run the documented command and capture evidence.
   Evidence:
   - Command: `./scripts/dev-up.sh 2>&1 | tee /tmp/dev-up-run.log`
     Output (excerpt): `/healthz` 200, `/readyz` 200 with worker not registered
   - Command: `curl -sS http://127.0.0.1:8080/ | sed -n '1,30p'`
     Output (excerpt): `<!DOCTYPE html>` and UI asset links
   - Paths touched: `scripts/dev-up.sh`, `var/logs/dev-up-backend.log`
   - Reason: "prove before claim" for the primary objective.

## Next step
- None.
