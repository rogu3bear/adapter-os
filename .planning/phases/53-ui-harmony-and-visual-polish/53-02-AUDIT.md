# Phase 53 Plan 02: Chat Workspace and Dashboard Audit Report

**Date:** 2026-03-05
**Auditor:** Claude (automated)
**Scope:** Chat workspace (all sub-components) + Dashboard

---

## Methodology

For each surface, every visible element was enumerated by reading the Rust component source. Elements were classified as:
- **Essential**: Connects to a working feature, actively serves the operator
- **Redundant**: Duplicate info shown elsewhere in the same view
- **Dead**: onClick does nothing, links to unimplemented feature, or is a placeholder
- **Orphaned**: References removed feature or shows stale data

Before classifying any element as dead/orphaned:
- Test-contracted IDs were searched across `tests/playwright/` (many are actively used)
- Command palette route references were checked (none reference chat/dashboard directly)
- Signal/callback wiring from parent components was verified

---

## Surface 1: Chat Workspace

### 1A. Chat Landing Page (`workspace.rs` - `Chat` component, route `/chat`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | PageScaffold with "Chat" title | Essential | Keep | Standard page wrapper |
| 2 | Breadcrumb: Chat > Quick Start | Essential | Keep | Navigation context |
| 3 | Status badge (Ready/Blocked/Unavailable/Checking) | Essential | Keep | Inference readiness feedback |
| 4 | `ChatQuickStartCard` (when ready) | Essential | Keep | Primary quickstart flow |
| 5 | `ChatUnavailableEntry` (when blocked) | Essential | Keep | Guides user to resolve blockers, test-contracted IDs |
| 6 | Loading spinner placeholder | Essential | Keep | Loading state, `data-testid="chat-loading-state"` in Playwright |
| 7 | Query param `?adapter=` handling | Essential | Keep | Deep-link adapter pinning |

**Verdict: Chat landing page is clean.** No dead/orphaned/redundant elements found. The quick-start flow is minimal-click: type prompt, hit Enter.

### 1B. Chat Session Page (`workspace.rs` - `ChatSession` component, route `/chat/s/:id` and `/chat/history`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | PageScaffold with dynamic title | Essential | Keep | Session vs History title |
| 2 | Status badge (Session/History) | Essential | Keep | Route context |
| 3 | Deferred mounting with `<Show>` | Essential | Keep | Avoids wbf re-entrancy panic |
| 4 | `ChatWorkspace` component | Essential | Keep | Core workspace layout |

### 1C. ChatWorkspace (`workspace.rs` - private component)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | `ChatWorkspaceLayout` wrapper | Essential | Keep | Two-column layout |
| 2 | Desktop session list sidebar (`SessionListPanel`) | Essential | Keep | History navigation |
| 3 | Mobile back-nav breadcrumb (History) | Essential | Keep | Mobile nav, back to /chat |
| 4 | Mobile back-nav breadcrumb (Session Detail) | Essential | Keep | Back to history |
| 5 | Mobile sessions toggle button | Essential | Keep | Opens slide-out sidebar |
| 6 | `ChatConversationPanel` (selected) | Essential | Keep | Core conversation |
| 7 | `ChatEmptyWorkspace` (no selection) | Essential | Keep | Empty state |
| 8 | Mobile session overlay (slide-in) | Essential | Keep | Mobile session picker |
| 9 | Backdrop blur overlay for mobile | Essential | Keep | Dismisses mobile panel |

**Verdict: ChatWorkspace layout is clean.** `<Show>` pattern used correctly. No dead controls.

