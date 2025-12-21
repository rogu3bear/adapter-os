# ChatInterface.tsx Refactoring Guide

## Status: In Progress (WORKSTREAM 1)

This document outlines the surgical refactoring of `ChatInterface.tsx` from 1,580 lines to <800 lines by extracting state management into dedicated hooks.

## Completed Work

### 1. Created New Hooks ✅

#### `/ui/src/hooks/chat/useSessionManager.ts`
- **Purpose**: Consolidates session state (currentSessionId, messages, editing state)
- **Pattern**: useReducer for predictable state transitions
- **Race Condition Fixes**:
  - ✅ AbortController for session creation (single-flight deduplication)
  - ✅ Proper cleanup on unmount
- **API**:
  ```typescript
  const sessionManager = useSessionManager({
    tenantId,
    sessionSourceType,
    documentContext,
  });

  // State
  sessionManager.currentSessionId
  sessionManager.messages
  sessionManager.editing

  // Actions
  sessionManager.loadSession(id)
  sessionManager.createSession(...) // Has AbortController
  sessionManager.updateSession(id, updates)
  sessionManager.deleteSession(id)
  sessionManager.clearSession()
  sessionManager.setMessages(messages)
  sessionManager.addMessage(message)
  sessionManager.startEditing(id, name)
  sessionManager.finishEditing()
  ```

#### `/ui/src/hooks/chat/useChatModals.ts`
- **Purpose**: Unified modal state management
- **Pattern**: Single state object with type-safe modal tracking
- **Consolidates**:
  - isHistoryOpen → `modals.isHistoryOpen`
  - isRouterActivityOpen → `modals.isRouterActivityOpen`
  - isArchivePanelOpen → `modals.isArchivePanelOpen`
  - shareDialogSessionId → `modals.shareDialogSessionId`
  - tagsDialogSessionId → `modals.tagsDialogSessionId`
- **API**:
  ```typescript
  const modals = useChatModals();

  modals.openModal('history');
  modals.openModal('share', { sessionId: 'xyz' });
  modals.closeModal();
  modals.isOpen('history'); // boolean
  modals.getModalData(); // { sessionId?: string } | null

  // Backward-compatible getters
  modals.isHistoryOpen
  modals.shareDialogSessionId

  // Backward-compatible setters
  modals.setIsHistoryOpen(true)
  modals.setShareDialogSessionId('xyz')
  ```

### 2. Updated Export Index ✅
- Added exports to `/ui/src/hooks/chat/index.ts`

## Remaining Work

### 3. ChatInterface.tsx Refactoring Required

The file is currently being modified by a linter/formatter, so direct edits are challenging. Here are the surgical changes needed:

#### A. Fix Duplicate Declarations (Lines 94-165)

**CURRENT PROBLEM**:
```typescript
// Line 94-96
const tenantId = selectedTenant || 'default';
const sessionSourceType = documentContext ? 'document' : 'general';

// ... session manager hook ...

// Line 127-133 - DUPLICATES!
const tenantId = selectedTenant || 'default'; // ❌ DUPLICATE
const sessionSourceType = documentContext ? 'document' : 'general'; // ❌ DUPLICATE
```

**FIX**: Remove lines 127-133 (duplicates)

#### B. Replace Old State with Hook Usage

**OLD** (Lines 92-100, 107-109):
```typescript
const [messages, setMessages] = useState<ChatMessage[]>([]);
const [currentSessionId, setCurrentSessionId] = useState<string | null>(sessionId || null);
const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
const [newSessionName, setNewSessionName] = useState('');
const [isHistoryOpen, setIsHistoryOpen] = useState(false);
const [isRouterActivityOpen, setIsRouterActivityOpen] = useState(false);
const [isArchivePanelOpen, setIsArchivePanelOpen] = useState(false);
const [shareDialogSessionId, setShareDialogSessionId] = useState<string | null>(null);
const [tagsDialogSessionId, setTagsDialogSessionId] = useState<string | null>(null);
```

**NEW** (Already done at lines 98-106):
```typescript
const sessionManager = useSessionManager({ tenantId, sessionSourceType, documentContext });
const modals = useChatModals();
```

#### C. Update useChatSessionsApi Call (Lines 134-150)

**CURRENT**:
```typescript
const {
  sessions,
  isLoading: isLoadingSessions,
  isUnsupported: isChatHistoryUnsupported,
  unsupportedReason: chatHistoryUnsupportedReason,
  createSession,  // ❌ Remove - use sessionManager.createSession
  updateSession,  // ❌ Remove - use sessionManager.updateSession
  addMessage,     // ✅ Keep - needed for optimistic updates
  deleteSession,  // ❌ Remove - use sessionManager.deleteSession
  getSession,     // ❌ Remove - use sessionManager.loadSession
  updateSessionCollection, // ✅ Keep - collection updates
} = useChatSessionsApi(tenantId, {
  sourceType: sessionSourceType,
  documentId: documentContext?.documentId,
  documentName: documentContext?.documentName,
  collectionId: documentContext?.collectionId ?? null,
});
```

