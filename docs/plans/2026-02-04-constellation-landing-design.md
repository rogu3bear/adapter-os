# Constellation Landing Page Design

**Date:** 2026-02-04
**Status:** Approved

## Overview

A spatial, chat-first landing experience that replaces the traditional dashboard as the default post-login destination. Users arrive at a calm center point in a constellation of their work, navigating through conversation rather than clicking through menus.

## Core Concept

When you log in, you arrive at **center** — a calm origin point in a spatial field. This isn't a page, it's a *place*.

**At center, you see:**
- A single input area, ready for you to type or speak
- The constellation around you — faintly visible, gently rendered
- If you have recent work, the nearest nodes glow softly nearby; if you're starting fresh, the space is open and quiet

**The constellation is your work made visible:**
- Each node is an artifact — a document, dataset, adapter, training run
- Connections between nodes are conversations that linked them
- The arrangement is organic: things you created together cluster; time flows outward from center

**You navigate by talking:**
- "Open the medical QA dataset"
- "Show me what I was working on Friday"
- "Go back to that adapter we trained"

The view drifts toward what you asked for. You're now "at" that node. The input stays with you — you can work on this artifact, talk to it, create new things from here.

## Visual Design: Calm Glass

The aesthetic is a settled variant of Liquid Glass — same language, different mood.

### The Field
- Deep, soft background — not black, not white, but a quiet depth
  - Dark mode: `hsla(222, 47%, 6%, 1)`
  - Light mode: a warm mist
- No noise texture in this view — the glass is smooth
- Subtle gradient toward the edges, drawing focus to center

### The Nodes
- Translucent cards with soft blur (`blur: 20px`, higher than the dashboard's 12px)
- Higher alpha (85-90%) — present but not demanding
- Gentle border glow instead of hard edges
- Size reflects importance or recency — recent work is slightly larger
- No pulsing — ever. A node can have a soft halo if selected, but it doesn't animate.

### The Connections
- Thin lines, nearly invisible until you focus on them
- Opacity based on strength of relationship (many shared conversations = more visible)
- They don't animate; they simply exist

### Transitions
- When you navigate, the view drifts (400-600ms ease-out, not the usual 200ms)
- Nodes ease into position like settling into water
- New nodes appear with a soft fade and gentle scale-up, then rest

### The Input
- Centered, minimal — a single-line field that expands as you type
- Glass-backed (Tier 1: 70% alpha, 9.6px blur)
- No border until focused; then a soft glow, not a hard ring

## Interaction Model: Progressive Disclosure

The interface starts radically simple and reveals capability as you grow into it.

### Level 1: Conversation Only (new users, first sessions)
- The constellation is visible but not interactive — a beautiful backdrop
- All navigation happens through the input: type what you want, the view responds
- No buttons, no menus, no chrome — just you and the space
- Keyboard accessible by default: Tab focuses input, Enter sends, Escape returns to center

### Level 2: Soft Hints (after ~3 sessions)
- Nodes gain subtle hover states — a gentle brightening
- Clicking a node navigates to it (equivalent to asking for it)
- A small "return to center" affordance appears when you're away from home (a soft home icon, bottom corner)
- Quicklinks appear as small glyphs near center — pinned workflows, recent items

### Level 3: Full Spatial (power users, opt-in)
- Drag to pan, scroll/pinch to zoom
- Drag nodes to rearrange (the system remembers your layout)
- Multi-select nodes to ask about them together
- Keyboard shortcuts: `H` for home, arrow keys to move between nearby nodes, `/` to focus input

### Accessibility Throughout
- Screen readers see a linearized view: "You are at center. Nearby: Medical QA Dataset, Training Run 47. Say or type to navigate."
- All navigation is achievable through conversation — spatial interaction is enhancement only
- Respects `prefers-reduced-motion`: transitions become instant cuts, no drift
- High contrast mode available: nodes become solid, connections become dashed lines

## Adaptive Center: Reading Your State

The center responds to who you are and where you've been.

### Fresh Start (new user, no history)
- Nearly empty — just the input, a soft welcome ("What would you like to work on?")
- The constellation is sparse or absent
- A few gentle suggestions float nearby as ghost nodes: "Upload a document," "Start a conversation," "Explore training"
- No pressure, no onboarding wizard — just an invitation

### Returning With Active Work
- Your most recent session's nodes are visible nearby, softly glowing
- The input might have context: "Continue with Medical QA?" as placeholder text
- One tap/Enter and you're back where you were
- The constellation is populated but not overwhelming — distant work fades to near-invisible

### Returning After Time Away
- A gentle reorientation: "It's been a while. Here's what you left open."
- Recent nodes are surfaced, but slightly more faded — acknowledging time has passed
- Any system events (training completed, errors occurred) appear as soft notification nodes near center, not as pop-ups

### Different Times of Day (optional, respects system theme)
- Morning: slightly warmer color temperature, if light mode
- Evening: cooler, darker, easier on eyes
- This is subtle — a 5% shift, not a dramatic transformation

### Quicklinks
- Appear as small, unlabeled glyphs arranged in a loose arc below the input
- Reveal labels on hover/focus
- Configurable: pin your most-used workflows
- Default set: "New chat," "Recent," "Datasets," "Adapters" — but the user can change this

## Integration & Technical Shape

### Routing
- New route: `/home` — the constellation landing
- `/` redirects to `/home` (currently goes to `/dashboard`)
- `/dashboard` remains as the dense ops view — accessible from quicklinks or by asking "show me the dashboard"
- The Shell wraps this page, but with a flag to hide the taskbar (the constellation *is* the navigation)

### State Model
- `ConstellationState`: nodes, connections, camera position, user's current location
- Nodes are lazy-loaded — only nearby nodes fetch full data; distant nodes are stubs (id, title, type)
- Connections derived from conversation history: "these two artifacts were discussed in the same session"
- Persisted per-user in the database; camera position and pinned quicklinks saved

### Chat Integration
- The existing chat system becomes the engine; this is a new *view* of it
- Each conversation is anchored to a location in the constellation
- The chat dock (currently a right panel) transforms into the centered input
- Chat history is accessible by navigating to past nodes, not by scrolling a sidebar

### Loading States
- Initial load: the center appears immediately (static, no API call), constellation fades in as data arrives
- Navigation: the view begins drifting immediately (optimistic), node details load as you arrive
- Errors: a soft red glow on center, a calm message in the input area — not a toast, not a modal

### New Components Needed
- `Constellation` — the spatial canvas (SVG or Canvas-based)
- `ConstellationNode` — individual artifact representation
- `ConstellationConnection` — relationship lines
- `CenterInput` — the adaptive chat input at home
- `GhostNode` — suggestion nodes for empty states

### CSS Additions
- New tier: `--glass-bg-calm` — higher translucency, softer blur
- Transition presets: `--transition-drift` (500ms ease-out)
- Reduced motion overrides for all constellation animations

## Design Principles

1. **Spatial, not temporal** — conversation creates objects in space, not messages in a scroll
2. **Conversation is navigation** — you move by asking, not by clicking
3. **Progressive disclosure** — radical simplicity first, power emerges with use
4. **Calm by default** — no infinite animations, no pulsing, things rest when you rest
5. **Accessible at core** — the conversation-first model works for everyone; spatial interaction is enhancement
