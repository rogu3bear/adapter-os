/**
 * ChatPage - Workbench v1
 *
 * Three-column layout:
 * - Left rail: Sessions | Datasets | Stacks tabs
 * - Center: ChatInterface
 * - Right rail: Evidence/Trace (collapsible)
 */

import { Suspense, useState, useEffect, useCallback, useMemo } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { ChatInterface } from '@/components/ChatInterface';
import { ChatErrorBoundary } from '@/components/chat/ChatErrorBoundary';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';
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
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { useSessionScope } from '@/hooks/chat/useSessionScope';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/admin/useAdmin';
import { EvidencePanel } from '@/components/evidence/EvidencePanel';
import { TraceSummaryPanel } from '@/components/trace/TraceSummaryPanel';
import { useTrace } from '@/hooks/observability/useTrace';

export default function ChatPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);
  const initialStackId = searchParams.get('stack') || undefined;
  const sessionId = searchParams.get('session') || undefined;

  // Mode toggles
  const [streamMode, setStreamMode] = useState<'tokens' | 'chunks'>('tokens');
  const [developerMode, setDeveloperMode] = useState(false);

  if (!canExecuteInference) {
    return (
      <PageWrapper pageKey="chat" title="Chat">
        <Alert variant="destructive">
          <ShieldAlert className="h-4 w-4" />
          <AlertDescription>
            You do not have permission to execute inference. Required permission: inference:execute
          </AlertDescription>
        </Alert>
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
    pinnedMessageId,
    setStrengthOverrides,
  } = useWorkbench();

  // Session scope management
  const sessionScope = useSessionScope();

  // Sessions
  const tenantId = selectedTenant || 'default';
  const {
    sessions,
    isLoading: isLoadingSessions,
    createSession,
    deleteSession,
  } = useChatSessionsApi(tenantId, { sourceType: 'general' });

  // Stacks
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStackData } = useGetDefaultStack(selectedTenant);

  // Local stack override - used to force re-render when stack selection changes
  // This is needed because sessionScope storage doesn't trigger React re-renders
  const [localStackOverride, setLocalStackOverride] = useState<string | null | undefined>(undefined);

  // Reset local override when sessionId changes (load from storage instead)
  useEffect(() => {
    setLocalStackOverride(undefined);
  }, [sessionId]);

  // Compute effective stack ID with precedence: Local Override > URL > Session Storage > Default
  const effectiveStackId = useMemo(() => {
    // 0. Local override (for immediate UI updates after stack change)
    if (localStackOverride !== undefined) return localStackOverride;

    // 1. URL param (highest priority)
    if (initialStackId) return initialStackId;

    // 2. Session stored value
    if (sessionId) {
      const scope = sessionScope.getSessionScope(sessionId);
      if (scope.selectedStackId) return scope.selectedStackId;
    }

    // 3. Default stack (lowest priority)
    return defaultStackData?.id ?? null;
  }, [localStackOverride, initialStackId, sessionId, sessionScope, defaultStackData?.id]);

  // Find active stack details
  const activeStack = useMemo(
    () => stacks.find((s) => s.id === effectiveStackId),
    [stacks, effectiveStackId]
  );

  // Trace for right rail
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const { data: traceData, isLoading: isLoadingTrace } = useTrace(selectedTraceId ?? undefined, selectedTenant);

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
      // Find session and sync stack selection to session scope
      const session = sessions.find((s) => s.id === sessionId);
      if (session?.stackId) {
        const stack = stacks.find((s) => s.id === session.stackId);
        sessionScope.setStackSelection(sessionId, session.stackId, stack?.name);
      }
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set('session', sessionId);
        return next;
      });
    },
    [sessions, stacks, sessionScope, setSearchParams]
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

  // Stack activation handler (from StacksTab)
  const handleStackActivated = useCallback((stackId: string) => {
    // Update local override for immediate UI feedback
    setLocalStackOverride(stackId);
    // Persist to session storage
    if (sessionId) {
      const stack = stacks.find((s) => s.id === stackId);
      sessionScope.setStackSelection(sessionId, stackId, stack?.name);
    }
  }, [sessionId, stacks, sessionScope]);

  // Stack change handler (from ChatInterface controlled mode)
  const handleStackChange = useCallback((stackId: string | null) => {
    // Update local override for immediate UI feedback
    setLocalStackOverride(stackId);
    // Persist to session storage
    if (sessionId && stackId) {
      const stack = stacks.find((s) => s.id === stackId);
      sessionScope.setStackSelection(sessionId, stackId, stack?.name);
    }
  }, [sessionId, stacks, sessionScope]);

  // Clear stack handler (for Detach All - sets to "base model only" mode)
  const handleClearStack = useCallback(() => {
    // Update local override to null for immediate UI feedback
    setLocalStackOverride(null);
    // Clear from session storage
    if (sessionId) {
      sessionScope.clearStackSelection(sessionId);
    }
  }, [sessionId, sessionScope]);

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
    <>
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
              <div className="flex items-center gap-2">
                <Switch
                  id="developer-mode"
                  checked={developerMode}
                  onCheckedChange={setDeveloperMode}
                />
                <label
                  htmlFor="developer-mode"
                  className="text-sm text-muted-foreground"
                >
                  Developer
                </label>
              </div>
            </div>

            {/* Right: Status chips */}
            <WorkbenchTopBar
              stackName={activeStack?.name}
              stackId={effectiveStackId}
              canExport={!!selectedTraceId}
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
                sessionId={sessionId}
                streamMode={streamMode}
                developerMode={developerMode}
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
        className="h-[calc(100vh-calc(var(--base-unit)*24))]"
      />

      {/* Floating toggle for collapsed right rail */}
      <RightRailToggle />

      {/* Undo snackbar for detach actions */}
      <UndoSnackbar sessionId={sessionId ?? null} onRestoreOverrides={setStrengthOverrides} />
    </>
  );
}