**CHANGE TO**:
```typescript
const {
  sessions,
  isLoading: isLoadingSessions,
  isUnsupported: isChatHistoryUnsupported,
  unsupportedReason: chatHistoryUnsupportedReason,
  addMessage, // Keep for API sync
  updateSessionCollection, // Keep for collection updates
} = useChatSessionsApi(tenantId, {
  sourceType: sessionSourceType,
  documentId: documentContext?.documentId,
  documentName: documentContext?.documentName,
  collectionId: documentContext?.collectionId ?? null,
});
```

#### D. Fix Race Condition #1: Session Creation (Lines 491-538)

**CURRENT ISSUE**: Lines 508-524 use async IIFE without abort control

**FIX**: Replace with:
```typescript
useEffect(() => {
  if (sessionId && sessionId !== sessionManager.currentSessionId) {
    sessionManager.loadSession(sessionId);
  } else if (!sessionManager.currentSessionId && selectedStackId && !isLoadingSessions) {
    const stack = stacks.find(s => s.id === selectedStackId);
    if (stack) {
      const documentCtx = documentContext
        ? { documentId: documentContext.documentId, documentName: documentContext.documentName }
        : undefined;

      // sessionManager.createSession already has AbortController
      sessionManager.createSession(
        `Chat with ${stack.name || 'Stack'}`,
        selectedStackId,
        stack.name,
        effectiveCollectionId,
        documentCtx,
        documentCtx ? 'document' : 'general',
        sessionConfigForRequest
      ).catch(() => {
        // Error already logged by sessionManager
      });
    }
  }
}, [
  sessionId,
  sessionManager,
  selectedStackId,
  stacks,
  isLoadingSessions,
  effectiveCollectionId,
  documentContext,
  sessionConfigForRequest,
]);
```

#### E. Fix Race Condition #2: handleSend Stale Closure (Lines 573-625)

**CURRENT ISSUE**: `selectedStack` is read from closure, can be stale

**ADD REFS** (after line 119):
```typescript
// Refs for handleSend to avoid stale closures
const stacksRef = useRef(stacks);
const selectedStackIdRef = useRef(selectedStackId);
const isBaseOnlyModeRef = useRef(isBaseOnlyMode);

useEffect(() => { stacksRef.current = stacks; }, [stacks]);
useEffect(() => { selectedStackIdRef.current = selectedStackId; }, [selectedStackId]);
useEffect(() => { isBaseOnlyModeRef.current = isBaseOnlyMode; }, [isBaseOnlyMode]);
```

**UPDATE handleSend**:
```typescript
const handleSend = useCallback(async () => {
  if (!input.trim() || isStreaming) return;

  if (autoLoadEnabled && !baseModelReady) {
    toast.error('Base model is not ready. Please load it first.');
    return;
  }

  // Only block on adapter readiness when adapters are present and base-only mode is off
  if (!isBaseOnlyModeRef.current && hasAdapters && !allReady) {
    toast.warning('Some adapters are not ready. Please load them first.');
    return;
  }

  // Read current stack from ref (not closure)
  const currentStack = stacksRef.current.find(s => s.id === selectedStackIdRef.current);

  // Resolve stack to adapter IDs (allow empty when base-only mode is active)
  const adapterIds: string[] = isBaseOnlyModeRef.current
    ? []
    : Array.isArray(currentStack?.adapter_ids)
      ? currentStack.adapter_ids
      : Array.isArray((currentStack as any)?.adapters)
        ? (currentStack as any).adapters.map((a: any) => a.id ?? a.adapter_id ?? '')
        : [];

  if (!adapterIds || adapterIds.length === 0) {
    if (!isBaseOnlyModeRef.current) {
      toast.error('Please select a stack with adapters');
      return;
    }
  }

  if (!sessionManager.currentSessionId) {
    toast.error('Preparing chat session. Please wait and try again.');
    return;
  }

  // Clear input immediately
  const messageContent = input.trim();
  setInput('');

  // Send message using the streaming hook
  await sendMessage(messageContent, adapterIds);
}, [
  allReady,
  autoLoadEnabled,
  baseModelReady,
  sessionManager.currentSessionId, // Use from sessionManager
  hasAdapters,
  input,
  isStreaming,
  sendMessage,
]); // Removed stale closure dependencies: selectedStack, isBaseOnlyMode, currentSessionId
```

#### F. Fix Race Condition #3: Collection Change (Lines 796-818)

