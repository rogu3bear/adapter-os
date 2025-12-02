import { useState, useRef, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Send, Bot, User, Loader2, Database, Layers, X } from 'lucide-react';
import { useChatStreaming } from '@/hooks/chat/useChatStreaming';
import { useModelStatus } from '@/hooks/useModelStatus';
import { useAutoLoadModel } from '@/hooks/useAutoLoadModel';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/useAdmin';
import { useChatAutoLoadModels } from '@/hooks/useFeatureFlags';
import {
  useModelLoadingState,
  useModelLoader,
  useChatLoadingPersistence,
  useLoadingAnnouncements,
} from '@/hooks/model-loading';
import { ChatErrorDisplay } from './ChatErrorDisplay';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import type { ChatMessage } from './ChatMessage';

interface SimplifiedChatWidgetProps {
  selectedTenant: string;
}

const WELCOME_MESSAGE: ChatMessage = {
  id: 'welcome',
  role: 'assistant',
  content: `Hello! I'm your AdapterOS chat assistant. I can help you with questions and tasks.

Select an adapter stack and make sure a model is loaded to start chatting.`,
  timestamp: new Date(),
};

export function SimplifiedChatWidget({ selectedTenant }: SimplifiedChatWidgetProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([WELCOME_MESSAGE]);
  const [input, setInput] = useState('');
  const [selectedStackId, setSelectedStackId] = useState<string>('');
  const scrollAreaRef = useRef<HTMLDivElement>(null);

  const tenantId = selectedTenant || 'default';
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);

  // Feature flag for new model loading hooks
  const autoLoadEnabled = useChatAutoLoadModels();

  // Legacy hooks (used when feature flag is off)
  const { status: legacyModelStatus, modelName: legacyModelName, isReady: legacyModelReady } = useModelStatus(tenantId);
  const { isAutoLoading: legacyIsAutoLoading, error: legacyAutoLoadError, loadModel: legacyLoadModel } = useAutoLoadModel(tenantId, true);

  // New model loading hooks (used when feature flag is on)
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

  // Track if we've started loading (to persist only on start, not every update)
  const wasLoadingRef = useRef(false);

  // Auto-recover loading state after page refresh
  useEffect(() => {
    if (autoLoadEnabled && isRecoverable && persistedState && selectedStackId === persistedState.stackId) {
      logger.info('Recovering loading state after page refresh', {
        component: 'SimplifiedChatWidget',
        stackId: persistedState.stackId,
        adaptersToLoad: persistedState.adaptersToLoad.length,
      });
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

  // Select which state to use based on feature flag
  const modelStatus = autoLoadEnabled ? newModelLoadingState.baseModelStatus : legacyModelStatus;
  const modelName = autoLoadEnabled ? newModelLoadingState.baseModelName : legacyModelName;
  const modelReady = autoLoadEnabled ? newModelLoadingState.baseModelReady : legacyModelReady;
  const isAutoLoading = autoLoadEnabled ? newModelLoadingState.isLoading : legacyIsAutoLoading;
  const autoLoadError = autoLoadEnabled ? newModelLoadingState.error : legacyAutoLoadError;

  // Set default stack on mount
  useEffect(() => {
    if (!selectedStackId && defaultStack?.id) {
      setSelectedStackId(defaultStack.id);
    }
  }, [defaultStack, selectedStackId]);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (scrollAreaRef.current) {
      const scrollContainer = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [messages]);

  // Chat streaming hook
  const {
    isStreaming,
    streamedText,
    sendMessage,
    cancelStream,
  } = useChatStreaming({
    sessionId: null, // No session management in simplified widget
    stackId: selectedStackId,
    onMessageSent: (message) => {
      setMessages(prev => [...prev, message]);
      // Create placeholder streaming message
      const assistantId = `assistant-${Date.now()}`;
      setMessages(prev => [...prev, {
        id: assistantId,
        role: 'assistant',
        content: '',
        timestamp: new Date(),
        isStreaming: true,
      }]);
    },
    onStreamComplete: (response) => {
      // Update streaming message with final response
      setMessages(prev => prev.map(msg => 
        msg.isStreaming ? { ...response, isStreaming: false } : msg
      ));
    },
    onError: (error) => {
      // Remove streaming message on error
      setMessages(prev => prev.filter(msg => !msg.isStreaming));
      toast.error(`Chat error: ${error.message}`);
    },
  });

  // Update streaming message text
  useEffect(() => {
    if (streamedText && isStreaming) {
      setMessages(prev => prev.map(msg => 
        msg.isStreaming ? { ...msg, content: streamedText } : msg
      ));
    }
  }, [streamedText, isStreaming]);

  const handleSend = useCallback(async () => {
    if (!input.trim() || isStreaming) return;

    // Check if model is ready
    if (!modelReady && modelStatus !== 'loading') {
      toast.warning('Please load a model first');
      return;
    }

    // Check if stack is selected
    const selectedStack = stacks.find(s => s.id === selectedStackId);
    if (!selectedStack || !selectedStack.adapter_ids || selectedStack.adapter_ids.length === 0) {
      toast.error('Please select a stack with adapters');
      return;
    }

    const messageContent = input.trim();
    setInput('');

    // Send message using streaming hook
    await sendMessage(messageContent, selectedStack.adapter_ids);
  }, [input, isStreaming, modelReady, modelStatus, selectedStackId, stacks, sendMessage]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleLoadModel = useCallback(async () => {
    try {
      if (autoLoadEnabled) {
        await newModelLoader.loadModels(selectedStackId);
      } else {
        await legacyLoadModel();
      }
    } catch {
      toast.error('Failed to load model');
    }
  }, [autoLoadEnabled, newModelLoader, selectedStackId, legacyLoadModel]);

  const selectedStack = stacks.find(s => s.id === selectedStackId);

  return (
    <div
      className="flex flex-col h-full bg-white rounded-lg border border-slate-200 shadow-sm relative"
      role="region"
      aria-label="Simplified chat interface"
    >
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

      {/* Error display (when feature flag is enabled and error occurred) */}
      {autoLoadEnabled && newModelLoadingState.error && !newModelLoadingState.isLoading && (
        <div className="absolute inset-x-2 top-2 z-20">
          <ChatErrorDisplay
            error={newModelLoadingState.error}
            onRetry={() => newModelLoader.loadModels(selectedStackId)}
            currentRetry={newModelLoadingState.error.retryCount}
            maxRetries={newModelLoadingState.error.maxRetries}
            className="text-xs"
          />
        </div>
      )}

      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-slate-200 bg-slate-50">
        <Bot className="w-5 h-5 text-blue-600" aria-hidden="true" />
        <h3 className="font-semibold text-slate-900">Chat Assistant</h3>
        <Badge variant="secondary" className="ml-auto text-xs">
          Simplified
        </Badge>
      </div>

      {/* Context Header */}
      <div className="px-4 py-2 bg-slate-100 border-b border-slate-200">
        <div className="flex items-center gap-2 flex-wrap text-xs mb-2">
          <span className="text-slate-500 font-medium">Status:</span>

          {/* Model Status */}
          <Badge 
            variant={modelReady ? "default" : modelStatus === 'loading' || isAutoLoading ? "secondary" : "destructive"}
            className="gap-1"
          >
            <Database className="h-3 w-3" />
            {modelStatus === 'loading' || isAutoLoading ? 'Loading...' : modelReady ? (modelName || 'Model loaded') : 'No model'}
          </Badge>

          {/* Stack Status */}
          <Badge variant="outline" className="gap-1">
            <Layers className="h-3 w-3" />
            {selectedStack?.name || 'No stack'}
          </Badge>
        </div>

        {/* Model Loading Controls */}
        {!modelReady && modelStatus !== 'loading' && !isAutoLoading && !autoLoadError && (
          <div className="flex items-center gap-2 mt-2">
            <Button
              size="sm"
              variant="outline"
              onClick={handleLoadModel}
              disabled={isAutoLoading}
              className="text-xs"
            >
              Load Model
            </Button>
          </div>
        )}

        {/* Stack Selector */}
        <div className="mt-2">
          <Select value={selectedStackId} onValueChange={setSelectedStackId}>
            <SelectTrigger className="h-8 text-xs">
              <SelectValue placeholder="Select adapter stack" />
            </SelectTrigger>
            <SelectContent>
              {stacks.map((stack) => (
                <SelectItem key={stack.id} value={stack.id}>
                  {stack.name || stack.id}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Messages */}
      <ScrollArea ref={scrollAreaRef} className="flex-1 p-4">
        <div className="space-y-4" role="log" aria-live="polite" aria-label="Chat messages">
          {messages.map((message) => (
            <div
              key={message.id}
              className={`flex gap-3 ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}
            >
              {message.role === 'assistant' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-100 flex items-center justify-center">
                  <Bot className="w-5 h-5 text-blue-600" />
                </div>
              )}

              <div
                className={`flex flex-col gap-2 max-w-[80%] ${
                  message.role === 'user' ? 'items-end' : 'items-start'
                }`}
              >
                <div
                  className={`rounded-lg px-4 py-2 ${
                    message.role === 'user'
                      ? 'bg-blue-600 text-white'
                      : 'bg-slate-100 text-slate-900'
                  } ${message.isStreaming ? 'animate-pulse' : ''}`}
                >
                  <p className="text-sm whitespace-pre-wrap">{message.content}</p>
                  {message.isStreaming && (
                    <span
                      className="inline-block w-2 h-4 ml-1 bg-current animate-pulse"
                      aria-label="Streaming in progress"
                    />
                  )}
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-xs text-slate-400">
                    {message.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </span>
                </div>
              </div>

              {message.role === 'user' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-slate-200 flex items-center justify-center">
                  <User className="w-5 h-5 text-slate-600" />
                </div>
              )}
            </div>
          ))}

          {isStreaming && !messages.some(m => m.isStreaming) && (
            <div className="flex gap-3 justify-start">
              <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-100 flex items-center justify-center">
                <Bot className="w-5 h-5 text-blue-600" />
              </div>
              <div className="bg-slate-100 rounded-lg px-4 py-2">
                <Loader2 className="w-5 h-5 text-slate-400 animate-spin" />
              </div>
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Input */}
      <form onSubmit={(e) => { e.preventDefault(); handleSend(); }} className="p-4 border-t border-slate-200" role="search">
        <label htmlFor="simplified-chat-input" className="sr-only">Type your message</label>
        <div className="flex gap-2">
          <Input
            id="simplified-chat-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={!modelReady ? "Load a model first..." : !selectedStackId ? "Select a stack..." : "Type your message..."}
            disabled={isStreaming || !modelReady || !selectedStackId}
            className="flex-1"
            aria-describedby="chat-status"
          />
          <span id="chat-status" className="sr-only" aria-live="assertive">
            {isStreaming ? 'Streaming response...' : modelReady && selectedStackId ? 'Ready' : 'Not ready'}
          </span>
          <Button
            type="submit"
            disabled={isStreaming || !input.trim() || !modelReady || !selectedStackId}
            size="icon"
            aria-label="Send message"
          >
            {isStreaming ? (
              <Loader2 className="w-4 h-4 animate-spin" aria-hidden="true" />
            ) : (
              <Send className="w-4 h-4" aria-hidden="true" />
            )}
          </Button>
          {isStreaming && (
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={cancelStream}
              aria-label="Cancel streaming"
            >
              <X className="w-4 h-4" aria-hidden="true" />
            </Button>
          )}
        </div>
      </form>
    </div>
  );
}

