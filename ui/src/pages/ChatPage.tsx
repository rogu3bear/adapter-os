/**
 * ChatPage - Workbench v1
 *
 * Three-column layout:
 * - Left rail: Sessions | Datasets | Stacks tabs
 * - Center: ChatInterface
 * - Right rail: Evidence/Trace (collapsible)
 */

import { Suspense, useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { ChatInterface } from '@/components/ChatInterface';
import { ChatErrorBoundary } from '@/components/chat/ChatErrorBoundary';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { ChatSkeleton } from '@/components/skeletons/ChatSkeleton';
import { Switch } from '@/components/ui/switch';
import { TooltipProvider } from '@/components/ui/tooltip';

// Workbench imports
import { WorkbenchProvider, useWorkbench } from '@/contexts/WorkbenchContext';
import { DatasetChatProvider } from '@/contexts/DatasetChatContext';
import {
  WorkbenchLayout,
  WorkbenchTopBar,
  LeftRail,
  SessionsTab,
  DatasetsTab,
  StacksTab,
  RightRail,
  RightRailToggle,
  UndoSnackbar,
} from '@/components/workbench';
import { useSessionScope } from '@/hooks/chat/useSessionScope';
import { useChatInitialLoad } from '@/hooks/chat/useChatInitialLoad';
import { ChatInitialLoadState } from '@/components/chat/ChatInitialLoadState';
import { EvidencePanel } from '@/components/evidence/EvidencePanel';
import { TraceSummaryPanel } from '@/components/trace/TraceSummaryPanel';
import { useTrace } from '@/hooks/observability/useTrace';
import { useKernelTelemetry } from '@/contexts/KernelTelemetryContext';
import { useAuth } from '@/providers/CoreProviders';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode } from '@/config/ui-mode';

export default function ChatPage() {
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();
  const [searchParams, setSearchParams] = useSearchParams();

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);
  const initialStackId = searchParams.get('stack') || undefined;
  const sessionId = searchParams.get('session') || undefined;

  // Mode toggles
  const [streamMode, setStreamMode] = useState<'tokens' | 'chunks'>('tokens');
  const [developerMode, setDeveloperMode] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (
        tag === 'input' ||
        tag === 'textarea' ||
        tag === 'select' ||
        target?.isContentEditable
      ) {
        return;
      }

      if ((e.metaKey || e.ctrlKey) && e.key === '.') {
        e.preventDefault();
        setDeveloperMode((prev) => !prev);
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  if (!canExecuteInference) {
    return (
      <PageWrapper pageKey="chat" title="Chat">
        <PermissionDenied
          requiredPermission={PERMISSIONS.INFERENCE_EXECUTE}
          requiredRoles={['admin', 'operator', 'developer']}
        />
      </PageWrapper>
    );
  }

  return (
    <PageWrapper
      pageKey="chat"
      title="Chat"
      description="Conversational interface with adapter stacks"
      contentPadding="none"
    >
      <TooltipProvider>
        <WorkbenchProvider>
          <DatasetChatProvider sessionId={sessionId}>
            <WorkbenchContent
              selectedTenant={selectedTenant}
              initialStackId={initialStackId}
              sessionId={sessionId}
              streamMode={streamMode}
              setStreamMode={setStreamMode}
              developerMode={developerMode}
              setDeveloperMode={setDeveloperMode}
              setSearchParams={setSearchParams}
            />
          </DatasetChatProvider>
        </WorkbenchProvider>
      </TooltipProvider>
    </PageWrapper>
  );
}

interface WorkbenchContentProps {
  selectedTenant: string;
  initialStackId?: string;
  sessionId?: string;
  streamMode: 'tokens' | 'chunks';
  setStreamMode: (mode: 'tokens' | 'chunks') => void;
  developerMode: boolean;
  setDeveloperMode: (mode: boolean) => void;
  setSearchParams: ReturnType<typeof useSearchParams>[1];
}