### 1D. Session List Panel (`session_list.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | Header "Conversations" label | Essential | Keep | Section label |
| 2 | "New Conversation" button | Essential | Keep | `data-testid="chat-sidebar-new-session"` in Playwright |
| 3 | Active/Archived toggle tabs | Essential | Keep | Tab controls with counts |
| 4 | "Create Adapter from Selection" button | Essential | Keep | Training integration, `data-testid="chat-sidebar-learn"` |
| 5 | Selection count + "Clear" link | Essential | Keep | Training selection UX |
| 6 | Training dataset error message | Essential | Keep | Error feedback |
| 7 | Search input | Essential | Keep | `data-testid="chat-sidebar-search"` |
| 8 | "Continue this draft" banner | Essential | Keep | Dock draft recovery, `data-testid="chat-sidebar-continue"` |
| 9 | Session list with `SessionListItem` | Essential | Keep | Core navigation |
| 10 | Empty state (no conversations/no match/no archived) | Essential | Keep | Contextual empty states |
| 11 | Delete confirmation dialog | Essential | Keep | Destructive action guard |

**Sidebar issue: Density.** The sidebar header has too many stacked controls: title row + Active/Archived toggle + Create Adapter button + selection count + search. This is 5 distinct control rows before any session appears. The "Create Adapter from Selection" area takes ~60px of vertical space even when no sessions are selected.

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| S1 | "Create Adapter from Selection" button + hint text + clear link | **Redundant density** | **Fix**: collapse into a single row that appears only when 1+ sessions are selected | Saves ~60px vertical when not in use |

### 1E. Composer Panel (`composer.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | Active config line (model, adapters, RAG, verify mode) | Essential | Keep | Context awareness |
| 2 | Mobile config toggle button | Essential | Keep | Collapses details on mobile |
| 3 | "Attach data" button | Essential | Keep | `data-testid="chat-attach"` |
| 4 | Textarea (message input) | Essential | Keep | `data-testid="chat-input"` in many Playwright tests |
| 5 | Send button (submit) | Essential | Keep | `data-testid="chat-send"` in many Playwright tests |
| 6 | Stop button (during streaming) | Essential | Keep | `data-testid="chat-stop"` |

**Issue: Missing hover/active states on Attach button.** Uses `ButtonVariant::Outline` which has hover via component library, so actually OK. Send button uses default variant with `loading` and `disabled` props -- OK.

**Verdict: Composer is clean and minimal.** No redundant elements.

### 1F. Conversation Panel (`conversation.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | Session header with Session ID label | Essential | Keep | `data-testid="chat-header"` |
| 2 | Target selector (inline, in header) | **Redundant** | **Fix**: Appears TWICE - once in header, once in Context drawer | See R1 below |
| 3 | Base model badge (in header) | **Redundant** | **Fix**: Duplicates target selector info | See R2 below |
| 4 | Mode toggle (Best-Effort / Bit-Identical) | Essential | Keep | Core trust mode control |
| 5 | Status badge (Ready/Streaming/Error/etc.) | Essential | Keep | `data-testid="chat-status-badge"` |
| 6 | Context strip (Model/Adapter/Mode badges) | **Redundant** | **Fix**: Duplicates info from header controls | See R3 below |
| 7 | Stream status / Paused banner | Essential | Keep | `data-testid="chat-stream-status"` |
| 8 | Adapter selection pending badge | Essential | Keep | Feedback during pin changes |
| 9 | Message log (scrollable, keyed `<For>`) | Essential | Keep | Core conversation display |
| 10 | Empty conversation state | Essential | Keep | `data-testid="chat-conversation-empty"` |
| 11 | "Start Chat" button in empty state | Essential | Keep | Focuses input, `data-testid="chat-conversation-start-chat"` |
| 12 | "Add Files" button in empty state | Essential | Keep | Opens attach dialog, `data-testid="chat-conversation-add-files"` |
| 13 | "Browse Adapters (Library)" link | Essential | Keep | Links to /adapters |
| 14 | Message bubbles (user/assistant/system) | Essential | Keep | Core chat rendering |
| 15 | Execution Record / Receipt / Replay links per message | Essential | Keep | All test-contracted IDs in Playwright |
| 16 | Token count display | Essential | Keep | Inference cost transparency |
| 17 | Trust panel (`<details>` per message) | Essential | Keep | `data-testid="chat-trust-panel"` |
| 18 | Critical meaning-change alert per message | Essential | Keep | `data-testid="chat-meaning-change-alert"` |
| 19 | Inline error indicator (after messages) | Essential | Keep | `data-testid="chat-inline-error"` |
| 20 | Error banner with retry/dismiss | Essential | Keep | `data-testid="chat-error-banner"` |
| 21 | Session confirmation banners (pending/not-found/transient) | Essential | Keep | All test-contracted IDs |
| 22 | Session inline notice | Essential | Keep | `data-testid="chat-session-inline-notice"` |
| 23 | Inference readiness banner | Essential | Keep | Shows when inference not ready |
| 24 | "Jump to latest" scroll button | Essential | Keep | `data-testid="chat-jump-to-latest"` |
| 25 | Trace panel (modal overlay) | Essential | Keep | Opens on trace link click |
| 26 | Context overflow notice | Essential | Keep | `data-testid="chat-overflow-notice"` |
| 27 | Mobile lane toggle (Conversation/Evidence/Context) | Essential | Keep | Mobile segmented control |
| 28 | Desktop drawer rail (Evidence/Context buttons) | Essential | Keep | Desktop side panel toggle |
| 29 | Evidence drawer panel | Essential | Keep | `data-testid="chat-drawer-evidence"` |
| 30 | Context drawer panel | Essential | Keep | `data-testid="chat-drawer-context"` |
| 31 | Attach dialog | Essential | Keep | File upload, paste, chat-to-dataset |

