# AdapterOS — End-to-End Visual Sweep Reference

**What this is:** A reference for an agent that will walk every user-facing flow in AdapterOS, fix what's broken, and leave behind a product where a human can create adapters, train them, and talk to them — seeing how adapters change the conversation.

**How to use it:** Read this, internalize the loop, then go. This is not a checklist. It's a map and a method.

---

## The Loop

Everything you do follows this cycle:

```
Navigate → Observe → Broken? → Understand → Fix → Re-navigate → Verify → Next
     ↑                                                                    │
     └────────────────────────────────────────────────────────────────────┘
```

You are a user. Open the browser. Walk the product. When something is wrong — and you'll know because you can see it — figure out why, fix it, go back, confirm it's fixed, then keep walking. Every time you fix something, you re-enter the loop. Every time you finish a flow, you start the next one.

**The browser is your eyes.** Navigate, snapshot, interact, snapshot again. If a page looks wrong, that's the signal. If a flow breaks, that's the signal. If text overflows, buttons don't work, errors are silent, loading never finishes — those are all signals. Logs and curl are for diagnosis after you see the problem, not for finding it.

---

## What "Done" Looks Like

A human sits down, runs `AOS_DEV_NO_AUTH=1 ./start`, opens `http://localhost:18080`, and can:

1. **See** a working UI — dashboard loads, navigation works, pages render
2. **Create** an adapter — upload files, name it, start training, watch it complete
3. **Talk** to the model — open chat, send messages, get responses
4. **See the difference** — chat without adapter gives a generic response; chat with adapter gives a response shaped by the adapter's training data
5. **Trust** the UI — errors show up as toasts or banners, not blank screens; failures are explained, not silent

That's it. If all five are true, you're done.

---

## Environment

### Boot

```bash
AOS_DEV_NO_AUTH=1 ./start
```

This bypasses auth, seeds the model (if `AOS_MODEL_PATH` is set), starts the backend + worker, and waits for inference readiness. If the UI is blank, run `./scripts/build-ui.sh`.

### Models

Inference requires a real model loaded on a real worker. No mocks.

```bash
# Download if needed
./scripts/download-model.sh

# Or manually
huggingface-cli download mlx-community/Llama-3.2-3B-Instruct-4bit \
  --include "*.safetensors" "*.json" \
  --local-dir var/models/Llama-3.2-3B-Instruct-4bit

export AOS_MODEL_PATH=var/models/Llama-3.2-3B-Instruct-4bit
```

After boot, confirm readiness: `curl -s http://localhost:18080/readyz | jq`. If `inference_ready` is false, the worker or model isn't loaded — diagnose from there.

### Backend Sanity

`scripts/golden_path_adapter_chat.sh` runs the entire adapter creation and inference flow from the CLI. If this script passes, the backend is sound and the problem is UI-side. If it fails, fix the backend first.

---

## The Flows (Walk These)

### Flow 1: First Load

Open the browser. Navigate to the app. What do you see?

- Does the page load? Does the boot overlay clear?
- Is there a clear starting point — a way to create an adapter or start working?
- Does navigation work? Can you reach Chat, Adapters, Training?

If anything is broken here, nothing else matters. Fix it first.

### Flow 2: Create an Adapter

This is the core value prop. A user uploads their files, names an adapter, trains it, and gets something they can use in chat.

Walk the wizard. Upload a document (a markdown file, a PDF, anything small). Name the adapter. Start training. Watch the progress. Wait for completion.

**What to look for:**
- Can you actually upload a file? Does the drop zone work, does progress show?
- Are the steps clear? Can a non-technical user follow them without understanding JSONL, datasets, or hyperparameters?
- Does training start? Does progress update? Does it complete?
- After completion, is there a clear "go use this" action?

**Where things live:**
- Wizard: `crates/adapteros-ui/src/pages/training/wizard.rs`
- Dataset step: `wizard.rs` (DatasetStepContent, DatasetChooseView)
- Upload: `upload_dialog.rs`, `data/mod.rs`
- API: `POST /v1/documents`, `POST /v1/training/datasets/from-upload`, `POST /v1/training/jobs`
- See also: `docs/PROMPT_CODEX_MVP_ADAPTER_FLOW.md` for the full reframe

### Flow 3: Chat — Without Adapter

Open Chat. Send a message. Get a response from the base model.

**What to look for:**
- Does the input area work? Can you type and submit?
- Does a response stream in or appear?
- Is the response readable, properly formatted?

**Where things live:**
- Chat page: `crates/adapteros-ui/src/pages/chat.rs`
- Chat dock: `components/chat_dock.rs`
- API: `POST /v1/chat/completions` or `POST /v1/infer`
- Client: `crates/adapteros-ui/src/api/client.rs`

### Flow 4: Chat — With Adapter

Now select the adapter you created. Send the same message (or a related one). Compare.

