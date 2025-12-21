# Workbench

The Workbench is the unified chat hub at `/chat`. It provides a three-column layout for managing chat sessions, datasets, adapter stacks, and viewing inference traces.

## Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ Top Bar: [Stream Toggle] [Developer Toggle] [Dataset] [Stack] [Export]│
├──────────┬───────────────────────────────────────────────────────────┤
│ Left Rail│       Center (ChatInterface)      │     Right Rail        │
│  320px   │            flex-1                 │   384px (collapsible) │
│          │                                   │                       │
│ [Tabs]   │                                   │   [Pin] [Collapse]    │
│ Sessions │        Chat messages              │   TraceSummaryPanel   │
│ Datasets │        Input area                 │   EvidencePanel       │
│ Stacks   │                                   │                       │
│          │                                   │                       │
│ ───────  │                                   │                       │
│ [Detach] │                                   │                       │
│ [Reset]  │                                   │                       │
└──────────┴───────────────────────────────────┴───────────────────────┘
```

## Features

### Left Rail Tabs

**Sessions Tab**
- Lists recent chat sessions
- Search/filter sessions
- Create new session
- Delete sessions

**Datasets Tab**
- Lists available datasets
- Select a dataset to scope chat context
- Clear active dataset
- Shows row counts and validation status

**Stacks Tab**
- Lists adapter stacks
- Click to activate a stack
- Shows adapter counts
- **Detach All** button - Deactivates current stack with 10s undo
- **Reset Default** button - Activates tenant's default stack

### Right Rail

- **TraceSummaryPanel** - Displays trace metadata (digests, backend, kernel, tokens)
- **EvidencePanel** - Shows evidence items for the trace
- **Pin** - Stops auto-update when viewing a specific trace
- **Collapse** - Hides the right rail

### Top Bar

- **Stream Mode Toggle** - Switch between token and chunk streaming modes
- **Developer Mode Toggle** - Shows raw JSON traces
- **Active Dataset Chip** - Shows selected dataset, click X to clear
- **Active Stack Chip** - Shows selected stack name

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Esc` | First press: Collapse right rail. Second press: Focus chat input |
| `Enter` | Send message |
| `Shift+Enter` | New line in message |

## State Persistence

The following UI state persists in localStorage:

| Key | Description |
|-----|-------------|
| `workbench:leftRail:activeTab` | Currently selected tab (sessions/datasets/stacks) |
| `workbench:leftRail:scrollPositions` | Scroll position for each tab |
| `workbench:rightRail:collapsed` | Whether right rail is collapsed |

## Component Architecture

```
ChatPage.tsx
├── WorkbenchProvider (context)
│   └── DatasetChatProvider (context)
│       └── WorkbenchLayout
│           ├── TopBar
│           │   ├── Stream/Developer toggles
│           │   └── WorkbenchTopBar (chips)
│           ├── LeftRail
│           │   ├── LeftRailTabs
│           │   ├── SessionsTab
│           │   ├── DatasetsTab
│           │   └── StacksTab
│           │       ├── DetachAllButton
│           │       └── ResetDefaultButton
│           ├── Center
│           │   └── ChatInterface
│           └── RightRail
│               ├── RightRailHeader (pin/collapse)
│               ├── TraceSummaryPanel
│               └── EvidencePanel
```

## Data Flow

### Message Selection → Trace Display

When a user clicks a message or a new message completes:

1. `ChatMessage.handleClick` or `onStreamComplete` extracts `traceId`
2. Calls `onMessageSelect(messageId, traceId)` or `onMessageComplete(messageId, traceId)`
3. `ChatPage` updates `selectedMessageId` (for highlighting) and `selectedTraceId` (for fetching)
4. `useTrace(selectedTraceId)` fetches the trace data
5. Right rail displays `TraceSummaryPanel` and `EvidencePanel`

### Pin Behavior

When pinned:
- `pinnedMessageId` is set in WorkbenchContext
- Auto-update on message completion is disabled
- User can still click other messages to view their traces
- Click pin again to unpin and resume auto-update

### Detach All Flow

1. User clicks "Detach All"
2. Current stack ID and adapter strength overrides are captured
3. `useDeactivateAdapterStack().mutateAsync()` is called
4. Adapter strength overrides are cleared
5. `UndoSnackbar` appears with 10s countdown
6. User can click "Undo" to restore the previous stack AND strength overrides
7. After 10s, undo action expires

### Trace ID Flow

The server's `request_id` (trace ID) flows through streaming:
1. Server sends `StreamingChunk` with `id` field containing the request_id
2. `apiClient.streamInfer` captures the first chunk's `id`
3. `onComplete` metadata includes `request_id`
4. `useStreamingInference` sets `response.id` to the server's request_id
5. `ChatInterface.onStreamComplete` stores `response.id` as `message.traceId`
6. Clicking a message passes `traceId` to fetch the full trace for the right rail

## Test IDs

All interactive elements have `data-testid` attributes for E2E testing:

| Component | Test ID |
|-----------|---------|
| Left rail container | `left-rail` |
| Tab buttons | `tab-sessions`, `tab-datasets`, `tab-stacks` |
| Sessions tab | `sessions-tab` |
| Datasets tab | `datasets-tab` |
| Stacks tab | `stacks-tab` |
| Right rail | `right-rail` |
| Pin button | `pin-toggle-button` |
| Collapse button | `collapse-toggle-button` |
| Detach All | `detach-all-button` |
| Reset Default | `reset-default-button` |
| Undo snackbar | `undo-snackbar` |
| Chat input | `chat-input` |

## Usage

```tsx
// ChatPage.tsx uses the workbench layout
import { WorkbenchProvider } from '@/contexts/WorkbenchContext';
import { WorkbenchLayout } from '@/components/workbench';

function ChatPage() {
  return (
    <WorkbenchProvider>
      <WorkbenchLayout
        topBar={<TopBar />}
        leftRail={<LeftRail />}
        center={<ChatInterface />}
        rightRail={<RightRail />}
      />
    </WorkbenchProvider>
  );
}
```

## Context API

```tsx
import { useWorkbench } from '@/contexts/WorkbenchContext';

function MyComponent() {
  const {
    // Left rail
    activeLeftTab,
    setActiveLeftTab,

    // Right rail
    rightRailCollapsed,
    setRightRailCollapsed,
    toggleRightRail,

    // Message selection
    selectedMessageId,
    selectMessage,
    pinnedMessageId,
    pinMessage,

    // Adapter strength overrides (for undo support)
    strengthOverrides,
    setStrengthOverrides,
    updateStrengthOverride,
    clearStrengthOverrides,

    // Undo
    undoAction,
    setUndoAction,
    clearUndoAction,

    // Keyboard
    handleGlobalEscape,
  } = useWorkbench();
}
```