#### Redundancies Found

**R1: Duplicate Target Selector.** `ChatTargetSelector` appears twice:
- In the conversation header (line 2499-2501): `chat-header-target`
- In the Context drawer panel (line 2457): `chat-context-target`

Both are identical `inline=true` instances. The header one is visible at all times; the Context drawer one appears when the Evidence/Context panel is open. On desktop, both are simultaneously visible.

**Proposed action:** Remove the header target selector. The Context drawer is the proper home for configuration controls. The mode toggle and status badge in the header provide enough at-a-glance context.

**R2: Base model badge redundancy.** The header shows `Badge variant=Outline` with "Base model: {name}" (line 2502-2509). This duplicates the target selector label which already shows the selected model. When target=Auto, the badge says "Base model: Auto" and the selector also says "Auto".

**Proposed action:** Remove the base model badge from the header. The model info is visible in the Context strip and the target selector.

**R3: Context strip redundancy.** The context strip (line 2640-2671) shows Model/Adapter/Mode badges. The header already shows mode toggle + status badge, and the Context drawer shows the same Model/Adapter badges. On desktop, all three surfaces are visible simultaneously, creating triple redundancy for the model label.

**Proposed action:** Keep the context strip but simplify -- it provides useful at-a-glance info in the conversation flow. However, removing the header target selector and base model badge (R1+R2) eliminates the triple redundancy. The context strip + Context drawer is two sources, which is acceptable (strip for at-a-glance, drawer for editing).

### 1G. Target Selector (`target_selector.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | Dropdown trigger button | Essential | Keep | `data-testid="chat-target-selector"` |
| 2 | Dropdown overlay (models, policies) | Essential | Keep | `data-testid="chat-target-dropdown"` |
| 3 | "Auto" default option | Essential | Keep | Default target |
| 4 | Model list (sorted, with quantization/backend labels) | Essential | Keep | Model selection |
| 5 | Policy Pack list | Essential | Keep | Policy selection |
| 6 | Loading state | Essential | Keep | Lazy load on dropdown open |
| 7 | Error state | Essential | Keep | API error feedback |

**Verdict: Target selector is clean.** No dead elements. Lazy-loads on open, which is good.

