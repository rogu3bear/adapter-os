import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ChatMessageComponent } from './chat/ChatMessage';
import type { ChatMessage, EvidenceItem, ChatInterfaceProps, RunMetadata } from '@/types/components';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { SectionAsyncBoundary } from '@/components/shared/Feedback/AsyncBoundary';
import { Send, Loader2, Layers, History, X, ChevronLeft, Plus, Activity, Database, Archive, Trash2, FileText, Bug, Copy, RefreshCw, PlayCircle, Link2 } from 'lucide-react';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/admin/useAdmin';
import { useCollections } from '@/hooks/api/useCollectionsApi';
import type { ExtendedRouterDecision } from '@/api/types';
import type { ReceiptVerificationResult } from '@/api/api-types';
import { RouterActivitySidebar } from './chat/RouterActivitySidebar';
import { AdapterLoadingStatus } from './chat/AdapterLoadingStatus';
import { PreChatAdapterPrompt } from './chat/PreChatAdapterPrompt';
import { ChatSearchBar } from './chat/ChatSearchBar';
import { ChatSessionActions } from './chat/ChatSessionActions';
import { AdapterMountIndicators, type AdapterMountItem, type AdapterMountTransition } from './chat/AdapterMountIndicators';
import { NeuralDebuggerPanel } from './chat/NeuralDebuggerPanel';
import { ChatTagsManager } from './chat/ChatTagsManager';
import { ChatCategoriesManager } from './chat/ChatCategoriesManager';
import { ChatShareDialog } from './chat/ChatShareDialog';
import { ChatArchivePanel } from './chat/ChatArchivePanel';
import { InlineModelLoadingBlock } from './chat/InlineModelLoadingBlock';
import { useChatExport } from '@/components/export';
import { AdapterAttachmentChip } from './chat/AdapterAttachmentChip';
import { cn } from '@/lib/utils';
import {
  useChatStreaming,
  useChatAdapterState,
  useChatRouterDecisions,
  useSessionManager,
  useChatModals,
} from '@/hooks/chat';
import { useAutoAttach } from '@/hooks/chat/useAutoAttach';
import { useChatAutoLoadModels } from '@/hooks/config/useFeatureFlags';
import {
  useModelLoadingState,
  useModelLoader,
  useChatLoadingPersistence,
  useLoadingAnnouncements,
} from '@/hooks/model-loading';
import { ChatLoadingOverlay } from './chat/ChatLoadingOverlay';
import { ChatErrorDisplay } from './chat/ChatErrorDisplay';
import { MissingPinnedAdaptersBanner } from './chat/MissingPinnedAdaptersBanner';
import { EvidenceDrawerProvider, useEvidenceDrawerOptional } from '@/contexts/EvidenceDrawerContext';
import { ChatProvider, useChatContextOptional, type SuggestedAdapter } from '@/contexts/ChatContext';
import { EvidenceDrawer } from './chat/EvidenceDrawer';
import { apiClient } from '@/api/services';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { useSystemMetrics, useWorkers } from '@/hooks/system/useSystemMetrics';
import { AdapterSuggestion } from './chat/AdapterSuggestion';
import { classifyMagnet, colorWithAlpha, playMagnetSnapFeedback } from '@/utils/adapterMagnet';
import { useDemoMode } from '@/hooks/demo/DemoProvider';
import { useDemoScriptRunner } from '@/hooks/demo/useDemoScriptRunner';
import { Switch } from '@/components/ui/switch';
import { RunEvidencePanel } from '@/components/chat/RunEvidencePanel';

// LocalStorage key for chat auto-load preference
const CHAT_AUTO_LOAD_KEY = 'aos-chat-auto-load-model';
const AUTO_ATTACH_KEY = 'aos-chat-auto-attach';
const MAGNET_CONFIDENCE_THRESHOLD = 0.85;

interface WorkspaceActiveState {
  activeBaseModelId?: string | null;
  activePlanId?: string | null;
  activeAdapterIds?: string[] | null;
  manifestHashB3?: string | null;
  policyMaskDigestB3?: string | null;
  updatedAt?: string | null;
}

const mergeDefinedFields = (base: Record<string, unknown>, update: Record<string, unknown>) => {
  const merged = { ...base };
  for (const [key, value] of Object.entries(update)) {
    if (value !== undefined) {
      merged[key] = value;
    }
  }
  return merged;
};

