import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { ChatMessageComponent, type ChatMessage, type EvidenceItem } from './chat/ChatMessage';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Send, Loader2, Layers, History, X, ChevronLeft, Plus, Activity, Database, Archive, Trash2, FileText } from 'lucide-react';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/useAdmin';
import { useChatSessionsApi } from '@/hooks/useChatSessionsApi';
import { useCollections } from '@/hooks/useCollectionsApi';
import type { ExtendedRouterDecision } from '@/api/types';
import type { ChatSession } from '@/types/chat';
import { RouterActivitySidebar } from './chat/RouterActivitySidebar';
import { AdapterLoadingStatus } from './chat/AdapterLoadingStatus';
import { PreChatAdapterPrompt } from './chat/PreChatAdapterPrompt';
import { ChatSearchBar } from './chat/ChatSearchBar';
import { ChatSessionActions } from './chat/ChatSessionActions';
import { ChatTagsManager } from './chat/ChatTagsManager';
import { ChatShareDialog } from './chat/ChatShareDialog';
import { ChatArchivePanel } from './chat/ChatArchivePanel';
import { useChatExport } from '@/components/export';
import {
  useChatStreaming,
  useChatAdapterState,
  useChatRouterDecisions
} from '@/hooks/chat';
import { useChatAutoLoadModels } from '@/hooks/useFeatureFlags';
import {
  useModelLoadingState,
  useModelLoader,
  useChatLoadingPersistence,
  useLoadingAnnouncements,
} from '@/hooks/model-loading';
import { ChatLoadingOverlay } from './chat/ChatLoadingOverlay';
import { ChatErrorDisplay } from './chat/ChatErrorDisplay';
import { MissingPinnedAdaptersBanner } from './chat/MissingPinnedAdaptersBanner';
import { EvidenceDrawerProvider } from '@/contexts/EvidenceDrawerContext';
import { EvidenceDrawer } from './chat/EvidenceDrawer';
import apiClient from '@/api/client';

interface ChatInterfaceProps {
  selectedTenant: string;
  initialStackId?: string;
  sessionId?: string; // Optional: load existing session
  /** Document context for document-specific chat */
  documentContext?: {
    documentId: string;
    documentName: string;
    collectionId?: string;
  };
  /** Dataset context for dataset-scoped chat */
  datasetContext?: {
    datasetId: string;
    datasetName: string;
    collectionId?: string;
    datasetVersionId?: string;
  };
  /** Callback when user wants to view a document (for evidence navigation) */
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
  /** Streaming render mode (tokens or chunks) */
  streamMode?: 'tokens' | 'chunks';
  /** Developer toggle to show raw traces */
  developerMode?: boolean;
  /** Callback when a message completes (for workbench right rail auto-update) */
  onMessageComplete?: (messageId: string, traceId?: string) => void;
  /** Callback when user selects/clicks a message. Receives traceId for trace fetching. */
  onMessageSelect?: (messageId: string, traceId?: string) => void;
  /** Currently selected message ID (for highlighting) */
  selectedMessageId?: string | null;
}