### 1H. Status Banners (`status_banners.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | Stream status badge (ttft, slow, normal) | Essential | Keep | `data-testid="chat-stream-status"` |
| 2 | Warning alert banner (for slow streams) | Essential | Keep | Uses `AlertBanner` component |
| 3 | Paused notice | Essential | Keep | `data-testid="chat-paused-notice"` |

**Verdict: Status banners are clean.** No dead elements. Good use of variants.

---

## Surface 2: Dashboard

### Dashboard (`dashboard.rs`)

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| 1 | PageScaffold with "Home" title | Essential | Keep | Landing page |
| 2 | Subtitle text | Essential | Keep | Brief description |
| 3 | SSE status indicator | Essential | Keep | Live connection status |
| 4 | "Start Chat" primary action button | Essential | Keep | Primary CTA |
| 5 | "Refresh" secondary action button | Essential | Keep | Manual data refresh |
| 6 | "View System" outline button | **Redundant** | **Fix**: Appears in both PageScaffoldActions AND in the services bar | See D1 below |
| 7 | Loading skeleton (2 cards + stats grid + 2 bars) | Essential | Keep | Skeleton matches layout |
| 8 | Error display with retry | Essential | Keep | Standard error pattern |
| 9 | `DashboardContent` (loaded) | Essential | Keep | Main content |

#### DashboardContent

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| D1 | `JourneyFlowSection` (Quick Start card) | Essential but verbose | **Fix**: See below | |
| D2 | System Status card | Essential | Keep | Shows readiness status with icon |
| D3 | Chat card (inference readiness) | Essential | Keep | Shows prompt readiness |
| D4 | Inference guidance (when blocked) | Essential | Keep | Action buttons for resolution |
| D5 | "Why?" button (opens status center) | Essential | Keep | Status center shortcut |
| D6 | Services bar (DB status + "View System" link) | Essential | Keep, but see D1 redundancy | |

#### JourneyFlowSection Details

| # | Element | Classification | Proposed Action | Notes |
|---|---------|---------------|-----------------|-------|
| J1 | Card title "Quick Start" | Essential | Keep | |
| J2 | Description text ("Use AdapterOS as a chat...") | Essential | Keep | One-liner value prop |
| J3 | Second description text ("Pick one action...") | **Redundant** | **Remove**: Obvious from the card layout | Unnecessary coaching text |
| J4 | "Advanced workflow" `<details>` collapsible | **Low value** | **Remove**: Vague text about "resolve a version in Versions, run checkout or promote" -- this doesn't connect to any clear UI action | References unclear workflow |
| J5 | Action 1: Start Chat step | Essential | Keep | Primary CTA |
| J6 | "Start here" badge on Action 1 | **Redundant** | **Remove**: Already the first item in the list; the visual hierarchy makes it obvious | Unnecessary annotation |
| J7 | Action 2: Create Adapter step | Essential | Keep | Secondary CTA |
| J8 | Action 3: View Evidence step | Essential | Keep | Tertiary CTA |
| J9 | "Current Configuration Fingerprint..." footer text | **Dead** | **Remove**: References a "top bar" fingerprint that does not exist as a visible element. No fingerprint badge appears in the navigation or header. | Stale reference |
| J10 | "Track every event in Event Viewer" footer text | **Low value** | **Remove**: The /audit link is accessible via navigation; this footnote adds clutter | Duplicate navigation |

**D1: "View System" redundancy.** Two "View System" links visible simultaneously:
- In `PageScaffoldActions` (header area): `ButtonLink href="/system"`
- In the services bar at bottom: `<a href="/system">`

**Proposed action:** Remove the "View System" from `PageScaffoldActions`. The services bar at the bottom is more contextual (next to the DB status indicator). Keep the "Refresh" button in the header actions -- it's useful there.

---

## Glass Tier Audit