**ADD REF** (after line 119):
```typescript
const currentSessionIdRef = useRef(sessionManager.currentSessionId);
useEffect(() => {
  currentSessionIdRef.current = sessionManager.currentSessionId;
}, [sessionManager.currentSessionId]);
```

**UPDATE handleCollectionChange**:
```typescript
const handleCollectionChange = useCallback(async (collectionId: string) => {
  if (guardChatHistory()) {
    return;
  }

  // Capture current session ID from ref (not closure)
  const sessionId = currentSessionIdRef.current;
  if (!sessionId) return;

  const newCollectionId = collectionId === 'none' ? null : collectionId;
  setSelectedCollectionId(newCollectionId);

  const controller = new AbortController();
  try {
    await updateSessionCollection(sessionId, newCollectionId, {
      signal: controller.signal,
    });
    toast.success(newCollectionId ? 'Collection selected' : 'Collection cleared');
  } catch (error) {
    if (error instanceof Error && error.name !== 'AbortError') {
      logger.error('Failed to update session collection', {
        component: 'ChatInterface',
        sessionId,
        collectionId: newCollectionId,
      }, toError(error));
      toast.error('Failed to update collection');
    }
  }
}, [guardChatHistory, updateSessionCollection]);
```

#### G. Update All References to Old State

**Find and Replace**:
- `currentSessionId` → `sessionManager.currentSessionId`
- `messages` → `sessionManager.messages`
- `setMessages` → `sessionManager.setMessages`
- `editingSessionId` → `sessionManager.editing?.id ?? null`
- `newSessionName` → `sessionManager.editing?.name ?? ''`
- `setEditingSessionId(id)` + `setNewSessionName(name)` → `sessionManager.startEditing(id, name)`
- `setEditingSessionId(null)` → `sessionManager.finishEditing()`
- `isHistoryOpen` → `modals.isHistoryOpen`
- `setIsHistoryOpen` → `modals.setIsHistoryOpen`
- `isRouterActivityOpen` → `modals.isRouterActivityOpen`
- `setIsRouterActivityOpen` → `modals.setIsRouterActivityOpen`
- `isArchivePanelOpen` → `modals.isArchivePanelOpen`
- `setIsArchivePanelOpen` → `modals.setIsArchivePanelOpen`
- `shareDialogSessionId` → `modals.shareDialogSessionId`
- `setShareDialogSessionId` → `modals.setShareDialogSessionId`
- `tagsDialogSessionId` → `modals.tagsDialogSessionId`
- `setTagsDialogSessionId` → `modals.setTagsDialogSessionId`

#### H. Update Handler Functions

**handleCreateSession** (lines 701-732):
```typescript
const handleCreateSession = useCallback(async () => {
  if (guardChatHistory()) {
    return;
  }

  if (!selectedStackId) {
    toast.error('Please select a stack first');
    return;
  }
  const selectedStack = stacks.find(s => s.id === selectedStackId);
  const documentCtx = documentContext
    ? { documentId: documentContext.documentId, documentName: documentContext.documentName }
    : undefined;
  try {
    const newSession = await sessionManager.createSession(
      `Session ${new Date().toLocaleString()}`,
      selectedStackId,
      selectedStack?.name,
      effectiveCollectionId,
      documentCtx,
      documentCtx ? 'document' : 'general',
      sessionConfigForRequest
    );
    if (newSession) {
      modals.setIsHistoryOpen(false);
      toast.success('New session created');
    }
  } catch {
    // error already surfaced
  }
}, [sessionManager, documentContext, effectiveCollectionId, guardChatHistory, selectedStackId, sessionConfigForRequest, stacks, modals]);
```

**handleDeleteSession** (lines 734-747):
```typescript
const handleDeleteSession = useCallback((sessionId: string, e: React.MouseEvent) => {
  e.stopPropagation();
  if (guardChatHistory()) {
    return;
  }
  if (window.confirm('Are you sure you want to delete this session?')) {
    sessionManager.deleteSession(sessionId);
    toast.success('Session deleted');
  }
}, [sessionManager, guardChatHistory]);
```

**handleRenameSession** (lines 749-768):
```typescript
const handleRenameSession = useCallback((sessionId: string, newName: string) => {
  if (guardChatHistory()) {
    return;
  }

  // Validate session name
  const trimmedName = newName.trim();
  if (!trimmedName || trimmedName.length === 0) {
    toast.error('Session name cannot be empty');
    return;
  }
  if (trimmedName.length > 100) {
    toast.error('Session name must be 100 characters or less');
    return;
  }

  sessionManager.updateSession(sessionId, { name: trimmedName });
  sessionManager.finishEditing();
  toast.success('Session renamed');
}, [guardChatHistory, sessionManager]);
```