export function ChatInterface({
  selectedTenant,
  initialStackId,
  sessionId,
  documentContext,
  datasetContext,
  onViewDocument,
  streamMode = 'tokens',
  developerMode = false,
  onMessageComplete,
  onMessageSelect,
  selectedMessageId,
}: ChatInterfaceProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [selectedStackId, setSelectedStackId] = useState<string>(initialStackId || '');
  const [selectedCollectionId, setSelectedCollectionId] = useState<string | null>(documentContext?.collectionId ?? null);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(sessionId || null);
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [isRouterActivityOpen, setIsRouterActivityOpen] = useState(false);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [newSessionName, setNewSessionName] = useState('');
  const [showContext, setShowContext] = useState(true);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const stackSelectorRef = useRef<HTMLButtonElement>(null);

  // New state for chat features
  const [searchQuery, setSearchQuery] = useState('');
  const [isArchivePanelOpen, setIsArchivePanelOpen] = useState(false);
  const [shareDialogSessionId, setShareDialogSessionId] = useState<string | null>(null);
  const [tagsDialogSessionId, setTagsDialogSessionId] = useState<string | null>(null);
  const [routingMode, setRoutingMode] = useState<'deterministic' | 'adaptive'>('deterministic');
  const [strengthOverrides, setStrengthOverrides] = useState<Record<string, number>>({});

  // Pinned adapters banner state
  const [bannerDismissed, setBannerDismissed] = useState(false);

  // Feature flags
  const autoLoadEnabled = useChatAutoLoadModels();

  // Use selectedTenant for API hooks that support undefined (default stack)
  // Keep tenantId fallback for other hooks that require a string
  const tenantId = selectedTenant || 'default';
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);
  const { data: collections = [] } = useCollections();
  const sessionSourceType = documentContext ? 'document' : 'general';
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
    const adapters = (selectedStack as any)?.adapters as
      | Array<{ id?: string; adapter_id?: string; name?: string; tier?: string; domain?: string; lora_strength?: number }>
      | undefined;
    if (adapters && adapters.length > 0) {
      return adapters.map(adapter => ({
        id: adapter.id ?? adapter.adapter_id ?? '',
        name: adapter.name ?? adapter.adapter_id ?? '',
        tier: adapter.tier,
        domain: adapter.domain,
        strength: adapter.lora_strength ?? 1,
      })).filter(adapter => adapter.id);
    }

    if ((selectedStack as any)?.adapter_ids) {
      return (selectedStack as any).adapter_ids.map((id: string) => ({
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
      }]);
    },
    onStreamComplete: async (response) => {
      // Fetch router decision and evidence
      let routerDecision = null;
      if (currentRequestId) {
        const decision = await fetchDecision(response.id, currentRequestId);
        routerDecision = decision;
      }

      // Fetch evidence
      const evidence = await fetchMessageEvidence(response.id);

      // Use response.id as traceId - streaming now extracts the server's request_id into id
      const traceId = response.id;

      // Use throughput stats from response (calculated in useChatStreaming with accurate values)
      // This avoids timing issues with React state batching

      const completedMessage = {
        ...response,
        id: streamingMessageId || response.id,
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
    },
    onError: (error) => {
      logger.error('Chat streaming error', { component: 'ChatInterface' }, error);
      // Remove streaming message on error
      if (streamingMessageId) {
        setMessages(prev => prev.filter(m => m.id !== streamingMessageId));
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
  const baseModelReady = autoLoadEnabled ? newModelLoadingState.baseModelReady : true;
  const canSend = autoLoadEnabled
    ? (isBaseOnlyMode || !hasAdapters ? baseModelReady : allReady)
    : allAdaptersReady;
  // Guard against an overlay lingering after readiness or error: only treat as loading when not ready and no error.
  const isLoadingModels = autoLoadEnabled
    ? newModelLoadingState.isLoading
      && !newModelLoadingState.error
      && !(isBaseOnlyMode && baseModelReady)
      && !newModelLoadingState.overallReady
    : isCheckingAdapters;

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

  // Set default stack on mount
  useEffect(() => {
    if (selectedStackId) {
      return;
    }

    if (defaultStack?.id) {
      setSelectedStackId(defaultStack.id);
      return;
    }

    if (stacks.length > 0) {
      setSelectedStackId(stacks[0].id);
    }
  }, [defaultStack, selectedStackId, stacks]);

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
        setSelectedStackId(session.stackId);
        // Note: collection_id is on the backend session, not local session
        // We'll fetch it when needed
      }
    } else if (!currentSessionId && selectedStackId && !isLoadingSessions) {
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
  }, [
    sessionId,
    currentSessionId,
    selectedStackId,
    stacks,
    isLoadingSessions,
    getSession,
    createSession,
    effectiveCollectionId,
    documentContext,
    sessionConfigForRequest,
  ]);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (scrollAreaRef.current) {
      const scrollContainer = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [messages]);


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

    if (autoLoadEnabled && !baseModelReady) {
      toast.error('Base model is not ready. Please load it first.');
      return;
    }

    // Only block on adapter readiness when adapters are present and base-only mode is off
    if (!isBaseOnlyMode && hasAdapters && !allReady) {
      toast.warning('Some adapters are not ready. Please load them first.');
      return;
    }

    // Resolve stack to adapter IDs (allow empty when base-only mode is active)
    const adapterIds: string[] = isBaseOnlyMode
      ? []
      : Array.isArray(selectedStack?.adapter_ids)
        ? selectedStack.adapter_ids
        : Array.isArray((selectedStack as any)?.adapters)
          ? (selectedStack as any).adapters.map((a: any) => a.id ?? a.adapter_id ?? '')
          : [];

    if (!adapterIds || adapterIds.length === 0) {
      if (!isBaseOnlyMode) {
        toast.error('Please select a stack with adapters');
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
    await sendMessage(messageContent, adapterIds);
  }, [
    allReady,
    autoLoadEnabled,
    baseModelReady,
    currentSessionId,
    hasAdapters,
    input,
    isBaseOnlyMode,
    isStreaming,
    selectedStack,
    sendMessage,
  ]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // selectedStack is memoized earlier in the component
  const adapterCount = selectedStack?.adapter_ids?.length ?? selectedStack?.adapters?.length ?? 0;
  const stackLabel = selectedStack?.name || 'No stack selected';
  const isDefaultStack = Boolean(
    defaultStack?.id && selectedStack?.id && selectedStack.id === defaultStack.id
  );
  const stackDetails = selectedStack?.lifecycle_state ?? selectedStack?.description ?? null;
  const baseModelLabel = autoLoadEnabled
    ? newModelLoadingState.baseModelStatus === 'loading'
      ? 'Loading base model...'
      : baseModelReady
        ? (isBaseOnlyMode || !hasAdapters ? 'Model ready (no adapters)' : 'Model ready')
        : 'Base model not ready'
    : 'Not provided';

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
        toast.success(newCollectionId ? 'Collection selected' : 'Collection cleared');
      } catch (error) {
        logger.error('Failed to update session collection', {
          component: 'ChatInterface',
          sessionId: currentSessionId,
          collectionId: newCollectionId,
        }, toError(error));
        toast.error('Failed to update collection');
      }
    }
  }, [currentSessionId, guardChatHistory, updateSessionCollection]);

  // Get selected collection name for display
  const selectedCollectionName = useMemo(() => {
    if (!selectedCollectionId) return 'No collection';
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
                                    onSetCategory={() => {
                                      // TODO: Implement category dialog
                                      toast.info('Category management coming soon');
                                    }}
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

      {/* Currently Loaded Panel */}
      <div className={`px-4 mt-2 ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
        <Card>
          <CardHeader className="flex flex-row items-start justify-between space-y-0">
            <div className="space-y-1">
              <CardTitle className="text-base">Currently Loaded</CardTitle>
              <p className="text-xs text-muted-foreground">
                Stack context for this chat session.
              </p>
              {isDefaultStack && (
                <Badge variant="secondary" className="w-fit" aria-label="This is the default adapter stack for your tenant">
                  Default stack for tenant
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
                <p className="text-xs text-muted-foreground">Collection</p>
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
            </CardContent>
          )}
        </Card>
      </div>

      {/* Header with stack selector */}
      <div className={`border-b px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
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
            <span className="text-sm text-muted-foreground">Collection:</span>
            <Select
              value={selectedCollectionId || 'none'}
              onValueChange={handleCollectionChange}
              aria-label="Select collection"
            >
              <SelectTrigger
                className="w-[calc(var(--base-unit)*50)]"
                aria-label="Select collection"
              >
                <SelectValue placeholder="No collection" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">No collection</SelectItem>
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
            onClick={() => setIsRouterActivityOpen(!isRouterActivityOpen)}
            aria-label={isRouterActivityOpen ? "Close router activity" : "Open router activity"}
            title="View router decision history"
          >
            <Activity className="h-4 w-4" />
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
      </div>

      {/* Missing Pinned Adapters Banner */}
      {unavailablePinnedAdapters.length > 0 && !bannerDismissed && (
        <div className={`px-4 pt-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
          <MissingPinnedAdaptersBanner
            unavailablePinnedAdapters={unavailablePinnedAdapters}
            pinnedRoutingFallback={pinnedRoutingFallback}
            onDismiss={() => setBannerDismissed(true)}
          />
        </div>
      )}

      {/* Messages area */}
      <ScrollArea
        className={`flex-1 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}
        ref={scrollAreaRef}
        aria-label="Chat messages"
        role="log"
        aria-live="polite"
        aria-atomic="false"
      >
        <SectionErrorBoundary sectionName="Chat Messages">
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
              messages.map(message => (
                <ChatMessageComponent
                  key={message.id}
                  message={
                    // Update streaming message with current streamed text
                    message.id === streamingMessageId
                      ? { ...message, content: streamingContent }
                      : message
                  }
                  onViewDocument={handleViewDocumentClick}
                  onSelect={onMessageSelect}
                  isSelected={selectedMessageId === message.id}
                />
              ))
            )}
            {isLoadingDecision && (
              <div className="text-xs text-muted-foreground px-4" role="status" aria-live="polite">
                Loading router decision details...
              </div>
            )}
          </div>
        </SectionErrorBoundary>
      </ScrollArea>

      {/* Input area */}
      <div className={`border-t px-4 py-3 transition-all ${isHistoryOpen ? 'ml-80' : ''} ${isRouterActivityOpen ? 'mr-96' : ''}`}>
        <form 
          onSubmit={(e) => { e.preventDefault(); handleSend(); }}
          className="flex gap-2"
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
          <Textarea
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
            className="min-h-[calc(var(--base-unit)*15)] resize-none"
            disabled={isStreaming || !selectedStackId || !currentSessionId}
            aria-label="Message input"
            aria-describedby={!selectedStackId ? "stack-required-hint" : undefined}
            data-testid="chat-input"
          />
          {!selectedStackId && (
            <span id="stack-required-hint" className="sr-only">
              Please select an adapter stack before sending messages
            </span>
          )}
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
            disabled={isStreaming || !input.trim() || !selectedStackId || (autoLoadEnabled && !canSend)}
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
        {!selectedStackId && (
          <p className="text-xs text-muted-foreground mt-2">
            Please select a stack to start chatting
          </p>
        )}
        {!isStreaming && tokensReceived > 0 && streamDuration && (
          <div className="text-xs text-muted-foreground mt-2 px-4" role="status" aria-live="polite">
            {tokensReceived} tokens · {(streamDuration / 1000).toFixed(1)}s
          </div>
        )}
      </div>

      {developerMode && (
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