function WorkbenchContent({
  selectedTenant,
  initialStackId,
  sessionId,
  streamMode,
  setStreamMode,
  developerMode,
  setDeveloperMode,
  setSearchParams,
}: WorkbenchContentProps) {
  const {
    handleGlobalEscape,
    selectedMessageId,
    selectMessage,
    pinMessage,
    pinnedMessageId,
    setStrengthOverrides,
    setRightRailCollapsed,
  } = useWorkbench();

  // Session scope management
  const sessionScope = useSessionScope();
  const { latencyMs } = useKernelTelemetry();
  const { user } = useAuth();
  const { uiMode } = useUiMode();
  const isKernelMode = uiMode === UiMode.Kernel && user?.role?.toLowerCase() === 'developer';
  const effectiveDeveloperMode = developerMode || isKernelMode;

  // Initial load state (wraps stacks, default stack, and sessions queries)
  const tenantId = selectedTenant || 'default';
  const loadState = useChatInitialLoad(tenantId);
  const {
    stacks,
    defaultStack: defaultStackData,
    sessionsHook: {
      sessions,
      isLoading: isLoadingSessions,
      createSession,
      updateSession,
      deleteSession,
    },
  } = loadState;

  const previousTenantRef = useRef<string | null>(null);

  useEffect(() => {
    const previousTenant = previousTenantRef.current;
    previousTenantRef.current = selectedTenant;

    if (!previousTenant || !previousTenant.trim()) return;
    if (previousTenant === selectedTenant) return;

    setSelectedTraceId(null);
    pinMessage(null);
    selectMessage(null);
    setStrengthOverrides({});
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete('session');
      next.delete('stack');
      return next;
    });
  }, [pinMessage, selectMessage, selectedTenant, setSearchParams, setStrengthOverrides]);

  useEffect(() => {
    if (effectiveDeveloperMode) {
      setRightRailCollapsed(false);
    }
  }, [effectiveDeveloperMode, setRightRailCollapsed]);

  const selectedSession = useMemo(() => {
    if (!sessionId) return undefined;
    return sessions.find((s) => s.id === sessionId);
  }, [sessionId, sessions]);

  const sessionStackId = useMemo(() => {
    if (!sessionId) return undefined;
    if (!selectedSession) return undefined;
    const trimmed = selectedSession.stackId.trim();
    return trimmed ? trimmed : null;
  }, [selectedSession, sessionId]);

  // Keep tab-scoped sessionScope aligned with the backend session stack.
  // This preserves workbench undo/restore behavior without making sessionStorage authoritative.
  useEffect(() => {
    if (!sessionId) return;
    if (sessionStackId === undefined) return;

    const stack = sessionStackId ? stacks.find((s) => s.id === sessionStackId) : undefined;
    if (sessionStackId) {
      sessionScope.setStackSelection(sessionId, sessionStackId, stack?.name);
    } else {
      sessionScope.clearStackSelection(sessionId);
    }
  }, [sessionId, sessionScope, sessionStackId, stacks]);

  // Compute effective stack ID with precedence:
  // 1) Backend session (if a session is selected)
  // 2) URL stack param (when no session selected, or as pre-load fallback)
  // 3) Tenant default stack
  const effectiveStackId = useMemo(() => {
    if (sessionId) {
      if (sessionStackId !== undefined) return sessionStackId;
      if (initialStackId) return initialStackId;
      return null;
    }

    if (initialStackId) return initialStackId;
    return defaultStackData?.id ?? null;
  }, [defaultStackData?.id, initialStackId, sessionId, sessionStackId]);

  // Find active stack details
  const activeStack = useMemo(
    () => stacks.find((s) => s.id === effectiveStackId),
    [stacks, effectiveStackId]
  );

  // Trace for right rail
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const { data: traceData, isLoading: isLoadingTrace } = useTrace(selectedTraceId ?? undefined, selectedTenant);

  // Clear trace + selections when session changes (including clears)
  useEffect(() => {
    setSelectedTraceId(null);
    pinMessage(null);
    selectMessage(null);
    setStrengthOverrides({});
  }, [pinMessage, selectMessage, sessionId, setStrengthOverrides]);

  // Keep URL stack param consistent with the selected session's backend stack.
  useEffect(() => {
    if (!sessionId) return;
    if (sessionStackId === undefined) return;

    const currentUrlStackId = initialStackId?.trim() ?? '';
    const desiredStackId = sessionStackId ?? '';
    if (currentUrlStackId === desiredStackId) return;

    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (desiredStackId) {
        next.set('stack', desiredStackId);
      } else {
        next.delete('stack');
      }
      return next;
    });
  }, [initialStackId, sessionId, sessionStackId, setSearchParams]);

  // Keyboard escape handling
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        const handled = handleGlobalEscape();
        if (handled) {
          e.preventDefault();
        }
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleGlobalEscape]);

  // Session selection handler - also syncs stack from session
  const handleSelectSession = useCallback(
    (sessionId: string) => {
      setSelectedTraceId(null);
      pinMessage(null);
      selectMessage(null);
      setStrengthOverrides({});

      const session = sessions.find((s) => s.id === sessionId);
      const stackId = session?.stackId?.trim() ?? '';
      const stack = stackId ? stacks.find((s) => s.id === stackId) : undefined;

      if (stackId) {
        sessionScope.setStackSelection(sessionId, stackId, stack?.name);
      } else {
        sessionScope.clearStackSelection(sessionId);
      }

      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set('session', sessionId);
        if (stackId) {
          next.set('stack', stackId);
        } else {
          next.delete('stack');
        }
        return next;
      });
    },
    [pinMessage, selectMessage, sessions, sessionScope, setSearchParams, setStrengthOverrides, stacks]
  );

  // New session handler
  const handleCreateSession = useCallback(async () => {
    try {
      const session = await createSession('New Chat', effectiveStackId ?? '');
      if (session) {
        handleSelectSession(session.id);
      }
    } catch (error) {
      // Error handling is done in the hook
    }
  }, [createSession, handleSelectSession, effectiveStackId]);

  const applyStackSelection = useCallback(
    (nextStackId: string | null) => {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        if (nextStackId) {
          next.set('stack', nextStackId);
        } else {
          next.delete('stack');
        }
        return next;
      });

      if (!sessionId) return;

      const stack = nextStackId ? stacks.find((s) => s.id === nextStackId) : undefined;
      if (nextStackId) {
        sessionScope.setStackSelection(sessionId, nextStackId, stack?.name);
      } else {
        sessionScope.clearStackSelection(sessionId);
      }

      const session = sessions.find((s) => s.id === sessionId);
      let metadata: Record<string, unknown> | undefined;
      if (session) {
        const nextMetadata: Record<string, unknown> = { ...(session.metadata ?? {}) };
        if (stack?.name) {
          nextMetadata.stackName = stack.name;
        } else {
          delete nextMetadata.stackName;
        }
        metadata = Object.keys(nextMetadata).length > 0 ? nextMetadata : undefined;
      }

      updateSession(sessionId, {
        stackId: nextStackId ?? '',
        stackName: stack?.name,
        metadata,
      });
    },
    [sessionId, sessions, sessionScope, setSearchParams, stacks, updateSession]
  );

  // Stack activation handler (from StacksTab)
  const handleStackActivated = useCallback((stackId: string) => {
    applyStackSelection(stackId);
  }, [applyStackSelection]);

  // Stack change handler (from ChatInterface controlled mode)
  const handleStackChange = useCallback((stackId: string | null) => {
    applyStackSelection(stackId);
  }, [applyStackSelection]);

  // Clear stack handler (for Detach All - sets to "base model only" mode)
  const handleClearStack = useCallback(() => {
    applyStackSelection(null);
  }, [applyStackSelection]);

  useEffect(() => {
    const handleDetach = () => handleClearStack();
    window.addEventListener('aos:detach-all', handleDetach);
    return () => window.removeEventListener('aos:detach-all', handleDetach);
  }, [handleClearStack]);

  // Message completion handler (for right rail auto-update)
  const handleMessageComplete = useCallback(
    (messageId: string, traceId?: string) => {
      if (!pinnedMessageId) {
        selectMessage(messageId);
        if (traceId) {
          setSelectedTraceId(traceId);
        }
      }
    },
    [pinnedMessageId, selectMessage]
  );

  // Message selection handler (when user clicks a message)
  const handleMessageSelect = useCallback(
    (messageId: string, traceId?: string) => {
      // Update selected message (respects pin state via selectMessage)
      selectMessage(messageId);
      // Update trace for right rail (only if not pinned)
      if (!pinnedMessageId && traceId) {
        setSelectedTraceId(traceId);
      }
    },
    [pinnedMessageId, selectMessage]
  );

  const handleSessionChange = useCallback(
    (nextSessionId: string | null) => {
      setSelectedTraceId(null);
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        if (nextSessionId) {
          next.set('session', nextSessionId);
        } else {
          next.delete('session');
        }
        return next;
      });
    },
    [setSearchParams, setSelectedTraceId]
  );

  // Convert sessions to the format expected by SessionsTab
  const sessionsList = useMemo(
    () =>
      sessions.map((s) => ({
        id: s.id,
        name: s.name,
        stackId: s.stackId,
        stackName: s.stackName,
        updatedAt: s.updatedAt,
        messageCount: s.messages?.length,
      })),
    [sessions]
  );

  return (
    <ChatInitialLoadState loadState={loadState}>
      <WorkbenchLayout
        topBar={
          <div className="flex items-center justify-between gap-4">
            {/* Left: Mode toggles */}
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Switch
                  id="stream-mode"
                  checked={streamMode === 'tokens'}
                  onCheckedChange={(checked) =>
                    setStreamMode(checked ? 'tokens' : 'chunks')
                  }
                />
                <label
                  htmlFor="stream-mode"
                  className="text-sm text-muted-foreground"
                >
                  Stream: {streamMode}
                </label>
              </div>
              <div className="flex items-center gap-3 px-3 py-2 rounded-md border bg-muted/40">
                <Switch
                  id="developer-mode"
                  checked={effectiveDeveloperMode}
                  disabled={isKernelMode}
                  onCheckedChange={setDeveloperMode}
                />
                <div className="flex flex-col leading-tight">
                  <span className="text-sm font-semibold text-foreground">
                    {isKernelMode ? 'Kernel Mode' : developerMode ? 'OS Mode' : 'User Mode'}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {isKernelMode ? 'Tokens, Q15 ranks, receipts' : 'Metrics, debugger, traces'}
                  </span>
                </div>
              </div>
            </div>

            {/* Right: Status chips */}
            <WorkbenchTopBar
              stackName={activeStack?.name}
              stackId={effectiveStackId}
              canExport={!!selectedTraceId}
              latencyMs={latencyMs}
            />
          </div>
        }
        leftRail={
          <LeftRail
            sessionsContent={
              <SessionsTab
                sessions={sessionsList}
                activeSessionId={sessionId}
                onSelectSession={handleSelectSession}
                onCreateSession={handleCreateSession}
                onDeleteSession={deleteSession}
                isLoading={isLoadingSessions}
              />
            }
            datasetsContent={<DatasetsTab />}
            stacksContent={
              <StacksTab
                activeStackId={effectiveStackId}
                sessionId={sessionId}
                onStackActivated={handleStackActivated}
                onClearStack={handleClearStack}
              />
            }
          />
        }
        center={
          <ChatErrorBoundary>
            <Suspense fallback={<ChatSkeleton />}>
              <ChatInterface
                selectedTenant={selectedTenant}
                selectedStackId={effectiveStackId}
                onStackChange={handleStackChange}
                onSessionChange={handleSessionChange}
                sessionId={sessionId}
                streamMode={streamMode}
                developerMode={developerMode}
                kernelMode={isKernelMode}
                onMessageComplete={handleMessageComplete}
                onMessageSelect={handleMessageSelect}
                selectedMessageId={selectedMessageId}
              />
            </Suspense>
          </ChatErrorBoundary>
        }
        rightRail={
          <RightRail title="Trace">
            {isLoadingTrace ? (
              <div className="flex items-center justify-center py-8">
                <div className="animate-spin h-5 w-5 border-2 border-primary border-t-transparent rounded-full" />
                <span className="ml-2 text-sm text-muted-foreground">Loading trace...</span>
              </div>
            ) : traceData ? (
              <div className="space-y-4">
                <TraceSummaryPanel trace={traceData} />
                <EvidencePanel
                  traceId={traceData.trace_id}
                  tenantId={selectedTenant}
                />
              </div>
            ) : selectedTraceId ? (
              <div className="text-sm text-muted-foreground text-center py-8">
                Trace not found
              </div>
            ) : (
              <div className="text-sm text-muted-foreground text-center py-8">
                Send a message to see trace details
              </div>
            )}
          </RightRail>
        }
        className="min-h-0"
      />

      {/* Floating toggle for collapsed right rail */}
      <RightRailToggle />

      {/* Undo snackbar for detach actions */}
      <UndoSnackbar
        sessionId={sessionId ?? null}
        onRestoreOverrides={setStrengthOverrides}
        onRestoreStack={applyStackSelection}
      />
    </ChatInitialLoadState>
  );
}
