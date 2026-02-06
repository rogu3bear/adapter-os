# Chat Queue UX Design

**Date:** 2026-02-05
**Status:** Ready for Implementation

## Problem

When inference isn't ready (no workers, model loading, etc.), the chat panel shows a warning banner that blocks users psychologically even though the input isn't disabled. Users see "Inference isn't ready" with technical jargon ("No workers running") and a passive-aggressive "Why?" button.

This is an engineer's mental model, not a user's task flow.

## Solution

**The input is never disabled.** Accept messages immediately and queue them. The wait happens gracefully in the message itself, not as a gate before the input.

## Core Behavior

### Optimistic Submit

1. User types message, hits send
2. Message appears immediately with "waiting..." indicator
3. UI polls system status until inference is ready
4. When ready, UI auto-resubmits the request
5. Response streams in, message transitions to complete

### Progressive Pending States

Messages waiting for inference move through three visual phases based on how long they've been waiting compared to historical norms:

| Phase | Trigger | Visual |
|-------|---------|--------|
| Calm | 0 → 1.5× typical wait | `◌ waiting...` (pulsing dot, muted text) |
| Informative | 1.5× → 3× typical wait | `◌ waiting for worker...` (explains blocker) |
| Estimated | > 3× typical wait | `◌ ~2 min · worker starting` (time estimate) |

**Timing source:** Adaptive based on historical response times. Start with hardcoded defaults (3s/10s/30s thresholds), add adaptive timing later.

## Message States

| State | Meaning | Visual |
|-------|---------|--------|
| `sending` | Request in flight | Spinner on send button |
| `queued` | Accepted, waiting for inference | Pulsing dot, "waiting..." |
| `streaming` | Response arriving | Typing indicator, text appearing |
| `complete` | Done | Static message, timestamp |
| `failed` | Error after retries | Red tint, retry option |

## Persistence

- Queued messages stored in `localStorage` with conversation ID
- Survives page navigation within the app
- Lost if user closes tab (acceptable for Phase 1)
- Max 5 queued messages per conversation
- Messages expire after 30 minutes with gentle timeout message

## What Gets Removed

| Element | Reason |
|---------|--------|
| "Inference isn't ready" banner | No longer blocking |
| "Why?" button | Status is in the message now |
| "Start a worker" link | Not the user's job |
| Disabled input styling | Input always works |
| "Start a conversation" placeholder | Dead text, input has its own placeholder |

## What Gets Simplified

| Element | Before | After |
|---------|--------|-------|
| "Default" button | Confusing mode selector | Show target name, hide if only one option |
| "Reasoning" tooltip | Engineer jargon | "Think step-by-step" |
| Context chips | Mystery toggles | Move to settings or remove |

## Implementation Phases

### Phase 1: UI-side queue (this PR)

- UI queues locally and retries silently
- Polls `/v1/system/status` every 5s when messages are queued
- Auto-resubmits when `inference_ready` becomes true
- No backend changes required

### Phase 2: Backend queue (future)

- `POST /v1/chat` returns `202 Accepted` with `request_id`
- Server holds request, processes when ready
- SSE notifies when request resolves
- Survives tab close

## Files to Modify

1. **chat_dock.rs** - Remove banner, add queue logic, update message rendering
2. **signals.rs** (chat signals) - Add `queued` message state, queue management
3. **inference_guidance.rs** - May be deletable or simplified
4. **components.css** - Add pending message styles (pulsing dot animation)

## Non-Goals

- No backend changes in Phase 1
- No changes to full-page /chat route (dock only for now)
- No changes to mobile overlay (follow-up PR)

## Test Plan

1. Send message when inference is ready → should work normally
2. Send message when no workers → message shows "waiting...", auto-submits when worker starts
3. Send 5 messages while queued → all queue, 6th shows limit message
4. Navigate away and back → queued messages persist
5. Wait 30+ minutes → queued message shows timeout
6. Cancel queued message → message removed from queue