**handleLoadSession** (lines 668-699):
```typescript
const handleLoadSession = useCallback((sessionId: string) => {
  if (guardChatHistory()) {
    return;
  }

  const session = sessionManager.loadSession(sessionId);
  if (session) {
    if (session.stackId) {
      setSelectedStackId(session.stackId);
    }
    if (session.collectionId !== undefined) {
      setSelectedCollectionId(session.collectionId);
    }
    const config = (session.metadata as Record<string, unknown> | undefined)?.chat_session_config as
      | { stack_id?: string; routing_determinism_mode?: string; adapter_strength_overrides?: Record<string, number> }
      | undefined;
    if (config?.stack_id && !session.stackId) {
      setSelectedStackId(config.stack_id);
    }
    if (config?.routing_determinism_mode) {
      setRoutingMode(
        config.routing_determinism_mode === 'adaptive' ? 'adaptive' : 'deterministic'
      );
    }
    if (config?.adapter_strength_overrides) {
      setStrengthOverrides(config.adapter_strength_overrides);
    }
    modals.setIsHistoryOpen(false);
  }
}, [sessionManager, guardChatHistory, modals]);
```

#### I. Update Streaming Hook Callbacks (Lines 236-326)

**onMessageSent** (lines 250-266):
```typescript
onMessageSent: (message) => {
  // Add user message to messages
  sessionManager.addMessage(message);
  if (sessionManager.currentSessionId && !isChatHistoryUnsupported) {
    addMessage(sessionManager.currentSessionId, message);
  }

  // Create placeholder streaming message
  const assistantId = `assistant-${Date.now()}`;
  setStreamingMessageId(assistantId);
  sessionManager.addMessage({
    id: assistantId,
    role: 'assistant',
    content: '',
    timestamp: new Date(),
    isStreaming: true,
  });
},
```

**onStreamComplete** (lines 268-314):
```typescript
onStreamComplete: async (response) => {
  // ... (router decision and evidence fetching unchanged)

  // Replace streaming message with completed message
  sessionManager.setMessages(prev => {
    const hasStreamingPlaceholder = prev.some(msg => msg.id === streamingMessageId);
    if (hasStreamingPlaceholder) {
      return prev.map(msg => (msg.id === streamingMessageId ? completedMessage : msg));
    }
    return [...prev, completedMessage];
  });

  setStreamingMessageId(null);

  if (sessionManager.currentSessionId) {
    addMessage(sessionManager.currentSessionId, completedMessage);
  }

  // Notify workbench of message completion (for right rail auto-update)
  onMessageComplete?.(completedMessage.id, traceId);
},
```

**onError** (lines 315-322):
```typescript
onError: (error) => {
  logger.error('Chat streaming error', { component: 'ChatInterface' }, error);
  // Remove streaming message on error
  if (streamingMessageId) {
    sessionManager.setMessages(prev => prev.filter(m => m.id !== streamingMessageId));
    setStreamingMessageId(null);
  }
},
```

#### J. Update JSX References (Lines 848-1582)

**Session editing UI** (lines 1037-1058):
```typescript
{sessionManager.editing?.id === session.id ? (
  <Input
    value={sessionManager.editing.name}
    onChange={(e) => sessionManager.startEditing(session.id, e.target.value)}
    onBlur={() => {
      if (sessionManager.editing?.name.trim()) {
        handleRenameSession(session.id, sessionManager.editing.name.trim());
      } else {
        sessionManager.finishEditing();
      }
    }}
    onKeyDown={(e) => {
      if (e.key === 'Enter' && sessionManager.editing?.name.trim()) {
        handleRenameSession(session.id, sessionManager.editing.name.trim());
      } else if (e.key === 'Escape') {
        sessionManager.finishEditing();
      }
    }}
    className="h-7 text-sm mb-1"
    autoFocus
    onClick={(e) => e.stopPropagation()}
  />
) : (
```

**Session actions** (lines 1076-1078):
```typescript
onRename={() => {
  sessionManager.startEditing(session.id, session.name);
}}
```

**All modal open/close calls**: Update throughout JSX (too many to list individually)

## Expected Line Count Reduction

- **Before**: 1,580 lines
- **Extracted to hooks**: ~400 lines
- **Expected After**: ~750 lines (target: <800)

## Testing Checklist

- [ ] Session creation doesn't trigger multiple times on rapid clicks
- [ ] handleSend uses current stack value (not stale closure)
- [ ] Collection change doesn't use stale session ID
- [ ] Modal state transitions work correctly
- [ ] Session editing state management works
- [ ] All ESLint exhaustive-deps warnings resolved
- [ ] No race conditions in rapid interaction testing

## Notes

- The file is being modified by a linter/formatter, so these changes may need to be applied in a single commit
- Consider running `pnpm lint:fix` after making changes
- Some changes may require running the TypeScript compiler to verify type safety