function ChatInterfaceInner({
  selectedTenant,
  initialStackId,
  selectedStackId: rawSelectedStackId,
  onStackChange,
  onSessionChange,
  sessionId,
  documentContext,
  datasetContext,
  onViewDocument,
  streamMode = 'tokens',
  developerMode = false,
  kernelMode = false,
  onMessageComplete,
  onMessageSelect,
  selectedMessageId,
}: ChatInterfaceProps) {
  const isStackControlled = rawSelectedStackId !== undefined;
  const [internalStackId, setInternalStackId] = useState<string>(() => initialStackId ?? '');

  useEffect(() => {
    if (isStackControlled) return;
    if (!initialStackId) return;
    setInternalStackId((current) => (current ? current : initialStackId));
  }, [initialStackId, isStackControlled]);

  // Normalize selectedStackId: null/undefined -> empty string for base-model-only mode
  const selectedStackId = (isStackControlled ? rawSelectedStackId : internalStackId) || '';

  // Use tenantId for API hooks that support undefined (default stack)
  const tenantId = selectedTenant || 'default';
  const sessionSourceType = documentContext ? 'document' : 'general';

  // Session management hook
  const sessionManager = useSessionManager({
    tenantId,
    sessionSourceType,
    documentContext,
  });

  // Destructure session state
  const {
    currentSessionId,
    messages,
    setMessages,
    setCurrentSessionId,
    clearSession,
    loadSession,
    createSession: createSessionFromManager,
  } = sessionManager;
  const chatContext = useChatContextOptional();

  // Modal management hook
  const {
    isHistoryOpen,
    setIsHistoryOpen,
    isRouterActivityOpen,
    setIsRouterActivityOpen,
    isArchivePanelOpen,
    setIsArchivePanelOpen,
    shareDialogSessionId,
    setShareDialogSessionId,
    tagsDialogSessionId,
    setTagsDialogSessionId,
    categoryDialogSessionId,
    setCategoryDialogSessionId,
  } = useChatModals();
  const [isDebuggerOpen, setIsDebuggerOpen] = useState(false);

  // Local editing state (for session rename)
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState('');

  // Remaining local state
  const [input, setInput] = useState('');
  const [autoAttachEnabled, setAutoAttachEnabled] = useState<boolean>(() => {
    try {
      return localStorage.getItem(AUTO_ATTACH_KEY) !== 'false';
    } catch {
      return true;
    }
  });
  const {
    suggestedAdapters,
    attachedAdapters,
    lastAttachedAdapterId,
    autoAttachPaused,
    attachWithResolution,
    removeAttachedAdapter,
    predictionLoading,
    predictionError,
    bestSuggestion,
    muteAdapter,
    conflictState,
  } = useAutoAttach({
    text: input,
    autoAttachEnabled,
    stackId: selectedStackId,
    tenantId,
  });
  const setSelectedStackId = useCallback(
    (stackId: string | null) => {
      if (!isStackControlled) {
        setInternalStackId(stackId ?? '');
      }
      onStackChange?.(stackId);
    },
    [isStackControlled, onStackChange]
  );
  const [selectedCollectionId, setSelectedCollectionId] = useState<string | null>(documentContext?.collectionId ?? null);
  const [showContext, setShowContext] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');
  const [routingMode, setRoutingMode] = useState<'deterministic' | 'adaptive'>('deterministic');
  const [strengthOverrides, setStrengthOverrides] = useState<Record<string, number>>({});

  const attemptedSessionLoadRef = useRef<string | null>(null);
  const previousTenantIdRef = useRef<string | null>(null);
  const previousSessionIdPropRef = useRef<string | undefined>(sessionId);
  const previousChatSessionRef = useRef<string | null>(null);

  // If the parent-controlled sessionId changes, clear current state immediately so we don't show stale messages.
  useEffect(() => {
    const previousSessionIdProp = previousSessionIdPropRef.current;
    if (previousSessionIdProp === sessionId) return;
    previousSessionIdPropRef.current = sessionId;

    if (!sessionId) {
      if (!previousSessionIdProp) return;
      attemptedSessionLoadRef.current = null;
      clearSession();
      return;
    }

    if (sessionId === currentSessionId) return;
    attemptedSessionLoadRef.current = null;
    clearSession();
  }, [clearSession, currentSessionId, sessionId]);

  useEffect(() => {
    if (!chatContext) return;
    const key = currentSessionId ?? null;
    chatContext.setActiveSessionId(key);
    if (previousChatSessionRef.current !== key) {
      chatContext.reset();
      previousChatSessionRef.current = key;
    }
  }, [chatContext, currentSessionId]);

  useEffect(() => {
    try {
      localStorage.setItem(AUTO_ATTACH_KEY, String(autoAttachEnabled));
    } catch {
      // ignore
    }
  }, [autoAttachEnabled]);

  useEffect(() => {
    if (!lastAttachedAdapterId) return;
    if (lastAutoSnapRef.current === lastAttachedAdapterId) return;

    const attached = attachedAdapters.find((adapter) => adapter.id === lastAttachedAdapterId);
    if (!attached || attached.attachedBy !== 'auto') return;

    lastAutoSnapRef.current = lastAttachedAdapterId;
    playMagnetSnapFeedback();
  }, [attachedAdapters, lastAttachedAdapterId]);

  // Notify parent when the active session changes (e.g., user loads/creates session within ChatInterface).
  const hasNotifiedSessionChangeRef = useRef(false);
  useEffect(() => {
    if (!onSessionChange) {
      return;
    }
    if (!hasNotifiedSessionChangeRef.current) {
      hasNotifiedSessionChangeRef.current = true;
      return;
    }
    onSessionChange(currentSessionId);
  }, [currentSessionId, onSessionChange]);

  // Refs
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const stackSelectorRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const lastAutoSnapRef = useRef<string | null>(null);
  const { enabled: demoMode } = useDemoMode();
  const { run: runDemoScript, isTyping: isTypingDemoScript } = useDemoScriptRunner({
    enabled: demoMode,
    setInput,
    focus: () => inputRef.current?.focus(),
  });

  // Auto-load preference (user's choice to auto-load model on chat page)
  const [chatAutoLoadPreference, setChatAutoLoadPreference] = useState<boolean>(() => {
    try {
      return localStorage.getItem(CHAT_AUTO_LOAD_KEY) === 'true';
    } catch {
      return false;
    }
  });

  const handleChatAutoLoadPreferenceChange = useCallback((enabled: boolean) => {
    setChatAutoLoadPreference(enabled);
    try {
      localStorage.setItem(CHAT_AUTO_LOAD_KEY, String(enabled));
    } catch {
      // Silently fail
    }
  }, []);

  // Pinned adapters banner state
  const [bannerDismissed, setBannerDismissed] = useState(false);

  // Evidence drawer for auto-follow
  const evidenceDrawer = useEvidenceDrawerOptional();

  // Feature flags
  const autoLoadEnabled = useChatAutoLoadModels();
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);
  const { data: collections = [] } = useCollections();
  const {
    sessions,
    isLoading: isLoadingSessions,
    isUnsupported: isChatHistoryUnsupported,
    unsupportedReason: chatHistoryUnsupportedReason,
    createSession,
    updateSession,
    addMessage,
    deleteSession,
    getSession,
    updateSessionCollection,
  } = useChatSessionsApi(tenantId, {
    sourceType: sessionSourceType,
    documentId: documentContext?.documentId,
    documentName: documentContext?.documentName,
    collectionId: documentContext?.collectionId ?? null,
  });
  const chatHistoryUnsupportedMessage =
    chatHistoryUnsupportedReason ?? 'Chat history is not supported for this version.';
  const guardChatHistory = useCallback(() => {
    if (!isChatHistoryUnsupported) {
      return false;
    }
    toast.info(chatHistoryUnsupportedMessage);
    return true;
  }, [chatHistoryUnsupportedMessage, isChatHistoryUnsupported]);

  useEffect(() => {
    if (isChatHistoryUnsupported && isHistoryOpen) {
      setIsHistoryOpen(false);
    }
  }, [isChatHistoryUnsupported, isHistoryOpen]);

  // Memoize selected stack (needed before hooks)
  const selectedStack = useMemo(
    () => stacks.find(s => s.id === selectedStackId),
    [stacks, selectedStackId]
  );

  const adapterList = useMemo(() => {
    // selectedStack can have either `adapters` (ActiveAdapter[]) or `adapter_ids` (string[])
    if (selectedStack?.adapters && selectedStack.adapters.length > 0) {
      return selectedStack.adapters.map(adapter => ({
        id: adapter.id ?? adapter.adapter_id,
        name: adapter.name ?? adapter.adapter_id,
        tier: undefined, // ActiveAdapter doesn't have tier
        domain: undefined, // ActiveAdapter doesn't have domain
        strength: adapter.gate ?? 1, // Use gate value from ActiveAdapter
      })).filter(adapter => adapter.id);
    }

    if (selectedStack?.adapter_ids && selectedStack.adapter_ids.length > 0) {
      return selectedStack.adapter_ids.map((id: string) => ({
        id,
        name: id,
        tier: undefined,
        domain: undefined,
        strength: 1,
      }));
    }

    return [];
  }, [selectedStack]);

  useEffect(() => {
    if (adapterList.length === 0) {
      setStrengthOverrides({});
      return;
    }
    setStrengthOverrides(prev => {
      const next: Record<string, number> = {};
      adapterList.forEach(adapter => {
        const existing = prev[adapter.id];
        next[adapter.id] = typeof existing === 'number' ? existing : 1;
      });
      return next;
    });
  }, [adapterList]);

  const handleStrengthChange = useCallback((adapterId: string, value: number) => {
    const clamped = Math.min(2, Math.max(0, value));
    setStrengthOverrides(prev => ({ ...prev, [adapterId]: clamped }));
  }, []);

  const sessionConfigForRequest = useMemo(() => ({
    stack_id: selectedStackId || undefined,
    routing_determinism_mode: routingMode,
    adapter_strength_overrides: strengthOverrides,
  }), [selectedStackId, routingMode, strengthOverrides]);

  // Streaming message state (for in-progress messages)
  const [streamingMessageId, setStreamingMessageId] = useState<string | null>(null);
  const [isBaseOnlyMode, setIsBaseOnlyMode] = useState(false);
  const [latestTraceId, setLatestTraceId] = useState<string | null>(null);
  const [modelGateBypass, setModelGateBypass] = useState(false);

  // Compute effective collection ID (reacts to documentContext or datasetContext changes)
  const effectiveCollectionId = useMemo(
    () => documentContext?.collectionId || datasetContext?.collectionId || selectedCollectionId || undefined,
    [documentContext?.collectionId, datasetContext?.collectionId, selectedCollectionId]
  );

  // For document chat, keep the collection fixed to the provided context (if any)
  useEffect(() => {
    if (documentContext?.collectionId && selectedCollectionId !== documentContext.collectionId) {
      setSelectedCollectionId(documentContext.collectionId);
    }
  }, [documentContext?.collectionId, selectedCollectionId]);

  const [verificationReports, setVerificationReports] = useState<Record<string, ReceiptVerificationResult>>({});
  const [verificationDialogTrace, setVerificationDialogTrace] = useState<string | null>(null);
  const [verificationDialogError, setVerificationDialogError] = useState<string | null>(null);
  const [verificationDialogLoading, setVerificationDialogLoading] = useState(false);

  const fetchVerificationReport = useCallback(async (traceId: string, silent = false): Promise<ReceiptVerificationResult | null> => {
    if (!traceId) return null;
    if (verificationReports[traceId]) return verificationReports[traceId];

    if (!silent) {
      setVerificationDialogLoading(true);
      setVerificationDialogError(null);
    }

    try {
      const report = await apiClient.verifyTraceReceipt(traceId);
      setVerificationReports((prev) => ({ ...prev, [traceId]: report }));
      return report;
    } catch (err) {
      const error = toError(err);
      if (!silent) {
        setVerificationDialogError(error.message);
        toast.error(`Verification failed: ${error.message}`);
      }
      logger.error('Verification fetch failed', {
        component: 'ChatInterface',
        traceId,
      }, error);
      return null;
    } finally {
      if (!silent) {
        setVerificationDialogLoading(false);
      }
    }
  }, [verificationReports]);

  const [workspaceActiveState, setWorkspaceActiveState] = useState<WorkspaceActiveState | null>(null);
  const [workspaceStateLoading, setWorkspaceStateLoading] = useState(false);
  const workspaceActiveSnapshot = workspaceActiveState;

  const fetchWorkspaceActiveState = useCallback(async () => {
    setWorkspaceStateLoading(true);
    let resolved = false;

    try {
      const canonicalPath = `/v1/workspaces/${encodeURIComponent(tenantId)}/active`;
      const response = await apiClient.request<WorkspaceActiveState>(canonicalPath);
      setWorkspaceActiveState(response);
      resolved = true;
    } catch (err) {
      logger.warn(
        'Failed to fetch workspace active state via canonical endpoint',
        { component: 'ChatInterface', tenantId, hint: 'workspace_active_state' },
        toError(err)
      );
    }

    if (!resolved) {
      try {
        const query = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : '';
        const response = await apiClient.request<WorkspaceActiveState>(`/v1/workspaces/active-state${query}`);
        setWorkspaceActiveState(response);
        resolved = true;
        logger.warn('Workspace active state loaded via legacy endpoint', {
          component: 'ChatInterface',
          tenantId,
          hint: 'workspace_active_state',
        });
      } catch (err) {
        logger.warn(
          'Failed to fetch workspace active state via legacy endpoint',
          { component: 'ChatInterface', tenantId, hint: 'workspace_active_state' },
          toError(err)
        );
      }
    }

    if (!resolved) {
      setWorkspaceActiveState(null);
    }

    setWorkspaceStateLoading(false);
  }, [tenantId]);

  useEffect(() => {
    void fetchWorkspaceActiveState();
  }, [fetchWorkspaceActiveState]);

  const mergeRunMetadataIntoMessages = useCallback(
    (metadata: RunMetadata) => {
      setMessages((prev) =>
        prev.map((message) => {
          const key = message.traceId || message.requestId || message.id;
          const matches =
            (metadata.traceId && metadata.traceId === key) ||
            (metadata.requestId && metadata.requestId === key) ||
            (streamingMessageId && message.id === streamingMessageId);

          if (!matches) {
            return message;
          }

          const existing = message.runMetadata ?? {};
          const merged = mergeDefinedFields(
            existing as Record<string, unknown>,
            metadata as Record<string, unknown>
          );
          const resolvedSeedMaterial = metadata.seedMaterial ?? existing.seedMaterial;
          const nextRunMetadata: RunMetadata & Record<string, unknown> = {
            ...merged,
            requestId: metadata.requestId ?? existing.requestId ?? message.requestId,
            traceId: metadata.traceId ?? existing.traceId ?? message.traceId,
            planId: metadata.planId ?? existing.planId ?? workspaceActiveSnapshot?.activePlanId ?? undefined,
            manifestHashB3:
              metadata.manifestHashB3 ?? existing.manifestHashB3 ?? workspaceActiveSnapshot?.manifestHashB3 ?? undefined,
            policyMaskDigestB3:
              metadata.policyMaskDigestB3 ??
              existing.policyMaskDigestB3 ??
              workspaceActiveSnapshot?.policyMaskDigestB3 ??
              undefined,
            seededViaHkdf:
              metadata.seededViaHkdf ??
              existing.seededViaHkdf ??
              (resolvedSeedMaterial ? true : undefined),
            seedMaterial: resolvedSeedMaterial,
          };

          return { ...message, runMetadata: nextRunMetadata };
        })
      );
    },
    [
      streamingMessageId,
      workspaceActiveSnapshot?.activePlanId,
      workspaceActiveSnapshot?.manifestHashB3,
      workspaceActiveSnapshot?.policyMaskDigestB3,
    ]
  );

  const handleRunMetadata = useCallback(
    (metadata: RunMetadata) => {
      if (metadata.traceId && !latestTraceId) {
        setLatestTraceId(metadata.traceId);
      }
      mergeRunMetadataIntoMessages(metadata);
    },
    [latestTraceId, mergeRunMetadataIntoMessages]
  );

  // Manual smoke checklist (Chat MVP):
  // - Stream a prompt; `aos.run_envelope` fills run_id/workspace_id before tokens.
  // - Export evidence: canonical `/v1/runs/{run_id}/evidence` first, legacy alias warns.
  // - Mid-stream failure keeps envelope values visible; export warns on failure.
  // - Workspace switch mid-chat preserves original workspace_id in the panel.
  // - Dev bypass keeps router_seed hidden unless dev mode is on.
  const handleExportRunEvidence = useCallback(
    async (message: ChatMessage) => {
      const runMeta = message.runMetadata;
      const runMetaRecord = runMeta as Record<string, unknown> | undefined;
      const readMetaScalar = (keys: string[]): string | number | boolean | undefined => {
        for (const key of keys) {
          const value = runMetaRecord?.[key];
          if (value === undefined || value === null) continue;
          if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
            return value;
          }
        }
        return undefined;
      };

      const rawRunId =
        (runMeta?.runId || runMeta?.requestId || readMetaScalar(['run_id', 'request_id'])) ??
        message.traceId ??
        message.requestId ??
        undefined;
      const runId = rawRunId === undefined ? undefined : String(rawRunId);
      const traceId = message.traceId || runMeta?.traceId || runId || undefined;
      const baseName = runId || traceId || message.id;
      const apiFilename = `run-evidence-${baseName}.zip`;
      const localFilename = `run-evidence-${baseName}-unverified-local-bundle.json`;

      if (!runId && !traceId) {
        toast.error('Evidence is still loading for this run.');
        return;
      }

      const downloadBundle = async (path: string, filename: string, label: 'canonical' | 'legacy'): Promise<boolean> => {
        try {
          const url = apiClient.buildUrl(path);
          const token = apiClient.getToken();
          const response = await fetch(url, {
            method: 'GET',
            headers: token ? { Authorization: `Bearer ${token}` } : undefined,
          });

          if (!response.ok) {
            throw new Error(`Export failed (${response.status})`);
          }

          const blob = await response.blob();
          const blobUrl = window.URL.createObjectURL(blob);
          const link = document.createElement('a');
          link.href = blobUrl;
          link.download = filename;
          document.body.appendChild(link);
          link.click();
          document.body.removeChild(link);
          window.URL.revokeObjectURL(blobUrl);
          return true;
        } catch (err) {
          logger.warn(
            'Evidence export via API failed, falling back',
            {
              component: 'ChatInterface',
              endpoint: label,
              path,
              messageId: message.id,
              traceId,
              runId,
            },
            toError(err)
          );
          return false;
        }
      };

      const fallbackExport = () => {
        const toStringOrNull = (value: string | number | boolean | undefined | null): string | null =>
          value === undefined || value === null ? null : String(value);
        const workspaceId = toStringOrNull(readMetaScalar(['workspaceId', 'workspace_id', 'tenantId', 'tenant_id']) ?? tenantId);
        const routerSeed = toStringOrNull(readMetaScalar(['routerSeed', 'router_seed']));
        const tick = readMetaScalar(['tick']);
        const determinismVersion = toStringOrNull(readMetaScalar(['determinismVersion', 'determinism_version']));
        const bootTraceId = toStringOrNull(readMetaScalar(['bootTraceId', 'boot_trace_id']));
        const createdAt = toStringOrNull(readMetaScalar(['createdAt', 'created_at']));
        const rawReasoningMode = runMeta?.reasoningMode ?? readMetaScalar(['reasoningMode', 'reasoning_mode']);
        const reasoningMode =
          typeof rawReasoningMode === 'boolean'
            ? rawReasoningMode
            : typeof rawReasoningMode === 'string'
              ? rawReasoningMode.toLowerCase() === 'true'
                ? true
                : rawReasoningMode.toLowerCase() === 'false'
                  ? false
                  : null
              : null;

        const bundle = {
          bundle_label: 'unverified local bundle',
          run_id: runId ?? null,
          workspace_id: workspaceId ?? null,
          manifest_hash_b3: runMeta?.manifestHashB3 ?? workspaceActiveSnapshot?.manifestHashB3 ?? null,
          policy_mask_digest_b3:
            runMeta?.policyMaskDigestB3 ??
            workspaceActiveSnapshot?.policyMaskDigestB3 ??
            message.routerDecision?.policy_mask_digest ??
            null,
          plan_id: runMeta?.planId ?? workspaceActiveSnapshot?.activePlanId ?? null,
          router_seed: routerSeed ?? null,
          tick: typeof tick === 'number' || typeof tick === 'string' ? tick : null,
          worker_id: runMeta?.workerId ?? null,
          reasoning_mode: reasoningMode,
          determinism_version: determinismVersion,
          boot_trace_id: bootTraceId,
          created_at: createdAt,
          message_id: message.id,
        };

        const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' });
        const url = window.URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = localFilename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        window.URL.revokeObjectURL(url);
      };

      const targetId = runId || traceId;
      if (!targetId) return;
      const canonicalPath = `/v1/runs/${encodeURIComponent(String(targetId))}/evidence`;
      const legacyPath = `/v1/evidence/runs/${encodeURIComponent(String(targetId))}/export`;
      const exportedCanonical = await downloadBundle(canonicalPath, apiFilename, 'canonical');

      if (exportedCanonical) {
        toast.success('Exported evidence bundle');
        return;
      }

      const exportedLegacy = await downloadBundle(legacyPath, apiFilename, 'legacy');
      if (exportedLegacy) {
        toast.warning('Exported evidence bundle via legacy endpoint');
        return;
      }

      fallbackExport();
      toast.warning('API export failed; downloaded unverified local bundle.');
    },
    [
      tenantId,
      workspaceActiveSnapshot?.activePlanId,
      workspaceActiveSnapshot?.manifestHashB3,
      workspaceActiveSnapshot?.policyMaskDigestB3,
    ]
  );

  // Chat streaming hook
  const {
    isStreaming,
    streamedText,
    currentRequestId,
    sendMessage,
    cancelStream,
    chunks,
    tokensReceived,
    streamDuration,
  } = useChatStreaming({
    sessionId: currentSessionId,
    stackId: selectedStackId,
    collectionId: effectiveCollectionId,
    documentId: documentContext?.documentId,
    onRunMetadata: handleRunMetadata,
    onMessageSent: (message) => {
      // Add user message to messages
      setMessages(prev => [...prev, message]);
        if (currentSessionId && !isChatHistoryUnsupported) {
        addMessage(currentSessionId, message);
      }

      // Create placeholder streaming message
      const assistantId = `assistant-${Date.now()}`;
      setStreamingMessageId(assistantId);
      setMessages(prev => [...prev, {
        id: assistantId,
        role: 'assistant',
        content: '',
        timestamp: new Date(),
        isStreaming: true,
        runMetadata: {
          requestId: currentRequestId ?? undefined,
          traceId: currentRequestId ?? undefined,
          planId: workspaceActiveState?.activePlanId ?? undefined,
          manifestHashB3: workspaceActiveState?.manifestHashB3 ?? undefined,
          policyMaskDigestB3: workspaceActiveState?.policyMaskDigestB3 ?? undefined,
          workspaceId: tenantId,
        } as RunMetadata,
      }]);
    },
	    onStreamComplete: async (response) => {
	      const completedMessageId = streamingMessageId || response.id;
	      const traceId = response.traceId || response.requestId || currentRequestId || undefined;
        setLatestTraceId(traceId ?? null);

	      // Fetch router decision and evidence
	      let routerDecision = null;
	      if (traceId) {
	        const decision = await fetchDecision(completedMessageId, traceId);
	        routerDecision = decision;
	      }

      const policyMaskDigest = (routerDecision as { policy_mask_digest?: string } | null)?.policy_mask_digest;
      if (policyMaskDigest) {
        handleRunMetadata({
          policyMaskDigestB3: policyMaskDigest,
          traceId: traceId ?? response.traceId ?? undefined,
          requestId: response.requestId ?? undefined,
        });
      }

	      // Fetch evidence
	      const evidence = await fetchMessageEvidence(completedMessageId);

      // Use throughput stats from response (calculated in useChatStreaming with accurate values)
      // This avoids timing issues with React state batching

	      const completedMessage = {
	        ...response,
	        id: completedMessageId,
	        traceId, // Store traceId on message for later trace fetching
	        routerDecision: routerDecision as ExtendedRouterDecision | null,
	        evidence,
	        isVerified: evidence.length > 0,
        verifiedAt: evidence.length > 0 ? new Date().toISOString() : undefined,
        isStreaming: false,
        // throughputStats comes from response via useChatStreaming
      };

      // Replace streaming message with completed message
      setMessages(prev => {
        const hasStreamingPlaceholder = prev.some(msg => msg.id === streamingMessageId);
        if (hasStreamingPlaceholder) {
          return prev.map(msg => (msg.id === streamingMessageId ? completedMessage : msg));
        }
        return [...prev, completedMessage];
      });

      setStreamingMessageId(null);

      if (currentSessionId) {
        addMessage(currentSessionId, completedMessage);
      }

      // Notify workbench of message completion (for right rail auto-update)
      onMessageComplete?.(completedMessage.id, traceId);

      // Notify evidence drawer for auto-follow
      if (evidenceDrawer) {
        evidenceDrawer.autoFollowToMessage(completedMessage.id, {
          evidence: evidence,
          routerDecision: routerDecision ? (routerDecision as unknown as ExtendedRouterDecision) : undefined,
          requestId: currentRequestId ?? undefined,
          traceId: traceId,
          proofDigest: completedMessage.proofDigest ?? null,
          isVerified: completedMessage.isVerified ?? undefined,
          verifiedAt: completedMessage.verifiedAt,
        });
      }

      if (traceId) {
        fetchVerificationReport(traceId, true);
      }
    },
    onError: (error) => {
      logger.error('Chat streaming error', { component: 'ChatInterface' }, error);
      // Remove streaming message on error
      if (streamingMessageId) {
        setMessages(prev =>
          prev.map((m) =>
            m.id === streamingMessageId
              ? {
                  ...m,
                  content: streamedText || m.content || 'Stream interrupted',
                  isStreaming: false,
                  streamError: error.message,
                  runMetadata: {
                    ...(m.runMetadata ?? {}),
                    requestId: m.runMetadata?.requestId ?? currentRequestId ?? undefined,
                    traceId: m.runMetadata?.traceId ?? currentRequestId ?? undefined,
                    planId: m.runMetadata?.planId ?? workspaceActiveState?.activePlanId ?? undefined,
                    manifestHashB3:
                      m.runMetadata?.manifestHashB3 ?? workspaceActiveState?.manifestHashB3 ?? undefined,
                  },
                }
              : m
          )
        );
        setStreamingMessageId(null);
      }
    },
    routingDeterminismMode: routingMode,
    adapterStrengthOverrides: strengthOverrides,
  });

  const streamingContent = streamMode === 'tokens'
    ? (streamedText || '').split(/\s+/).filter(Boolean).join(' ▪ ')
    : chunks.length > 0
      ? chunks.map(chunk => chunk.content).join(' | ')
      : streamedText;

  // Adapter state tracking hook (legacy - used when feature flag is off)
  const {
    adapterStates,
    isCheckingAdapters,
    allAdaptersReady,
    loadAllAdapters,
    showAdapterPrompt,
    dismissAdapterPrompt,
    continueWithUnready,
  } = useChatAdapterState({
    stackId: selectedStackId,
    enabled: !autoLoadEnabled, // Disable legacy hook when feature flag is on
  });

  // New model loading hooks (enabled when feature flag is on)
  const newModelLoadingState = useModelLoadingState({
    stackId: selectedStackId,
    tenantId,
    enabled: autoLoadEnabled,
  });

  const newModelLoader = useModelLoader();

  // Loading persistence for recovery after page refresh
  const {
    persistedState,
    persist: persistLoadingState,
    clear: clearLoadingState,
    isRecoverable,
  } = useChatLoadingPersistence({
    stackId: selectedStackId,
    enabled: autoLoadEnabled,
  });

  // Track if we've started loading (to persist only on start, not every update)
  const wasLoadingRef = useRef(false);

  // Auto-recover loading state after page refresh
  useEffect(() => {
    if (autoLoadEnabled && isRecoverable && persistedState && selectedStackId === persistedState.stackId) {
      logger.info('Recovering loading state after page refresh', {
        component: 'ChatInterface',
        stackId: persistedState.stackId,
        adaptersToLoad: persistedState.adaptersToLoad.length,
      });
      // Resume loading with the same stack
      newModelLoader.loadModels(persistedState.stackId);
    }
  }, [autoLoadEnabled, isRecoverable, persistedState, selectedStackId]); // Intentionally exclude newModelLoader to avoid re-triggering

  // Persist loading state only when loading starts (not on every update)
  useEffect(() => {
    const isCurrentlyLoading = newModelLoadingState.isLoading && !newModelLoadingState.error;

    if (autoLoadEnabled && isCurrentlyLoading && !wasLoadingRef.current) {
      // Loading just started
      persistLoadingState({
        stackId: selectedStackId,
        startedAt: Date.now(),
        adaptersToLoad: newModelLoadingState.loadingAdapters.map(a => a.adapterId),
        lastUpdated: Date.now(),
      });
    }

    wasLoadingRef.current = isCurrentlyLoading;
  }, [autoLoadEnabled, newModelLoadingState.isLoading, newModelLoadingState.error, selectedStackId, persistLoadingState, newModelLoadingState.loadingAdapters]);

  // Clear persistence when loading completes or errors
  useEffect(() => {
    if (autoLoadEnabled && (newModelLoadingState.overallReady || newModelLoadingState.error)) {
      clearLoadingState();
    }
  }, [autoLoadEnabled, newModelLoadingState.overallReady, newModelLoadingState.error, clearLoadingState]);

  // Screen reader announcements for loading state
  const { announcement: loadingAnnouncement } = useLoadingAnnouncements({
    state: {
      isLoading: newModelLoadingState.isLoading,
      progress: newModelLoadingState.progress,
      error: newModelLoadingState.error?.message ?? null,
      partialFailureCount: newModelLoadingState.failedAdapters.length,
      totalItems: newModelLoadingState.loadingAdapters.length + newModelLoadingState.readyAdapters.length + newModelLoadingState.failedAdapters.length,
    },
    enabled: autoLoadEnabled,
  });

  // Select which state to use based on feature flag
  const allReady = autoLoadEnabled ? newModelLoadingState.overallReady : allAdaptersReady;
  const adapterStateMap = autoLoadEnabled ? newModelLoadingState.adapterStates : adapterStates;
  const hasAdapters = adapterStateMap.size > 0;
  const baseModelReady = newModelLoadingState.baseModelReady;
  const baseModelStatus = newModelLoadingState.baseModelStatus;
  const baseModelName = newModelLoadingState.baseModelName;
  const canBypassModelGate = developerMode || kernelMode;
  // Guard against an overlay lingering after readiness or error: only treat as loading when not ready and no error.
  const isLoadingModels = autoLoadEnabled
    ? newModelLoadingState.isLoading
      && !newModelLoadingState.error
      && !(isBaseOnlyMode && baseModelReady)
      && !newModelLoadingState.overallReady
    : baseModelStatus === 'loading' || isCheckingAdapters;
  const modelGateActive = !baseModelReady && !(canBypassModelGate && modelGateBypass);

  const { metrics: systemMetrics } = useSystemMetrics('fast', autoLoadEnabled && isLoadingModels);
  const { workers: workerList } = useWorkers(selectedTenant, undefined, 'slow', autoLoadEnabled && isLoadingModels);
  const [backendSummary, setBackendSummary] = useState<{ name: string; mode?: string | null } | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!autoLoadEnabled || !isLoadingModels) {
      setBackendSummary(null);
      return;
    }

    (async () => {
      try {
        const backends = await apiClient.listBackends();
        if (cancelled) return;
        const defaultBackend = backends.default_backend || backends.backends?.find((b) => b.status === 'healthy')?.backend || backends.backends?.[0]?.backend || null;
        const matching = backends.backends?.find((b) => b.backend === defaultBackend);
        setBackendSummary(defaultBackend ? { name: defaultBackend, mode: matching?.mode ?? null } : null);
      } catch (err) {
        logger.debug('Failed to fetch backend list during boot', {
          component: 'ChatInterface',
          errorMessage: toError(err).message,
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [autoLoadEnabled, isLoadingModels]);

  const kernelInfo = useMemo(() => {
    const activeWorker = workerList.find((w) => w.status === 'running') || workerList[0];
    const used = systemMetrics?.gpu_memory_used_mb ?? null;
    const total = systemMetrics?.gpu_memory_total_mb ?? null;

    return {
      workerName: activeWorker?.worker_id || activeWorker?.id || null,
      workerStatus: activeWorker?.status ?? null,
      backend: backendSummary?.name ?? null,
      backendMode: backendSummary?.mode ?? null,
      baseModelName: newModelLoadingState.baseModelName,
      vramUsedMb: used,
      vramTotalMb: total,
      bootProgress: newModelLoadingState.progress,
    };
  }, [backendSummary, newModelLoadingState.baseModelName, newModelLoadingState.progress, systemMetrics?.gpu_memory_total_mb, systemMetrics?.gpu_memory_used_mb, workerList]);

  const [adapterTransitions, setAdapterTransitions] = useState<AdapterMountTransition[]>([]);
  const adapterStateSnapshotRef = useRef<Map<string, string>>(new Map());

  const adapterMountItems: AdapterMountItem[] = useMemo(
    () =>
      Array.from(adapterStateMap.values())
        .map((adapter: any) => ({
          adapterId: (adapter as any).adapterId ?? (adapter as any).id ?? '',
          name: (adapter as any).name ?? (adapter as any).adapterId ?? (adapter as any).id ?? 'Adapter',
          state: (adapter as any).state,
          isLoading: (adapter as any).isLoading,
        }))
        .filter((adapter) => adapter.adapterId),
    [adapterStateMap]
  );

  useEffect(() => {
    if (adapterMountItems.length === 0) {
      adapterStateSnapshotRef.current = new Map();
      return;
    }

    const updates: AdapterMountTransition[] = [];
    adapterMountItems.forEach((adapter) => {
      const previousState = adapterStateSnapshotRef.current.get(adapter.adapterId);
      if (previousState && previousState !== adapter.state) {
        updates.push({
          adapterId: adapter.adapterId,
          name: adapter.name,
          from: previousState,
          to: adapter.state,
          timestamp: Date.now(),
        });
      } else if (!previousState) {
        updates.push({
          adapterId: adapter.adapterId,
          name: adapter.name,
          from: undefined,
          to: adapter.state,
          timestamp: Date.now(),
        });
      }
    });

    if (updates.length) {
      setAdapterTransitions((prev) => [...updates, ...prev].slice(0, 8));
    }

    const snapshot = new Map<string, string>();
    adapterMountItems.forEach((adapter) => snapshot.set(adapter.adapterId, String(adapter.state)));
    adapterStateSnapshotRef.current = snapshot;
  }, [adapterMountItems]);

  // Auto-focus input when model becomes ready
  useEffect(() => {
    if (baseModelReady && inputRef.current) {
      inputRef.current.focus();
    }
  }, [baseModelReady]);

  const developerModeEnabled = developerMode || kernelMode;

  useEffect(() => {
    if (baseModelReady && modelGateBypass) {
      setModelGateBypass(false);
    }
  }, [baseModelReady, modelGateBypass]);

  useEffect(() => {
    if (!developerModeEnabled && modelGateBypass) {
      setModelGateBypass(false);
    }
  }, [developerModeEnabled, modelGateBypass]);

  useEffect(() => {
    if (developerModeEnabled) {
      setIsDebuggerOpen(true);
      if (isRouterActivityOpen) {
        setIsRouterActivityOpen(false);
      }
    } else {
      setIsDebuggerOpen(false);
    }
  }, [developerModeEnabled, isRouterActivityOpen, setIsRouterActivityOpen]);

  useEffect(() => {
    if (!verificationDialogTrace) return;
    fetchVerificationReport(verificationDialogTrace);
  }, [fetchVerificationReport, verificationDialogTrace]);

  // Inline model loading handlers (for InlineModelLoadingBlock)
  const handleInlineLoadModel = useCallback(async () => {
    try {
      await newModelLoader.loadModels(selectedStackId || '');
    } catch (err) {
      logger.error('Failed to load model from inline block', {
        component: 'ChatInterface',
      }, toError(err));
    }
  }, [newModelLoader, selectedStackId]);

  const handleInlineRetryLoad = useCallback(async () => {
    try {
      await newModelLoader.retryFailed();
    } catch (err) {
      logger.error('Failed to retry model load', {
        component: 'ChatInterface',
      }, toError(err));
    }
  }, [newModelLoader]);

  const handleSelectMessageWithVerification = useCallback(
    (messageId: string, traceId?: string) => {
      onMessageSelect?.(messageId, traceId);
      if (traceId) {
        setLatestTraceId(traceId);
        setVerificationDialogTrace(traceId);
      }
    },
    [onMessageSelect]
  );

  // Router decisions hook
  const {
    isLoadingDecision,
    fetchDecision,
    decisionHistory,
    lastDecision,
    clearDecisions,
  } = useChatRouterDecisions({
    stackId: selectedStackId,
  });

  useEffect(() => {
    if (!lastDecision) return;
    const label = lastDecision.adapterName || lastDecision.adapterId;
    toast.info(`Switched to ${label} based on reasoning...`, {
      id: `thought-swap-${lastDecision.messageId}`,
      duration: 4000,
    });
  }, [lastDecision]);

  // Reset session state when the tenant changes to avoid cross-tenant state bleed.
  useEffect(() => {
    const previousTenantId = previousTenantIdRef.current;
    previousTenantIdRef.current = tenantId;

    if (!previousTenantId) return;
    if (previousTenantId === tenantId) return;

    attemptedSessionLoadRef.current = null;
    clearSession();
    setEditingSessionId(null);
    setNewSessionName('');
    setInput('');
    setSearchQuery('');
    setStrengthOverrides({});
    setLatestTraceId(null);
    setVerificationReports({});
    setVerificationDialogTrace(null);
    setVerificationDialogError(null);
    clearDecisions();
    setIsHistoryOpen(false);
    setIsRouterActivityOpen(false);
    setIsArchivePanelOpen(false);
  }, [
    clearDecisions,
    clearSession,
    setIsArchivePanelOpen,
    setIsHistoryOpen,
    setIsRouterActivityOpen,
    tenantId,
  ]);

  // Export hook for chat session
  const { ExportButton } = useChatExport(
    {
      id: currentSessionId || '',
      name: 'Chat Session',
      stackId: selectedStackId,
      stackName: selectedStack?.name,
      collectionId: selectedCollectionId,
      messages,
      createdAt: new Date(),
      updatedAt: new Date(),
      tenantId,
    },
    messages
  );

  // Note: Default stack selection is now handled by parent (ChatPage)
  // This component is fully props-driven for stack selection

  useEffect(() => {
    setIsBaseOnlyMode(false);
  }, [selectedStackId]);

  // Load session if sessionId prop is provided
  useEffect(() => {
    if (sessionId && sessionId !== currentSessionId) {
      const session = getSession(sessionId);
      if (session) {
        setCurrentSessionId(sessionId);
        setMessages(session.messages);
        setSelectedStackId(session.stackId?.trim() ? session.stackId : null);
        return;
      }

      if (isLoadingSessions || isChatHistoryUnsupported) {
        return;
      }

      if (attemptedSessionLoadRef.current === sessionId) {
        return;
      }
      attemptedSessionLoadRef.current = sessionId;

      let cancelled = false;
      (async () => {
        try {
          const remoteSession = await apiClient.getChatSession(sessionId);
          const remoteMessages = await apiClient.getChatMessages(sessionId);
          if (cancelled) return;

          const messagesLocal: ChatMessage[] = remoteMessages
            .filter((m) => m.role === 'user' || m.role === 'assistant')
            .map((m) => {
              let metadata: Record<string, unknown> | undefined;
              try {
                metadata = m.metadata_json ? (JSON.parse(m.metadata_json) as Record<string, unknown>) : undefined;
              } catch {
                metadata = undefined;
              }
              return {
                id: m.id,
                role: m.role as 'user' | 'assistant',
                content: m.content,
                timestamp: new Date(m.timestamp),
                routerDecision: metadata?.routerDecision as ExtendedRouterDecision | null | undefined,
                unavailablePinnedAdapters: metadata?.unavailablePinnedAdapters as string[] | undefined,
                pinnedRoutingFallback: metadata?.pinnedRoutingFallback as 'stack_only' | 'partial' | undefined,
              };
            });

          setCurrentSessionId(sessionId);
          setMessages(messagesLocal);
          const stackId = remoteSession.stack_id?.trim() ? remoteSession.stack_id : null;
          setSelectedStackId(stackId);
        } catch (err) {
          if (cancelled) return;
          logger.warn('Failed to load chat session by id', { component: 'ChatInterface', sessionId }, toError(err));
          toast.error('Unable to load this chat session. It may be missing or you may not have access.');
          setCurrentSessionId(null);
          setMessages([]);
        }
      })();

      return () => {
        cancelled = true;
      };
    } else if (!sessionId && !currentSessionId && selectedStackId && !isLoadingSessions) {
      // Create new session if none exists and stack is selected
      const stack = stacks.find(s => s.id === selectedStackId);
      if (stack) {
        const documentCtx = documentContext
          ? { documentId: documentContext.documentId, documentName: documentContext.documentName }
          : undefined;
        (async () => {
          try {
            const newSession = await createSession(
              `Chat with ${stack.name || 'Stack'}`,
              selectedStackId,
              stack.name,
              effectiveCollectionId,
              documentCtx,
              documentCtx ? 'document' : 'general',
              sessionConfigForRequest
            );
            setCurrentSessionId(newSession.id);
            setMessages(newSession.messages);
          } catch {
            // error already logged
          }
        })();
      }
    }
    return undefined;
  }, [
    sessionId,
    currentSessionId,
    selectedStackId,
    stacks,
    isLoadingSessions,
    isChatHistoryUnsupported,
    getSession,
    createSession,
    effectiveCollectionId,
    documentContext,
    sessionConfigForRequest,
  ]);

  // Virtualized message list setup
  // Get scroll element dynamically from Radix ScrollArea
  const getScrollElement = useCallback(() => {
    if (scrollAreaRef.current) {
      const viewport = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (viewport) {
        return viewport as HTMLDivElement;
      }
    }
    return null;
  }, []);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement,
    estimateSize: () => 150, // Estimated height per message (will adjust dynamically)
    overscan: 5, // Render 5 extra items above/below viewport for smooth scrolling
  });

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (messages.length > 0 && virtualizer) {
      // Small delay to allow virtualizer to update sizes
      const timeoutId = setTimeout(() => {
        virtualizer.scrollToIndex(messages.length - 1, {
          align: 'end',
          behavior: 'smooth',
        });
      }, 100);
      return () => clearTimeout(timeoutId);
    }
    return undefined;
  }, [messages.length, virtualizer]);

  // Fetch evidence data for a message
  const fetchMessageEvidence = useCallback(async (messageId: string): Promise<EvidenceItem[]> => {
    try {
      return await apiClient.getMessageEvidence(messageId);
    } catch (err) {
      // apiClient already logs with request correlation; keep minimal fallback here
      logger.error('Failed to fetch message evidence', {
        component: 'ChatInterface',
        messageId,
      }, toError(err));
      return [];
    }
  }, []);

  const handleLoadBaseModelOnly = useCallback(() => {
    if (!autoLoadEnabled) {
      return;
    }
    setIsBaseOnlyMode(true);
    newModelLoader.loadModels(selectedStackId);
  }, [autoLoadEnabled, newModelLoader, selectedStackId]);

  const handleSend = useCallback(async () => {
    if (!input.trim() || isStreaming) return;

    if (modelGateActive) {
      toast.error('Base model is not ready. Please load it first.');
      return;
    }

    // Determine if we're in effective base-only mode (explicit or no stack selected)
    const effectiveBaseOnlyMode = isBaseOnlyMode || !selectedStackId;

    // Only block on adapter readiness when adapters are present and NOT in base-only mode
    if (!effectiveBaseOnlyMode && hasAdapters && !allReady) {
      toast.warning('Some adapters are not ready. Please load them first.');
      return;
    }

    // Resolve stack to adapter IDs (allow empty when base-only mode is active or no stack selected)
    const adapterIds: string[] = effectiveBaseOnlyMode
      ? []
      : Array.isArray(selectedStack?.adapter_ids)
        ? selectedStack.adapter_ids
        : Array.isArray(selectedStack?.adapters)
          ? selectedStack.adapters.map(a => a.id ?? a.adapter_id)
          : [];

    const attachedIds = effectiveBaseOnlyMode ? [] : attachedAdapters.map((adapter) => adapter.id);
    const mergedAdapterIds = effectiveBaseOnlyMode ? [] : Array.from(new Set([...adapterIds, ...attachedIds]));

    // Only block if user explicitly selected a stack but it has no adapters
    if (!mergedAdapterIds || mergedAdapterIds.length === 0) {
      if (selectedStackId && !effectiveBaseOnlyMode) {
        toast.error('Selected stack has no adapters. Select a different stack or use base model only.');
        return;
      }
    }

    if (!currentSessionId) {
      toast.error('Preparing chat session. Please wait and try again.');
      return;
    }

    // Clear input immediately
    const messageContent = input.trim();
    setInput('');

    // Send message using the streaming hook
    await sendMessage(messageContent, mergedAdapterIds);
  }, [
    allReady,
    baseModelReady,
    currentSessionId,
    hasAdapters,
    input,
    isBaseOnlyMode,
    modelGateActive,
    isStreaming,
    attachedAdapters,
    selectedStack,
    selectedStackId,
    sendMessage,
  ]);

  const attachFromUI = useCallback((adapter: SuggestedAdapter) => {
    const forceReplace = conflictState?.candidateId === adapter.id;
    const result = attachWithResolution(adapter, 'manual', { forceReplace });

    if (!result.attached && result.resolution?.conflicts.length && !forceReplace) {
      const conflictsLabel = result.resolution.conflicts.map((c) => c.id).join(', ');
      toast.warning(`Conflicts with ${conflictsLabel}. Attach again to replace.`);
    }

    if (result.attached && forceReplace && result.resolution?.conflicts.length) {
      const replaced = result.resolution.conflicts.map((c) => c.id).join(', ');
      toast.success(`Replaced ${replaced} with ${adapter.id}`);
    }

    return result.attached;
  }, [attachWithResolution, conflictState?.candidateId]);

  const handleAcceptSuggestion = useCallback(() => {
    const target = suggestedAdapters[0] ?? bestSuggestion;
    if (!target) return;
    attachFromUI(target);
  }, [attachFromUI, bestSuggestion, suggestedAdapters]);

  const handleDismissSuggestion = useCallback((adapterId?: string) => {
    const targetId = adapterId ?? suggestedAdapters[0]?.id ?? bestSuggestion?.id;
    if (!targetId) return;
    muteAdapter(targetId);
  }, [bestSuggestion?.id, muteAdapter, suggestedAdapters]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab' && (suggestedAdapters.length > 0 || bestSuggestion)) {
      e.preventDefault();
      handleAcceptSuggestion();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleAutoAttachToggle = useCallback((enabled: boolean) => {
    setAutoAttachEnabled(enabled);
  }, []);

  const handleRemoveAttachment = useCallback(
    (adapterId: string, mute = false) => {
      removeAttachedAdapter(adapterId);
      if (mute) {
        muteAdapter(adapterId);
      }
    },
    [muteAdapter, removeAttachedAdapter]
  );

  const activeSuggestion = useMemo(
    () => bestSuggestion ?? suggestedAdapters[0] ?? null,
    [bestSuggestion, suggestedAdapters]
  );
  const snapMatched = useMemo(
    () => Boolean(lastAttachedAdapterId && activeSuggestion && lastAttachedAdapterId === activeSuggestion.id),
    [activeSuggestion, lastAttachedAdapterId]
  );
  const snapVisual = useMemo(
    () => snapMatched && (activeSuggestion?.confidence ?? 0) >= MAGNET_CONFIDENCE_THRESHOLD,
    [activeSuggestion?.confidence, snapMatched]
  );

  const magnetDetails = useMemo(() => {
    if (!activeSuggestion) {
      return { color: null as string | null, auraLabel: null as string | null, confidence: 0 };
    }
    const classification = classifyMagnet(activeSuggestion);
    return {
      color: classification.color,
      auraLabel: classification.label,
      confidence: Math.min(1, Math.max(0, activeSuggestion.confidence ?? 0)),
    };
  }, [activeSuggestion]);

  const activeConflict = useMemo(
    () => (conflictState && activeSuggestion && conflictState.candidateId === activeSuggestion.id ? conflictState : null),
    [activeSuggestion, conflictState]
  );

  const showMagnetField = useMemo(
    () => Boolean(magnetDetails.color && magnetDetails.confidence > 0.55 && !snapMatched),
    [magnetDetails.color, magnetDetails.confidence, snapMatched]
  );

  const magnetGlowStyle = useMemo(() => {
    if (!magnetDetails.color || !showMagnetField) return undefined;
    return {
      boxShadow: `0 0 0 1px ${colorWithAlpha(magnetDetails.color, 0.35)}, 0 0 24px ${colorWithAlpha(
        magnetDetails.color,
        0.22
      )}`,
    };
  }, [magnetDetails.color, showMagnetField]);

  // selectedStack is memoized earlier in the component
  const adapterCount = selectedStack?.adapter_ids?.length ?? selectedStack?.adapters?.length ?? 0;
  const stackLabel = selectedStack?.name || 'No stack selected';
  const isDefaultStack = Boolean(
    defaultStack?.id && selectedStack?.id && selectedStack.id === defaultStack.id
  );
  const stackDetails = selectedStack?.lifecycle_state ?? selectedStack?.description ?? null;
  const baseModelDescriptor = baseModelName || workspaceActiveState?.activeBaseModelId || 'Base model';
  const baseModelLabel = baseModelReady
    ? `${baseModelDescriptor}${isBaseOnlyMode || !hasAdapters ? ' ready (no adapters)' : ' ready'}`
    : baseModelStatus === 'loading'
      ? 'Loading base model...'
      : 'Base model not ready';
  const rightPanelsOpen = isRouterActivityOpen || isDebuggerOpen;
  const activeTraceId = currentRequestId || latestTraceId;
  const activeVerification = activeTraceId ? verificationReports[activeTraceId] : undefined;
  const activeRunHeadHash =
    activeVerification?.run_head_hash?.computed_hex ||
    activeVerification?.run_head_hash?.expected_hex ||
    null;

  useEffect(() => {
    if (!activeTraceId || isStreaming) return;
    fetchVerificationReport(activeTraceId, true);
  }, [activeTraceId, fetchVerificationReport, isStreaming]);

  // Get recent sessions (last 10, sorted by updatedAt), filtered by search query
  const recentSessions = useMemo(() => {
    let filtered = sessions;

    // Filter by search query
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      filtered = sessions.filter(session =>
        session.name.toLowerCase().includes(query) ||
        session.messages.some(msg => msg.content.toLowerCase().includes(query))
      );
    }

    return filtered
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime())
      .slice(0, 10);
  }, [sessions, searchQuery]);

  // Get preview text from first user message
  const getSessionPreview = (session: typeof sessions[0]) => {
    const firstUserMessage = session.messages.find(m => m.role === 'user');
    if (firstUserMessage) {
      return firstUserMessage.content.slice(0, 50) + (firstUserMessage.content.length > 50 ? '...' : '');
    }
    return 'No messages yet';
  };

  const handleLoadSession = useCallback((sessionId: string) => {
    if (guardChatHistory()) {
      return;
    }

    const session = getSession(sessionId);
    if (session) {
      setMessages(session.messages);
      setCurrentSessionId(sessionId);
      const nextStackId = session.stackId?.trim() ? session.stackId : null;
      if (session.collectionId !== undefined) {
        setSelectedCollectionId(session.collectionId);
      }
      const config = (session.metadata as Record<string, unknown> | undefined)?.chat_session_config as
        | { stack_id?: string; routing_determinism_mode?: string; adapter_strength_overrides?: Record<string, number> }
        | undefined;
      if (nextStackId) {
        setSelectedStackId(nextStackId);
      } else if (config?.stack_id) {
        setSelectedStackId(config.stack_id);
      } else {
        setSelectedStackId(null);
      }
      if (config?.routing_determinism_mode) {
        setRoutingMode(
          config.routing_determinism_mode === 'adaptive' ? 'adaptive' : 'deterministic'
        );
      }
      if (config?.adapter_strength_overrides) {
        setStrengthOverrides(config.adapter_strength_overrides);
      }
      setIsHistoryOpen(false);
    }
  }, [getSession, guardChatHistory]);

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
      const newSession = await createSession(
        `Session ${new Date().toLocaleString()}`,
        selectedStackId,
        selectedStack?.name,
        effectiveCollectionId,
        documentCtx,
        documentCtx ? 'document' : 'general',
        sessionConfigForRequest
      );
      setCurrentSessionId(newSession.id);
      setMessages(newSession.messages);
      setNewSessionName('');
      setIsHistoryOpen(false);
      toast.success('New session created');
    } catch {
      // error already surfaced
    }
  }, [createSession, documentContext, effectiveCollectionId, guardChatHistory, selectedStackId, sessionConfigForRequest, stacks]);

  const handleDeleteSession = useCallback((sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (guardChatHistory()) {
      return;
    }
    if (window.confirm('Are you sure you want to delete this session?')) {
      deleteSession(sessionId);
      if (currentSessionId === sessionId) {
        setCurrentSessionId(null);
        setMessages([]);
      }
      toast.success('Session deleted');
    }
  }, [currentSessionId, deleteSession, guardChatHistory]);

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
    
    updateSession(sessionId, { name: trimmedName });
    setEditingSessionId(null);
    toast.success('Session renamed');
  }, [guardChatHistory, updateSession]);

  // Handler for viewing document evidence
  const handleViewDocumentClick = useCallback((documentId: string, pageNumber?: number, highlightText?: string) => {
    // Use the provided callback if available
    if (onViewDocument) {
      onViewDocument(documentId, pageNumber, highlightText);
    } else {
      // Fallback behavior: log and show toast
      logger.info('View document requested', {
        component: 'ChatInterface',
        documentId,
        pageNumber,
        highlightText,
      });
      toast.info(`Opening document ${documentId}${pageNumber ? ` (page ${pageNumber})` : ''}`);
    }
  }, [onViewDocument]);

  // Handler for collection change
  const handleCollectionChange = useCallback(async (collectionId: string) => {
    if (guardChatHistory()) {
      return;
    }

    const newCollectionId = collectionId === 'none' ? null : collectionId;
    setSelectedCollectionId(newCollectionId);

    // Update current session's collection if session exists
    if (currentSessionId) {
      try {
        await updateSessionCollection(currentSessionId, newCollectionId);
        toast.success(newCollectionId ? 'Knowledge base selected' : 'Context cleared');
      } catch (error) {
        logger.error('Failed to update session collection', {
          component: 'ChatInterface',
          sessionId: currentSessionId,
          collectionId: newCollectionId,
        }, toError(error));
        toast.error('Failed to update knowledge base');
      }
    }
  }, [currentSessionId, guardChatHistory, updateSessionCollection]);

  // Get selected collection name for display
  const selectedCollectionName = useMemo(() => {
    if (!selectedCollectionId) return 'No knowledge base';
    const collection = collections.find(c => c.collection_id === selectedCollectionId);
    return collection?.name || 'Unknown';
  }, [selectedCollectionId, collections]);

  // Compute unavailable pinned adapters from messages
  const { unavailablePinnedAdapters, pinnedRoutingFallback } = useMemo(() => {
    // Find the latest assistant message with unavailable pinned adapters
    const affectedMessages = messages.filter(
      msg => msg.role === 'assistant' && msg.unavailablePinnedAdapters && msg.unavailablePinnedAdapters.length > 0
    );

    if (affectedMessages.length === 0) {
      return { unavailablePinnedAdapters: [], pinnedRoutingFallback: undefined };
    }

    // Get the latest affected message
    const latestMessage = affectedMessages[affectedMessages.length - 1];

    return {
      unavailablePinnedAdapters: latestMessage.unavailablePinnedAdapters || [],
      pinnedRoutingFallback: latestMessage.pinnedRoutingFallback,
    };
  }, [messages]);

  // Reset banner dismissed state when new affected messages arrive
  useEffect(() => {
    if (unavailablePinnedAdapters.length > 0 && bannerDismissed) {
      // Check if we have new messages since banner was dismissed
      // For simplicity, we reset on any change to unavailable adapters
      setBannerDismissed(false);
    }
  }, [unavailablePinnedAdapters]); // Intentionally exclude bannerDismissed to avoid loop

  return (
    <EvidenceDrawerProvider>
    <div className="flex flex-col h-full relative">
      {/* Screen reader announcements for loading state */}
      {autoLoadEnabled && loadingAnnouncement && (
        <div
          role="status"
          aria-live="polite"
          aria-atomic="true"
          className="sr-only"
        >
          {loadingAnnouncement}
        </div>
      )}

      {/* Pre-Chat Adapter Loading Prompt */}
      <PreChatAdapterPrompt
        open={autoLoadEnabled ? false : showAdapterPrompt} // Don't show legacy prompt when feature flag is on
        onOpenChange={dismissAdapterPrompt}
        adapters={Array.from(adapterStates.values()).map(state => ({
          id: state.adapterId,
          name: state.name,
          state: state.state,
          isLoading: state.isLoading,
          error: state.error,
        }))}
        onLoadAll={loadAllAdapters}
        onContinueAnyway={continueWithUnready}
        isLoading={isCheckingAdapters}
        // Model loading props (when feature flag is enabled)
        modelStatus={autoLoadEnabled ? newModelLoadingState.baseModelStatus : undefined}
        modelName={autoLoadEnabled ? newModelLoadingState.baseModelName || undefined : undefined}
        isModelLoading={autoLoadEnabled ? newModelLoadingState.baseModelStatus === 'loading' : undefined}
        onLoadAndChat={autoLoadEnabled ? handleLoadBaseModelOnly : undefined}
      />

      <div className={`px-4 pt-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <Badge variant="secondary">Workspace: {tenantId}</Badge>
          <Badge variant={baseModelReady ? 'outline' : 'destructive'}>
            Base: {baseModelDescriptor}
            {!baseModelReady ? ' (not loaded)' : ''}
          </Badge>
          <Badge variant="outline">Adapters: {adapterCount || 0}</Badge>
          <Badge variant="outline">Plan: {workspaceActiveState?.activePlanId || 'none'}</Badge>
          {workspaceActiveState?.manifestHashB3 && (
            <Badge variant="outline">Manifest {workspaceActiveState.manifestHashB3}</Badge>
          )}
        </div>
      </div>

      {modelGateActive && (
        <div className={`px-4 pt-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
          <Card className="border-amber-500/70 bg-amber-50/40 dark:bg-amber-950/20">
            <CardHeader className="pb-2 flex items-start justify-between">
              <div>
                <CardTitle className="text-base">Base model required</CardTitle>
                <p className="text-xs text-muted-foreground">
                  Load an active base model before running chat. Workspace guard prevents accidental runs.
                </p>
              </div>
              {workspaceStateLoading ? (
                <Loader2 className="h-4 w-4 animate-spin text-amber-600" />
              ) : workspaceActiveState?.activeBaseModelId ? (
                <Badge variant="outline" className="text-xs">
                  Target: {workspaceActiveState.activeBaseModelId}
                </Badge>
              ) : null}
            </CardHeader>
            <CardContent className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" onClick={handleInlineLoadModel} disabled={isLoadingModels}>
                  Load base model
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void fetchWorkspaceActiveState()}
                  disabled={workspaceStateLoading}
                  className="gap-2"
                >
                  <RefreshCw className={`h-4 w-4 ${workspaceStateLoading ? 'animate-spin' : ''}`} />
                  Refresh
                </Button>
                {canBypassModelGate && (
                  <div className="flex items-center gap-2">
                    <Switch
                      id="developer-bypass"
                      checked={modelGateBypass}
                      onCheckedChange={setModelGateBypass}
                    />
                    <label htmlFor="developer-bypass" className="text-xs text-muted-foreground">
                      Dev bypass
                    </label>
                  </div>
                )}
              </div>
              <div className="text-xs text-muted-foreground">
                Status: {baseModelLabel}
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Chat Loading Overlay (when feature flag is enabled) */}
      {autoLoadEnabled && isLoadingModels && (
        <ChatLoadingOverlay
          loadingState={{
            adapters: [
              // Map loading adapters
              ...newModelLoadingState.loadingAdapters.map(adapter => ({
                id: adapter.adapterId,
                name: adapter.name,
                status: 'loading' as const,
                error: adapter.errorMessage,
                progress: 50, // Approximate progress for loading adapters
                estimatedTimeRemaining: 8, // Use constant from types
              })),
              // Map ready adapters
              ...newModelLoadingState.readyAdapters.map(adapter => ({
                id: adapter.adapterId,
                name: adapter.name,
                status: 'ready' as const,
                error: undefined,
                progress: 100,
                estimatedTimeRemaining: undefined,
              })),
              // Map failed adapters
              ...newModelLoadingState.failedAdapters.map(adapter => ({
                id: adapter.adapterId,
                name: adapter.name,
                status: 'failed' as const,
                error: adapter.errorMessage,
                progress: undefined,
                estimatedTimeRemaining: undefined,
              })),
            ],
            overallProgress: newModelLoadingState.progress,
            estimatedTimeRemaining: newModelLoadingState.estimatedTimeRemaining ?? undefined,
          }}
          onLoadAll={() => newModelLoader.loadModels(selectedStackId)}
          onCancel={() => newModelLoader.cancelLoading()}
          kernelInfo={kernelInfo}
        />
      )}

      {/* Chat Error Display (when feature flag is enabled and error occurred) */}
      {autoLoadEnabled && newModelLoadingState.error && !isLoadingModels && (
        <div className="absolute inset-x-4 top-4 z-20">
          <ChatErrorDisplay
            error={newModelLoadingState.error}
            onRetry={() => newModelLoader.loadModels(selectedStackId)}
            currentRetry={newModelLoadingState.error.retryCount}
            maxRetries={newModelLoadingState.error.maxRetries}
            alternativeActions={[
              {
                label: 'Change Stack',
                onClick: () => {
                  // Focus and click the stack selector to open it
                  if (stackSelectorRef.current) {
                    stackSelectorRef.current.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    stackSelectorRef.current.focus();
                    stackSelectorRef.current.click();
                  }
                },
                variant: 'outline',
              },
            ]}
          />
        </div>
      )}

      {/* History Sidebar */}
      {isHistoryOpen && (
        <SectionErrorBoundary sectionName="Session History">
          <div className="absolute left-0 top-0 bottom-0 w-80 bg-background border-r z-10 flex flex-col">
            <div className="border-b px-4 py-3 flex items-center justify-between">
              <h3 className="font-semibold text-sm flex items-center gap-2">
                <History className="h-4 w-4" />
                Conversation History
              </h3>
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsArchivePanelOpen(true)}
                  aria-label="Open archive"
                  title="View archived sessions"
                >
                  <Archive className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsHistoryOpen(false)}
                  aria-label="Close history"
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            </div>

            {/* Search Bar */}
            {/* Note: ChatSearchBar supports both controlled mode (for local filtering) and 
                API-based search (via useChatSearch hook). The value prop filters local sessions
                via recentSessions memo, while the component also provides API search results. */}
            <div className="px-4 py-2 border-b">
              <ChatSearchBar
                value={searchQuery}
                onChange={setSearchQuery}
                onSelectSession={(sessionId) => handleLoadSession(sessionId)}
                onSelectMessage={(sessionId, messageId) => {
                  handleLoadSession(sessionId);
                  // TODO: After loading, scroll to the specific message
                  // For now, just load the session - message scrolling can be added later
                  if (messageId) {
                    logger.info('Search navigated to message', { sessionId, messageId });
                  }
                }}
                placeholder="Search sessions..."
              />
            </div>

            {/* Create New Session */}
            <div className="border-b px-4 py-2">
              <Button
                variant="outline"
                size="sm"
                className="w-full"
                onClick={handleCreateSession}
                disabled={!selectedStackId}
              >
                <Plus className="h-4 w-4 mr-2" />
                New Session
              </Button>
            </div>

            <ScrollArea className="flex-1">
              <div className="p-2 space-y-1">
                {isLoadingSessions ? (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    Loading sessions...
                  </div>
                ) : recentSessions.length === 0 ? (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    No conversation history
                  </div>
                ) : (
                  recentSessions.map(session => (
                    <div
                      key={session.id}
                      className={`group p-3 rounded-lg border cursor-pointer transition-colors hover:bg-muted ${
                        currentSessionId === session.id ? 'bg-muted border-primary' : ''
                      }`}
                      onClick={() => handleLoadSession(session.id)}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          {editingSessionId === session.id ? (
                            <Input
                              value={newSessionName}
                              onChange={(e) => setNewSessionName(e.target.value)}
                              onBlur={() => {
                                if (newSessionName.trim()) {
                                  handleRenameSession(session.id, newSessionName.trim());
                                } else {
                                  setEditingSessionId(null);
                                }
                              }}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter' && newSessionName.trim()) {
                                  handleRenameSession(session.id, newSessionName.trim());
                                } else if (e.key === 'Escape') {
                                  setEditingSessionId(null);
                                }
                              }}
                              className="h-7 text-sm mb-1"
                              autoFocus
                              onClick={(e) => e.stopPropagation()}
                            />
                          ) : (
                            <>
                              <div className="flex items-center justify-between">
                                <p className="text-sm font-medium truncate">{session.name}</p>
                                <div className="flex items-center gap-1 ml-2" onClick={(e) => e.stopPropagation()}>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                                    onClick={(e) => handleDeleteSession(session.id, e)}
                                    aria-label={`Delete session ${session.name}`}
                                  >
                                    <Trash2 className="h-3 w-3 text-destructive" />
                                  </Button>
                                  <ChatSessionActions
                                    sessionId={session.id}
                                    tenantId={tenantId}
                                    onRename={() => {
                                      setEditingSessionId(session.id);
                                      setNewSessionName(session.name);
                                    }}
                                    onManageTags={() => setTagsDialogSessionId(session.id)}
                                    onSetCategory={() => setCategoryDialogSessionId(session.id)}
                                    onShare={() => setShareDialogSessionId(session.id)}
                                  />
                                </div>
                              </div>
                              <p className="text-xs text-muted-foreground mt-1 line-clamp-2">
                                {getSessionPreview(session)}
                              </p>
                              <div className="flex items-center gap-2 mt-2">
                                <span className="text-xs text-muted-foreground">
                                  {session.messages.length} message{session.messages.length !== 1 ? 's' : ''}
                                </span>
                                <span className="text-xs text-muted-foreground">•</span>
                                <span className="text-xs text-muted-foreground">
                                  {new Date(session.updatedAt).toLocaleDateString()}
                                </span>
                              </div>
                            </>
                          )}
                        </div>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </ScrollArea>
          </div>
        </SectionErrorBoundary>
      )}

      {/* Router Activity Sidebar */}
      <SectionErrorBoundary sectionName="Router Activity">
        <RouterActivitySidebar
          open={isRouterActivityOpen}
          onClose={() => setIsRouterActivityOpen(false)}
          stackId={selectedStackId}
          decisions={decisionHistory}
          lastDecision={lastDecision}
          onClear={clearDecisions}
        />
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="Neural Debugger">
        <NeuralDebuggerPanel
          open={isDebuggerOpen}
          onClose={() => setIsDebuggerOpen(false)}
          tokens={chunks}
          adapterId={lastDecision?.adapterId ?? (hasAdapters ? 'Adapter warming' : 'Base model')}
          routerConfidence={lastDecision?.confidence ?? null}
          runHeadHash={activeRunHeadHash}
          traceId={activeTraceId ?? undefined}
          verificationReport={activeVerification ?? null}
          onOpenVerification={activeTraceId ? () => setVerificationDialogTrace(activeTraceId) : undefined}
          onRefreshVerification={activeTraceId ? () => fetchVerificationReport(activeTraceId) : undefined}
        />
      </SectionErrorBoundary>

      {/* Currently Loaded Panel */}
      <div className={`px-4 mt-2 ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
        <Card>
          <CardHeader className="flex flex-row items-start justify-between space-y-0">
            <div className="space-y-1">
              <CardTitle className="text-base">Currently Loaded</CardTitle>
              <p className="text-xs text-muted-foreground">
                Stack context for this chat session.
              </p>
              {isDefaultStack && (
                <Badge
                  variant="secondary"
                  className="w-fit"
                  aria-label="This is the default adapter stack for your workspace"
                >
                  Default stack for this workspace
                </Badge>
              )}
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowContext(!showContext)}
              aria-label={showContext ? 'Hide stack context' : 'Show stack context'}
            >
              {showContext ? 'Hide' : 'Show'}
            </Button>
          </CardHeader>
          {showContext && (
            <CardContent className="grid grid-cols-1 sm:grid-cols-4 gap-3">
              <div>
                <p className="text-xs text-muted-foreground">Stack</p>
                <p className="font-medium truncate">{stackLabel}</p>
                {stackDetails && (
                  <p className="text-xs text-muted-foreground truncate">{stackDetails}</p>
                )}
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Adapters</p>
                <p className="font-medium">{adapterCount || '—'}</p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Knowledge Base</p>
                <p className="font-medium truncate">{selectedCollectionName}</p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Base model</p>
                <p className="font-medium text-muted-foreground">{baseModelLabel}</p>
              </div>
              <div className="sm:col-span-4 space-y-2">
                <div className="flex items-center justify-between">
                  <p className="text-xs text-muted-foreground">Active adapters</p>
                  <p className="text-xs text-muted-foreground">
                    Strength overrides (0.0–2.0, default 1.0)
                  </p>
                </div>
                {adapterList.length === 0 ? (
                  <p className="text-xs text-muted-foreground">No adapters selected</p>
                ) : (
                  <div className="space-y-3">
                    {adapterList.map(adapter => (
                      <div key={adapter.id} className="flex items-center gap-3">
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium truncate">{adapter.name}</p>
                          <p className="text-xs text-muted-foreground truncate">
                            {[adapter.tier, adapter.domain].filter(Boolean).join(' • ') || 'Adapter'}
                          </p>
                        </div>
                        <div className="flex items-center gap-2">
                          <input
                            type="range"
                            min={0}
                            max={2}
                            step={0.05}
                            value={strengthOverrides[adapter.id] ?? 1}
                            onChange={(e) => handleStrengthChange(adapter.id, Number(e.target.value))}
                            aria-label={`Strength for ${adapter.name}`}
                            className="w-32"
                          />
                          <span className="text-xs tabular-nums">
                            {(strengthOverrides[adapter.id] ?? 1).toFixed(2)}x
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              {attachedAdapters.length > 0 && (
                <div className="sm:col-span-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-xs text-muted-foreground">Magnet attachments:</span>
                    {attachedAdapters.map((adapter) => (
                      <AdapterAttachmentChip
                        key={`${adapter.id}-active`}
                        adapterId={adapter.id}
                        confidence={adapter.confidence}
                        onRemove={() => handleRemoveAttachment(adapter.id, true)}
                        flash={lastAttachedAdapterId === adapter.id}
                      />
                    ))}
                  </div>
                </div>
              )}
            </CardContent>
          )}
        </Card>
      </div>

      {/* Header with stack selector */}
      <div className={`border-b px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              if (guardChatHistory()) {
                return;
              }
              setIsHistoryOpen(!isHistoryOpen);
            }}
            aria-label={isChatHistoryUnsupported ? chatHistoryUnsupportedMessage : isHistoryOpen ? "Close history" : "Open history"}
          >
            {isHistoryOpen ? (
              <ChevronLeft className="h-4 w-4" />
            ) : (
              <History className="h-4 w-4" />
            )}
          </Button>
          {isChatHistoryUnsupported && (
            <span className="text-xs text-muted-foreground">
              {chatHistoryUnsupportedMessage}
            </span>
          )}
          <Layers className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
          {documentContext && (
            <Badge variant="secondary" className="gap-1">
              <FileText className="h-3 w-3" />
              {documentContext.documentName}
            </Badge>
          )}
          {datasetContext && (
            <Badge variant="secondary" className="gap-1">
              <Database className="h-3 w-3" />
              {datasetContext.datasetName}
            </Badge>
          )}
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Stack:</span>
            <Select
              value={selectedStackId}
              onValueChange={setSelectedStackId}
              aria-label="Select adapter stack"
              aria-describedby={stacks.length === 0 ? "no-stacks-hint" : undefined}
            >
              <SelectTrigger
                ref={stackSelectorRef}
                className="w-[calc(var(--base-unit)*75)]"
                aria-label="Select adapter stack"
              >
                <SelectValue placeholder="Select a stack" />
              </SelectTrigger>
              <SelectContent>
                {stacks.map(stack => (
                  <SelectItem key={stack.id} value={stack.id}>
                    {stack.name}
                    {stack.description && (
                      <span className="text-muted-foreground ml-2">
                        ({stack.description})
                      </span>
                    )}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {stacks.length === 0 && (
              <span id="no-stacks-hint" className="sr-only">
                No adapter stacks available. Please create a stack first.
              </span>
            )}
          </div>
          {autoLoadEnabled && (
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleLoadBaseModelOnly}
                disabled={!selectedStackId || isLoadingModels}
              >
                Load base model and chat without adapters
              </Button>
              <span className="text-xs text-muted-foreground">{baseModelLabel}</span>
            </div>
          )}
          {selectedStack && (
            <Badge variant="outline" aria-label={`${selectedStack.adapter_ids?.length || 0} adapters in stack`}>
              {selectedStack.adapter_ids?.length || 0} adapter
              {(selectedStack.adapter_ids?.length || 0) !== 1 ? 's' : ''}
            </Badge>
          )}
          {/* Adapter loading status indicator */}
          {adapterStates.size > 0 && (
            <AdapterLoadingStatus
              stackId={selectedStackId}
              adapters={Array.from(adapterStates.values()).map(state => ({
                id: state.adapterId,
                name: state.name,
                state: state.state,
                isLoading: state.isLoading,
                error: state.error,
              }))}
              compact
            />
          )}
          <Database className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Knowledge Base:</span>
            <Select
              value={selectedCollectionId || 'none'}
              onValueChange={handleCollectionChange}
              aria-label="Select knowledge base"
            >
              <SelectTrigger
                className="w-[calc(var(--base-unit)*50)]"
                aria-label="Select knowledge base"
              >
                <SelectValue placeholder="No knowledge base" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">No knowledge base</SelectItem>
                {collections.map(collection => (
                  <SelectItem key={collection.collection_id} value={collection.collection_id}>
                    {collection.name}
                    {collection.document_count > 0 && (
                      <span className="text-muted-foreground ml-2">
                        ({collection.document_count} docs)
                      </span>
                    )}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              if (isDebuggerOpen) {
                setIsDebuggerOpen(false);
              }
              setIsRouterActivityOpen(!isRouterActivityOpen);
            }}
            aria-label={isRouterActivityOpen ? "Close router activity" : "Open router activity"}
            title="View router decision history"
          >
            <Activity className="h-4 w-4" />
          </Button>
          <Button
            variant={isDebuggerOpen ? 'secondary' : 'ghost'}
            size="sm"
            onClick={() => {
              if (isRouterActivityOpen) {
                setIsRouterActivityOpen(false);
              }
              setIsDebuggerOpen((open) => !open);
            }}
            aria-label={isDebuggerOpen ? "Close neural debugger" : "Open neural debugger"}
            title="Live neural debugger"
          >
            <Bug className="h-4 w-4" />
          </Button>
          {currentSessionId && messages.length > 0 && (
            <ExportButton />
          )}
          </div>
        </div>

        {/* Session Tags */}
        {currentSessionId && (
          <div className="mt-2">
            <ChatTagsManager sessionId={currentSessionId} />
          </div>
        )}

        {adapterMountItems.length > 0 && (
          <div className={`px-4 pb-2 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
            <AdapterMountIndicators
              adapters={adapterMountItems}
              transitions={adapterTransitions}
              activeAdapterId={lastDecision?.adapterId}
              isStreaming={isStreaming}
            />
          </div>
        )}
      </div>

      {/* Missing Pinned Adapters Banner */}
      {unavailablePinnedAdapters.length > 0 && !bannerDismissed && (
        <div className={`px-4 pt-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
          <MissingPinnedAdaptersBanner
            unavailablePinnedAdapters={unavailablePinnedAdapters}
            pinnedRoutingFallback={pinnedRoutingFallback}
            onDismiss={() => setBannerDismissed(true)}
          />
        </div>
      )}

      {/* Messages area */}
      <ScrollArea
        className={`flex-1 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}
        ref={scrollAreaRef}
        aria-label="Chat messages"
        role="log"
        aria-live="polite"
        aria-atomic="false"
      >
        <SectionAsyncBoundary section="chat-messages">
          <div className="py-4">
            {messages.length === 0 ? (
              <div
                className="flex items-center justify-center h-full text-muted-foreground"
                role="status"
                aria-live="polite"
              >
                <div className="text-center">
                  <Layers className="h-12 w-12 mx-auto mb-4 opacity-50" aria-hidden="true" />
                  <p className="text-lg font-medium">Start a conversation</p>
                  <p className="text-sm mt-1">
                    {documentContext
                      ? `I'm ready to help you with "${documentContext.documentName}". Ask me anything about this document.`
                      : datasetContext
                        ? `I'm ready to help you with the "${datasetContext.datasetName}" dataset. Ask me anything about this data.`
                        : 'Select a stack and send a message to begin'}
                  </p>
                </div>
              </div>
            ) : (
              <div
                style={{
                  height: `${virtualizer.getTotalSize()}px`,
                  width: '100%',
                  position: 'relative',
                }}
              >
                {virtualizer.getVirtualItems().map((virtualItem) => {
                  const message = messages[virtualItem.index];
                  return (
                    <div
                      key={message.id}
                      data-index={virtualItem.index}
                      ref={virtualizer.measureElement}
                      style={{
                        position: 'absolute',
                        top: 0,
                        left: 0,
                        width: '100%',
                        transform: `translateY(${virtualItem.start}px)`,
                      }}
                    >
                      <div className="space-y-2">
                        <ChatMessageComponent
                          message={
                            // Update streaming message with current streamed text
                            message.id === streamingMessageId
                              ? {
                                  ...message,
                                  content: streamingContent,
                                  tokenStream: chunks.map((chunk) => ({
                                    token: chunk.token,
                                    logprob: chunk.logprob,
                                    routerScore: chunk.routerScore,
                                    index: chunk.index,
                                    timestamp: chunk.timestamp,
                                  })),
                                }
                              : message
                          }
                          onViewDocument={handleViewDocumentClick}
                          onSelect={handleSelectMessageWithVerification}
                          isSelected={selectedMessageId === message.id}
                          developerMode={developerModeEnabled}
                          kernelMode={kernelMode}
                        />
                        {message.role === 'assistant' && (
                          <RunEvidencePanel
                            evidence={message.runMetadata}
                            traceId={message.traceId}
                            fallbackPolicyMask={message.routerDecision?.policy_mask_digest}
                            fallbackPlanId={workspaceActiveState?.activePlanId ?? undefined}
                            manifestFallback={workspaceActiveState?.manifestHashB3 ?? undefined}
                            workspaceIdFallback={tenantId}
                            showSeedValue={developerModeEnabled}
                            onExport={() => handleExportRunEvidence(message)}
                            pending={message.isStreaming}
                          />
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
            {isLoadingDecision && (
              <SectionAsyncBoundary section="chat-streaming">
                <div className="text-xs text-muted-foreground px-4" role="status" aria-live="polite">
                  Loading router decision details...
                </div>
              </SectionAsyncBoundary>
            )}
          </div>
        </SectionAsyncBoundary>
      </ScrollArea>

      {/* Input area */}
      <div className={`border-t px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${rightPanelsOpen ? 'mr-96' : ''}`}>
        {/* Inline model loading block - shown when model is not ready */}
        {!baseModelReady && (
          <InlineModelLoadingBlock
            modelStatus={newModelLoadingState.baseModelStatus}
            modelName={baseModelName}
            backendInfo={undefined}
            errorMessage={newModelLoadingState.error?.message ?? null}
            isLoading={newModelLoadingState.isLoading}
            progress={newModelLoadingState.progress}
            onLoadModel={handleInlineLoadModel}
            onRetry={handleInlineRetryLoad}
            autoLoadEnabled={chatAutoLoadPreference}
            onAutoLoadChange={handleChatAutoLoadPreferenceChange}
          />
        )}
        <AdapterSuggestion
          suggestion={activeSuggestion}
          loading={predictionLoading}
          autoAttachEnabled={autoAttachEnabled && !autoAttachPaused}
          onToggleAutoAttach={handleAutoAttachToggle}
          onAccept={handleAcceptSuggestion}
          onDismiss={() => handleDismissSuggestion()}
          showSnap={snapVisual}
          error={predictionError}
          conflictInfo={activeConflict}
          magnetColor={magnetDetails.color ?? undefined}
        />
        <form
          onSubmit={(e) => { e.preventDefault(); handleSend(); }}
          className="flex gap-2 items-start"
          aria-label="Chat message input"
        >
          <div className="flex flex-col gap-1">
            <span className="text-xs text-muted-foreground">Routing determinism</span>
            <Select
              value={routingMode}
              onValueChange={(value: 'deterministic' | 'adaptive') => setRoutingMode(value)}
              aria-label="Routing determinism mode"
            >
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder="Deterministic" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="deterministic">Deterministic</SelectItem>
                <SelectItem value="adaptive">Adaptive</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex-1 flex flex-col gap-2">
            <div className="relative">
              {showMagnetField && (
                <div
                  className="pointer-events-none absolute inset-0 rounded-lg magnet-field"
                  style={{
                    ...magnetGlowStyle,
                    transform: `scale(${1 + Math.min(0.05, magnetDetails.confidence / 8)})`,
                  }}
                  aria-hidden
                />
              )}
              {demoMode && (
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  className="absolute right-2 top-2 z-10"
                  onClick={runDemoScript}
                  disabled={isStreaming || isTypingDemoScript}
                >
                  <PlayCircle className="h-4 w-4 mr-1" />
                  Run Script
                </Button>
              )}
              <Textarea
                ref={inputRef}
                value={input}
                onChange={e => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
                className={cn(
                  'min-h-[calc(var(--base-unit)*15)] resize-none flex-1 pr-28 transition-shadow relative z-[1]',
                  showMagnetField ? 'magnet-textarea' : ''
                )}
                disabled={isStreaming || modelGateActive}
                aria-label="Message input"
                data-testid="chat-input"
              />
            </div>
            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>Attached adapters</span>
                <div className="flex items-center gap-2">
                  <span className={autoAttachEnabled ? 'text-primary font-medium' : ''}>
                    Auto-Attach {autoAttachEnabled ? 'on' : 'off'}
                  </span>
                  {autoAttachPaused && (
                    <span className="text-amber-600 font-medium">Temporarily paused</span>
                  )}
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {attachedAdapters.length === 0 ? (
                  <span className="text-xs text-muted-foreground">No attachments yet</span>
                ) : (
                  attachedAdapters.map((adapter) => (
                    <AdapterAttachmentChip
                      key={adapter.id}
                      adapterId={adapter.id}
                      confidence={adapter.confidence}
                      onRemove={() => handleRemoveAttachment(adapter.id, true)}
                      flash={lastAttachedAdapterId === adapter.id}
                    />
                  ))
                )}
              </div>
              {snapVisual && activeSuggestion && (
                <div className="flex items-center gap-2 text-xs text-primary pt-1">
                  <Link2 className="h-3.5 w-3.5" aria-hidden />
                  <span>Snapped {activeSuggestion.id} to this prompt</span>
                </div>
              )}
              {suggestedAdapters.length > 0 && (
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  <span className="text-xs text-muted-foreground">Suggested:</span>
                  {suggestedAdapters.map((adapter) => (
                    <AdapterAttachmentChip
                      key={adapter.id}
                      adapterId={adapter.id}
                      confidence={adapter.confidence}
                      variant="suggested"
                      onClick={() => attachFromUI(adapter)}
                    />
                  ))}
                </div>
              )}
            </div>
          </div>
          {isStreaming && (
            <Button
              variant="outline"
              size="icon"
              onClick={cancelStream}
              aria-label="Cancel response"
              className="mr-2"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
          <Button
            type="submit"
            disabled={isStreaming || !input.trim() || modelGateActive}
            size="lg"
            aria-label={isStreaming ? "Sending message..." : "Send message"}
          >
            {isStreaming ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Send className="h-4 w-4" />
            )}
          </Button>
        </form>
        {!isStreaming && tokensReceived > 0 && streamDuration && (
          <div className="text-xs text-muted-foreground mt-2 px-4" role="status" aria-live="polite">
            {tokensReceived} tokens · {(streamDuration / 1000).toFixed(1)}s
          </div>
        )}
      </div>

      {developerModeEnabled && (
        <div className="mt-4 rounded-md border border-border bg-muted/40 p-3 text-xs font-mono text-foreground">
          <div className="font-semibold mb-2">Raw JSON traces</div>
          <pre className="whitespace-pre-wrap break-all text-muted-foreground">
            {JSON.stringify(
              {
                lastDecision,
                recentDecisions: decisionHistory.slice(-3),
                sessionId: currentSessionId,
                stackId: selectedStackId,
              },
              null,
              2
            )}
          </pre>
        </div>
      )}

      {/* Share Dialog */}
      {shareDialogSessionId && (
        <ChatShareDialog
          sessionId={shareDialogSessionId}
          open={!!shareDialogSessionId}
          onOpenChange={(open) => {
            if (!open) setShareDialogSessionId(null);
          }}
        />
      )}

      {/* Tags Manager Dialog */}
      {tagsDialogSessionId && (
        <Dialog open={!!tagsDialogSessionId} onOpenChange={(open) => {
          if (!open) setTagsDialogSessionId(null);
        }}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Manage Tags</DialogTitle>
            </DialogHeader>
            <ChatTagsManager sessionId={tagsDialogSessionId} />
          </DialogContent>
        </Dialog>
      )}

      {/* Categories Manager Dialog */}
      {categoryDialogSessionId && (
        <Dialog open={!!categoryDialogSessionId} onOpenChange={(open) => {
          if (!open) setCategoryDialogSessionId(null);
        }}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Manage Category</DialogTitle>
            </DialogHeader>
            <ChatCategoriesManager sessionId={categoryDialogSessionId} />
          </DialogContent>
        </Dialog>
      )}

      <Dialog
        open={!!verificationDialogTrace}
        onOpenChange={(open) => {
          if (!open) {
            setVerificationDialogTrace(null);
            setVerificationDialogError(null);
          }
        }}
      >
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>Verification Report</DialogTitle>
            <p className="text-sm text-muted-foreground">
              Trace: {verificationDialogTrace ?? 'n/a'}
            </p>
          </DialogHeader>
          <div className="bg-muted/60 rounded-md p-3 text-xs font-mono max-h-[60vh] overflow-auto border">
            {verificationDialogLoading ? (
              <div className="flex items-center gap-2 text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>Verifying...</span>
              </div>
            ) : verificationDialogTrace && verificationReports[verificationDialogTrace] ? (
              <pre className="whitespace-pre-wrap break-all">
                {JSON.stringify(verificationReports[verificationDialogTrace], null, 2)}
              </pre>
            ) : (
              <div className="text-muted-foreground">No verification data yet.</div>
            )}
          </div>
          {verificationDialogError && (
            <p className="text-xs text-destructive">{verificationDialogError}</p>
          )}
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={!verificationDialogTrace}
              onClick={() => verificationDialogTrace && fetchVerificationReport(verificationDialogTrace)}
            >
              <RefreshCw className="h-4 w-4 mr-1" />
              Refresh report
            </Button>
            {verificationDialogTrace && verificationReports[verificationDialogTrace]?.run_head_hash && (
              <Button
                variant="ghost"
                size="sm"
                onClick={async () => {
                  const value =
                    verificationReports[verificationDialogTrace]?.run_head_hash?.computed_hex ||
                    verificationReports[verificationDialogTrace]?.run_head_hash?.expected_hex;
                  if (!value) return;
                  try {
                    await navigator.clipboard.writeText(value);
                    toast.success('run_head_hash copied');
                  } catch {
                    toast.error('Unable to copy run_head_hash');
                  }
                }}
              >
                <Copy className="h-4 w-4 mr-1" />
                Copy run_head_hash
              </Button>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Archive Panel Dialog */}
      {isArchivePanelOpen && (
        <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 flex items-center justify-center">
          <div className="bg-background border rounded-lg shadow-lg p-6 max-w-4xl w-full max-h-[90vh] overflow-auto">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold">Archive & Trash</h2>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setIsArchivePanelOpen(false)}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
            <SectionErrorBoundary sectionName="Archive Panel">
              <ChatArchivePanel />
            </SectionErrorBoundary>
          </div>
        </div>
      )}

      {/* Evidence Drawer */}
      <EvidenceDrawer onViewDocument={handleViewDocumentClick} />
    </div>
    </EvidenceDrawerProvider>
  );
}

function areChatPropsEqual(prevProps: ChatInterfaceProps, nextProps: ChatInterfaceProps) {
  // Check primitive props
  if (prevProps.selectedTenant !== nextProps.selectedTenant) return false;
  if (prevProps.initialStackId !== nextProps.initialStackId) return false;
  if (prevProps.selectedStackId !== nextProps.selectedStackId) return false;
  if (prevProps.sessionId !== nextProps.sessionId) return false;
  if (prevProps.streamMode !== nextProps.streamMode) return false;
  if (prevProps.developerMode !== nextProps.developerMode) return false;
  if (prevProps.kernelMode !== nextProps.kernelMode) return false;
  if (prevProps.selectedMessageId !== nextProps.selectedMessageId) return false;

  // Check documentContext (deep comparison)
  if (prevProps.documentContext !== nextProps.documentContext) {
    if (!prevProps.documentContext || !nextProps.documentContext) return false;
    if (prevProps.documentContext.documentId !== nextProps.documentContext.documentId) return false;
    if (prevProps.documentContext.documentName !== nextProps.documentContext.documentName) return false;
    if (prevProps.documentContext.collectionId !== nextProps.documentContext.collectionId) return false;
  }

  // Check datasetContext (deep comparison)
  if (prevProps.datasetContext !== nextProps.datasetContext) {
    if (!prevProps.datasetContext || !nextProps.datasetContext) return false;
    if (prevProps.datasetContext.datasetId !== nextProps.datasetContext.datasetId) return false;
    if (prevProps.datasetContext.datasetName !== nextProps.datasetContext.datasetName) return false;
    if (prevProps.datasetContext.collectionId !== nextProps.datasetContext.collectionId) return false;
    if (prevProps.datasetContext.datasetVersionId !== nextProps.datasetContext.datasetVersionId) return false;
  }

  // Check callbacks - compare by reference
  if (prevProps.onStackChange !== nextProps.onStackChange) return false;
  if (prevProps.onSessionChange !== nextProps.onSessionChange) return false;
  if (prevProps.onViewDocument !== nextProps.onViewDocument) return false;
  if (prevProps.onMessageComplete !== nextProps.onMessageComplete) return false;
  if (prevProps.onMessageSelect !== nextProps.onMessageSelect) return false;

  // All props are equal - skip re-render
  return true;
}

function ChatInterfaceWithProviders(props: ChatInterfaceProps) {
  const providerKey = `${props.selectedTenant || 'tenant-default'}-${props.sessionId || 'no-session'}`;
  return (
    <ChatProvider key={providerKey}>
      <ChatInterfaceInner {...props} />
    </ChatProvider>
  );
}

// Wrap with React.memo to prevent unnecessary re-renders
export const ChatInterface = React.memo(ChatInterfaceWithProviders, areChatPropsEqual);