**What to look for:**
- Can you select/deselect adapters from the adapter bar?
- Does the response change when an adapter is active?
- Is it obvious to the user which adapters are selected?
- Does the adapter magnet UI work (not overflow, not cut off)?

**Where things live:**
- Adapter bar: `components/adapter_bar.rs`
- Adapter magnets: CSS in `dist/components/pages.css` (`.adapter-magnet-*`)

### Flow 5: Adapters Page

Navigate to the adapters list. Is the adapter you created there? Can you open its detail?

**What to look for:**
- List renders, adapter is visible
- Detail panel shows metadata (hash, rank, base model, framework)
- Long IDs don't overflow or break layout
- Version/promotion section (if visible) is coherent

**Where things live:**
- List: `crates/adapteros-ui/src/pages/adapters.rs`
- Detail panel: `components/adapter_detail_panel.rs`

### Flow 6: Everything Else

Once the core flows work, explore. Navigate every page you can reach. Open dialogs. Resize the window. Try edge cases.

**Look for:**
- Overflow, truncation, layout breaks
- Silent failures (you click something and nothing happens)
- Error states that are blank or show raw JSON instead of a human message
- Loading states that never resolve
- Buttons that look clickable but aren't, or vice versa

---

## How to Fix Things

When you see something wrong:

1. **Snapshot** the state — you already have it from the browser
2. **Understand** the cause — is it CSS? Is it a missing API call? Is it a broken signal? Is the backend returning an error the UI doesn't handle?
3. **Find the file** — use the anchors above, or search the codebase
4. **Fix in place** — follow existing patterns, minimal diff, don't refactor what isn't broken
5. **Re-navigate** — go back to the page, re-do the action, confirm it's fixed
6. **Move on** — re-enter the loop

### Guardrails

- **Runtime data:** `./var/` only. Never `/tmp`, `/private/tmp`, `/var/tmp`.
- **Minimal diffs:** Prefer existing patterns. Don't rename, restructure, or refactor unless the fix requires it.
- **Scope:** Fix what you see. If you notice an adjacent issue that's not in a flow you're walking, note it and keep going.
- **Real inference:** No mocks, no stubs, no fake responses. The model must be loaded, the worker must be running, `/readyz` must pass.
- **Auth bypass:** `AOS_DEV_NO_AUTH=1`. Do not modify auth/RBAC code to unblock flows.
- **Follow CONTRIBUTING.md** for code conventions.

---

## Diagnosis Tools (When You See a Problem)

| What you see | First check | Then |
|---|---|---|
| Blank page | `./scripts/build-ui.sh`, then reload | Check browser console for WASM errors |
| "Inference not ready" | `curl -s localhost:18080/readyz \| jq` | `aosctl models seed --model-path $AOS_MODEL_PATH` |
| Training won't start | `curl -s localhost:18080/v1/training/jobs \| jq` | Check trust_state, dataset status |
| No response in chat | `curl -X POST -H "Content-Type: application/json" -d '{"prompt":"Hi","max_tokens":16}' localhost:18080/v1/infer` | Worker logs: `var/logs/worker.log` |
| 401/403 | Confirm `AOS_DEV_NO_AUTH=1` is set | Restart with the env var |
| Worker crashed | `var/logs/worker.log` | Ensure `AOS_MODEL_PATH` is valid |

---

## Implementation Anchors

Quick reference for where things live when you need to fix them:

| Area | Primary file(s) |
|------|----------------|
| Training wizard | `pages/training/wizard.rs` |
| Dataset upload | `pages/training/dataset_wizard.rs`, `upload_dialog.rs` |
| Training data state | `pages/training/data/state.rs`, `data/mod.rs` |
| Chat page | `pages/chat.rs` |
| Chat dock (side panel) | `components/chat_dock.rs` |
| Adapter bar / magnets | `components/adapter_bar.rs` |
| Adapter detail | `components/adapter_detail_panel.rs` |
| Adapters list | `pages/adapters.rs` |
| Dashboard | `pages/dashboard.rs` |
| Welcome / first-run | `pages/welcome.rs` |
| Command palette | `components/command_palette.rs` |
| API client (all endpoints) | `api/client.rs` |
| CSS: core components | `dist/components/core.css` |
| CSS: page-specific | `dist/components/pages.css` |
| CSS: layout | `dist/components/layout.css` |
| CSS: overlays/dialogs | `dist/components/overlays.css` |

All paths relative to `crates/adapteros-ui/src/`.

---

## Related Docs

- [PROMPT_CODEX_MVP_ADAPTER_FLOW.md](PROMPT_CODEX_MVP_ADAPTER_FLOW.md) — MVP flow reframe, terminology, what to simplify
- [QUICKSTART.md](../QUICKSTART.md) — Build, init, start
- [TRAINING.md](TRAINING.md) — Training pipeline, CLI, backends
- [APIS.md](APIS.md) — API types, request/response format
- [ROUTES.md](ROUTES.md) — Route tiers, middleware chain