| Surface | Current Tier | Expected Tier | Status |
|---------|-------------|--------------|--------|
| Session list sidebar panel | Uses `ChatSessionListShell` wrapper (CSS `chat-session-sidebar`) | Tier 2 (panel) | **Check CSS** -- the CSS has `background: var(--glass-bg-2)` for the sidebar at line 754. Correct. |
| Chat conversation area | `bg-card` class on message log | Tier 1 (content) | Uses standard card background. CSS has `glass-bg-1` for cards. Correct. |
| Composer input area | No glass tier applied directly | Tier 1 (content) | Uses `border-t border-border` -- content-level. OK. |
| Mobile overlay (session list) | `bg-background/80 backdrop-blur-sm` for backdrop, `bg-background` for panel | Tier 3 (overlay) | Should use `var(--glass-bg-3)` for the panel, but currently uses `bg-background`. **Fix needed.** |
| Chat drawer panels | CSS `.chat-drawer-panel` | Tier 1 | CSS shows glass-bg-1 for drawer content. OK. |
| Dashboard cards | Uses `<Card>` component | Tier 1 (content) | Cards use `bg-card/60` with `glass-bg-1`. Correct. |
| Dashboard Quick Start card | Uses `<Card>` component | Tier 1 | Same as above. Correct. |

**Glass tier fix needed:** Mobile session overlay panel should use `glass-bg-3` backdrop instead of plain `bg-background`.

---

## Spacing Audit

| Location | Issue | Action |
|----------|-------|--------|
| Conversation panel outer padding | `p-4` (hardcoded) | **Fix**: Use `var(--space-4)` via CSS class |
| Message gap in conversation | `space-y-5` (hardcoded) | **Keep**: utility class, consistent with design tokens |
| Session list header | `p-3 space-y-2` | **Keep**: utility classes |
| Composer border-top | `border-t border-border pt-4` | **Keep**: utility pattern |
| Dashboard grid gaps | `gap-3`, `gap-4` | **Keep**: utility classes |

Spacing is generally clean -- Tailwind-style utility classes are used consistently. No raw `px` values in component code. The CSS files use design token variables appropriately.

---

## Interaction State Audit

| Element | Hover | Active | Focus-visible | Status |
|---------|-------|--------|---------------|--------|
| Composer Send button | Via `<Button>` component | Via `<Button>` | Via `<Button>` | OK |
| Composer Stop button | Inline class `hover:bg-destructive/90` | None explicit | None explicit | **Fix**: Add focus-visible ring |
| Composer Attach button | Via `<Button>` component | Via `<Button>` | Via `<Button>` | OK |
| Session list item | CSS `.chat-session-row:hover` | None explicit | None explicit | **Fix**: Add active state + focus-visible |
| Session list "New Conversation" | `hover:bg-primary/90` | None explicit | None explicit | **Fix**: Add active + focus-visible |
| Target selector dropdown items | `hover:bg-accent` | None explicit | None explicit | **Fix**: Add focus-visible for keyboard nav |
| Dashboard "Start Chat" button | Via `<ButtonLink>` | Via `<ButtonLink>` | Via `<ButtonLink>` | OK |
| Dashboard Journey step CTAs | Via `<ButtonLink>` | Via `<ButtonLink>` | Via `<ButtonLink>` | OK |
| Dashboard "Refresh" button | Via `<Button>` | Via `<Button>` | Via `<Button>` | OK |
| Dashboard services bar "View System" | `hover:underline` | None | None | OK (link pattern) |
| Mode toggle buttons (Best-Effort/Verified) | Via `btn btn-ghost` classes | None explicit | None explicit | **Fix**: Add focus-visible ring |
| Drawer rail buttons (Evidence/Context) | Via `btn btn-ghost` | None explicit | None explicit | **Fix**: Already has shadow-sm on active, needs focus-visible |
| Mobile lane toggle buttons | Via `btn btn-ghost` | None explicit | None explicit | **Fix**: Needs focus-visible ring |
| "Jump to latest" button | `hover:bg-muted/80` | None explicit | None explicit | OK (minor element) |
| Error retry/dismiss buttons | Via `<Button>`/btn classes | Via btn classes | Via btn classes | OK |

