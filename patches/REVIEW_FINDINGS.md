# UI Re-entrancy Fix Review Findings

Reviewer: Claude Opus 4.6 (agent: reviewer)
Date: 2026-02-08

## Scope

Reviewed all UI changes from previous sessions that address the wasm-bindgen-futures
RefCell re-entrancy panic (issue #2562). These changes apply workarounds at the
call-site level while a proper wasm-bindgen-futures patch is being designed (tasks #1, #2).

---

## File-by-File Review

### 1. `pages/chat.rs` (lines 54-143) -- Chat + ChatSession components

**Pattern**: Deferred `navigate()` via `Timeout::new(0, ...)`, deferred mounting via
`Show` + `mounted` signal.

**Assessment: CORRECT with caveats**

- The `Timeout::new(0, ...)` deferral for `navigate()` in `Chat` (line 71) correctly
  breaks out of the wasm-bindgen-futures task queue context. The `move ||` closure
  captures `navigate` and `path` safely.
- The early `return view! { <div class="chat-redirect" /> }.into_any()` (line 83)
  prevents signal/effect creation that would panic when the redirect disposes the
  component -- correct.
- `ChatSession` (line 115) uses the same `mounted` + `Show` pattern to defer
  `ChatWorkspace` construction. The `Spinner` fallback provides visual continuity.

**Caveat -- Timeout(0) has a visible frame**:
The `Timeout::new(0, ...)` defers to the next macrotask. On slow devices, the user
may see a single frame of the `<Spinner>` before the full workspace renders. This is
cosmetically acceptable but not zero-cost. If the wasm-bindgen-futures patch lands
(task #2), these deferrals become unnecessary.

**Caveat -- Redirect race with slow navigate**:
If `navigate()` is slow (network delay on hash-based routing is negligible, but
Leptos route matching is synchronous), the user could briefly interact with the
spinner. Since the spinner has no interactive elements, this is safe.

---

### 2. `components/layout/shell.rs` -- Outlet pattern

**Assessment: CORRECT**

- `Shell` renders `<Outlet/>` (line 159) instead of taking `Children`. This is the
  canonical Leptos 0.7 pattern for `ParentRoute`.
- The `Outlet` renders whichever child `Route` matches, so `Shell` is instantiated
  **once** per ParentRoute lifetime rather than being re-created on every navigation.
- The `Workspace` wrapper around `Outlet` is fine -- it adds CSS structure without
  interfering with Leptos routing.

---

### 3. `components/layout/taskbar.rs` -- `try_with_value` fix

**Assessment: CORRECT**

- Line 83: `routes.try_with_value(|routes| { ... }).unwrap_or(false)` gracefully
  handles the case where `StoredValue` is disposed during SPA navigation re-renders.
- The `unwrap_or(false)` fallback is semantically correct: if the StoredValue is
  disposed, the module button should not appear active.
- **No silent error swallowing**: disposal of a `StoredValue` during navigation is an
  expected lifecycle event, not an error. Returning `false` is the correct default.

---

### 4. `components/layout/topbar.rs` -- `try_get_untracked` / `try_set`

**Assessment: CORRECT**

- Lines 72, 97: `user_menu_open.try_get_untracked().unwrap_or(false)` in the leaked
  click/key closures correctly handles disposal. If the signal is gone, the menu
  isn't open.
- Lines 93, 101: `set_user_menu_open.try_set(false)` silently no-ops if disposed.
  This is correct because the menu is gone anyway.
- Line 105: `search.command_palette_open.try_get_untracked().unwrap_or(false)` in the
  keyboard handler -- correct default.
- Line 116: `ui_profile.try_get_untracked()` returns `None` if disposed, skipping
  the alt-shortcut. Correct behavior.
- Line 298: `auth_action_signal.try_with_value(...)` for the logout button -- correct,
  since if the StoredValue is disposed, the button is gone too.

**No silent error swallowing risk**: All `try_` uses are in leaked closures (`.forget()`)
where disposal is expected and the fallback behavior is correct.

---

### 5. `components/chat_dock.rs` -- Timeout(10) for scroll

**Assessment: CORRECT**

- Lines 646-658: The `MessageList` Effect uses `Timeout::new(10, ...)` to defer
  `set_scroll_top()`. This is a DOM timing issue (content needs to render before
  scrolling), not a re-entrancy fix. The 10ms delay is appropriate for this purpose.
- This is not wrapping `spawn_local` -- it's wrapping a synchronous DOM operation.
  No re-entrancy concern here.

---

### 6. `hooks/mod.rs` -- Deferred spawn_local in polling hooks

**Assessment: CORRECT with one observation**

- `use_api_resource` (line 180): Wraps `spawn_local` inside `Timeout::new(0, ...)`.
  Uses `try_set` for signal updates within the timeout. Version counter ensures stale
  responses are discarded. The `Effect::new` at line 227 wraps the initial fetch in
  `untrack()` to prevent reactive re-runs.
- `use_polling` (line 288): Initial fetch deferred via `Timeout::new(0, ...)`. The
  `initialized` flag prevents duplicate intervals on Effect re-run. Interval callbacks
  use `spawn_local` directly (correct -- they run outside Effect context).
- `use_conditional_polling` (line 413): Same pattern as `use_polling` but with
  `should_poll` signal. Initial fetch deferred via `Timeout::new(0, ...)`.

**Observation -- Timeout handle leak**:
All `Timeout::new(...).forget()` calls leak the timeout handle. In WASM, there is no
way to cancel a forgotten timeout. However, since these are 0ms timeouts (fire
immediately), the leak is negligible. The version counter / `try_set` guards prevent
stale updates.

---

### 7. `components/layout/system_tray.rs` -- `try_set` in interval

**Assessment: CORRECT**

- Line 44: `set_time.try_set(get_current_time())` inside the leaked `Interval`
  callback. If the signal is disposed (Shell recreated), the update is a no-op.
- The `interval_created` StoredValue guard (line 41) prevents duplicate intervals
  on Effect re-run. Correct pattern.

---

### 8. `components/status_center/hooks.rs` -- Timeout-wrapped spawn_local

**Assessment: CORRECT with one concern**

- `use_status_data` (lines 150-171): Wraps `spawn_local` in `Timeout::new(0, ...)`.
  Uses `set_state.try_set(StatusLoadingState::Loading)` before the timeout (line 148),
  but then uses `set_state.set(...)` (not `try_set`) inside the spawn_local
  (lines 160, 166).

**CONCERN -- Missing `try_set` inside spawn_local**:
Lines 160 and 166 use `set_state.set(...)` instead of `set_state.try_set(...)`. If the
component unmounts while the async fetch is in-flight, `set_state.set(...)` could
panic because the signal is disposed. The version-counter pattern used in
`use_api_resource` would be a better approach here.

**Severity**: Low. The `use_status_data` hook is called from `StatusCenterProvider`
which wraps the entire Shell. The Shell is now stable (ParentRoute prevents
re-creation), so disposal during an in-flight fetch is unlikely. But it's still a
latent bug.

---

### 9. `signals/auth.rs` -- Timeout-wrapped spawn_local (lines 342-401)

**Assessment: CORRECT**

- The auth check Effect (line 342) uses `has_checked` StoredValue guard to run only
  once. The `spawn_local` is wrapped in `Timeout::new(0, ...)` (line 351).
- The async block uses `state_timeout.set(...)` for auth state updates. Since auth
  context is provided at the top level (`AuthProvider` wraps everything), the signal
  will not be disposed during the async operation.

---

### 10. `lib.rs` -- ParentRoute + Outlet route structure (lines 194-232)

**Assessment: CORRECT**

- `ParentRoute path=path!("") view=|| view! { <ProtectedRoute><Shell/></ProtectedRoute> }`
  wraps all protected routes. This means `Shell` is instantiated once and persists
  across child route changes.
- `Shell` uses `<Outlet/>` to render the active child route.
- Non-shell routes (`/login`, `/safe`, `/style-audit`) are outside the `ParentRoute`,
  so they render without Shell. This is correct.
- Backward compatibility redirects (`/dashboard`, `/flight-recorder`) are also outside
  the `ParentRoute` but wrap in `ProtectedRoute`. They redirect immediately, so they
  don't need Shell.

**Import verification**: `ParentRoute` is imported via `use leptos_router::components::*`
(line 41). `Outlet` is imported explicitly in `shell.rs` (line 23). Both are standard
Leptos 0.7 APIs.

---

## Unfixed `spawn_local` inside `Effect::new` -- Missed Locations

The following locations have `spawn_local` directly inside an `Effect::new` body
without a `Timeout` wrapper. These are potential re-entrancy hazards:

### HIGH PRIORITY (likely to trigger during navigation)

1. **`components/chat_dock.rs:234-242`** -- `TargetSelector` dropdown fetch.
   `Effect::new` calls `spawn_local` directly when `show_dropdown` is true.
   Guarded by `has_loaded` flag, so it only fires when the dropdown opens.
   Risk: Low (user action triggers this, not navigation), but inconsistent with the
   pattern applied elsewhere.

2. **`pages/chat.rs:2407-2415`** -- Duplicate `TargetSelector` in the full chat page.
   Same pattern as above -- `spawn_local` directly inside `Effect::new`.

3. **`components/trace_viewer.rs:54-60`** -- `TraceViewer` loads trace data via
   `spawn_local` directly inside `Effect::new`. This Effect re-runs when
   `selected_trace_id` changes.

4. **`components/trace_viewer.rs:840-844`** -- `InlineTraceViewer` same pattern.

### LOWER PRIORITY (less likely to trigger during navigation)

5. **`signals/ui_profile.rs:31`** -- `spawn_local` called from `provide_ui_profile_context()`.
   This is called from `Shell::new`, but since Shell is now stable (ParentRoute),
   this only fires once. However, it's called synchronously during component
   construction, not inside an Effect. The risk depends on whether
   `provide_ui_profile_context()` is called during the wasm-bindgen-futures task
   queue processing.

6. **`api/sse.rs:590`** -- SSE `connect()` called from Effect. This calls
   `EventSource` JS API, not `spawn_local`, so it's not subject to the RefCell
   re-entrancy. **NOT a bug.**

---

## Broader Pattern Analysis

### Silent Error Swallowing Assessment

All `try_` variant uses are in contexts where disposal is the expected reason for
failure (leaked closures, intervals). None of them swallow genuine application errors.
**Verdict: No silent error swallowing.**

### Race Conditions from Timeout Deferral

The `Timeout::new(0, ...)` pattern defers work to the next macrotask. This creates a
window where:
- The component could be disposed between scheduling and execution.
- Mitigated by `try_set` and version counters in `use_api_resource`.
- **NOT mitigated** in `use_status_data` (see concern #8 above).

The `Timeout::new(10, ...)` in `MessageList` is cosmetic (scroll timing) and does not
create a race condition.

### ParentRoute + Outlet Correctness

The `ParentRoute` with `path!("")` and `Outlet` is the correct Leptos 0.7 pattern for
persistent layout components. This is the single most impactful fix because it
eliminates Shell re-creation on every navigation, which was the primary trigger for
the re-entrancy panic.

---

## Summary

| File | Fix | Correct? | Risk |
|------|-----|----------|------|
| `chat.rs` (Chat/ChatSession) | Timeout(0) + mounted signal | Yes | Cosmetic spinner flash |
| `shell.rs` | Outlet pattern | Yes | None |
| `taskbar.rs` | try_with_value | Yes | None |
| `topbar.rs` | try_get_untracked/try_set | Yes | None |
| `chat_dock.rs` (scroll) | Timeout(10) | Yes | None |
| `hooks/mod.rs` | Timeout(0) + version counter | Yes | None |
| `system_tray.rs` | try_set in interval | Yes | None |
| `status_center/hooks.rs` | Timeout(0) for spawn_local | Mostly | Missing try_set in spawn_local |
| `auth.rs` | Timeout(0) for spawn_local | Yes | None |
| `lib.rs` | ParentRoute restructure | Yes | None |

### Recommendations

1. **Fix `status_center/hooks.rs`**: Use `try_set` instead of `set` inside the
   `spawn_local` block in `use_status_data` (lines 160, 166), or add a version
   counter like `use_api_resource`.

2. **Fix remaining `spawn_local` in Effect bodies**: `chat_dock.rs:242`,
   `chat.rs:2415`, and `trace_viewer.rs:60,844` should wrap `spawn_local` in
   `Timeout::new(0, ...)` for consistency. These are lower risk since they're
   triggered by user actions or signal changes (not navigation), but they'll still
   panic if the Effect runs during the wasm-bindgen-futures task queue context.

3. **Once the wasm-bindgen-futures patch lands** (tasks #1/#2), all `Timeout::new(0, ...)`
   workarounds can be removed, simplifying the code significantly.