---

## Squint Test Results

**Chat Workspace:** PASS with minor issue. When squinted:
- The header bar is visually dense with 5+ controls (session ID, target selector, model badge, mode toggle, status badge). After removing R1 (target selector) and R2 (model badge), the hierarchy becomes: session ID (left) + mode toggle + status (right) -- much cleaner.
- Conversation area has clear bubble hierarchy (user right-aligned, assistant left-aligned).
- Composer is clearly bottom-anchored.
- Sidebar is clearly a secondary panel.

**Dashboard:** PASS with minor issue. When squinted:
- Quick Start card dominates appropriately as the hero.
- System Status and Chat cards are equal siblings -- correct.
- Services bar is a subtle footer element -- correct.
- The "Pick one action" text and "Advanced workflow" details add visual noise between the hero text and the action cards.

---

## Summary: Proposed Cut List

### Removals

| ID | Surface | Element | Justification | Test Impact |
|----|---------|---------|---------------|-------------|
| CUT-1 | Chat header | Target selector (`chat-header-target`) | Duplicate of Context drawer target selector | None -- `data-testid="chat-target-selector"` remains in Context drawer |
| CUT-2 | Chat header | Base model badge (`chat-header-base-model`) | Duplicates target selector info; model shown in context strip | None -- no test-contracted IDs |
| CUT-3 | Dashboard | "View System" in PageScaffoldActions | Duplicate of services bar link | None -- no test-contracted IDs |
| CUT-4 | Dashboard | "Pick one action..." description text | Obvious from layout; unnecessary coaching | None |
| CUT-5 | Dashboard | "Advanced workflow" `<details>` | Vague text referencing unclear workflow | None |
| CUT-6 | Dashboard | "Start here" badge on Action 1 | Obvious from visual hierarchy | None |
| CUT-7 | Dashboard | "Current Configuration Fingerprint..." footer | Dead reference -- no fingerprint badge exists in top bar | None |
| CUT-8 | Dashboard | "Track every event in Event Viewer" footer | Duplicate of navigation; clutter | None |

### Fixes

| ID | Surface | Element | Issue | Proposed Fix |
|----|---------|---------|-------|-------------|
| FIX-1 | Chat sidebar | "Create Adapter" training controls | Takes ~60px when unused | Collapse into single-row contextual button that appears only when 1+ sessions selected |
| FIX-2 | Chat mobile overlay | Session list panel background | Uses `bg-background` instead of glass tier 3 | Apply `glass-bg-3` background |
| FIX-3 | Chat conversation | Session list items | Missing active + focus-visible states | Add `active:` and `focus-visible:` CSS |
| FIX-4 | Chat conversation | Mode toggle buttons | Missing focus-visible ring | Add `focus-visible:ring-2` |
| FIX-5 | Chat conversation | Drawer rail buttons | Missing focus-visible ring | Add `focus-visible:ring-2` |
| FIX-6 | Chat conversation | Mobile lane toggle buttons | Missing focus-visible ring | Add `focus-visible:ring-2` |
| FIX-7 | Chat conversation | Target dropdown items | Missing focus-visible for keyboard nav | Add `focus-visible:bg-accent` |

### Preserved (not touched)

All test-contracted IDs (`chat-input`, `chat-send`, `chat-loading-state`, `chat-empty-state`, `chat-unavailable-state`, `chat-unavailable-reason`, `chat-unavailable-action`, `chat-run-link`, `chat-receipt-link`, `chat-replay-link`, `chat-adapter-chips`, `chat-citation-chips`, `chat-trace-links`, `chat-header`, `chat-conversation-empty`, `chat-stream-status`, `chat-session-state-pending`, `chat-session-state-not-found`, `chat-session-state-transient`, `chat-session-confirm-retry`) remain untouched.

No SSE event handling, streaming inference logic, signal types, or API call shapes are modified.

All `aria-label`, `role`, and accessibility attributes are preserved.
