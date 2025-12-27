// @ts-nocheck
import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { ToolPageHeader } from './ui/page-headers/ToolPageHeader';
import { ProgressiveHint } from './ui/progressive-hint';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

// Inference components
import { ConfigurationPanel } from './inference/ConfigurationPanel';
import { SessionHistoryPanel } from './inference/SessionHistoryPanel';
import { InferenceOutput } from './inference/InferenceOutput';
import { BatchProcessor } from './inference/BatchProcessor';
import { ComparisonMode } from './inference/ComparisonMode';
import { RunReceiptPanel } from '@/components/receipts/RunReceiptPanel';
import { EvidencePanel as TraceEvidencePanel } from '@/components/evidence/EvidencePanel';
import { PromptTemplateManager } from './PromptTemplateManager';
import { Stack } from './inference/StackSelector';
import { InferenceMode, PlaygroundMode, InferenceMetrics } from './inference/types';

// Hooks
import {
  useInferenceConfig,
  useStreamingInference,
  useBatchInference,
  useInferenceSessions,
} from '@/hooks/inference';
import { useBackendSelection } from '@/hooks/inference/useBackendSelection';
import { useCoreMLManagement } from '@/hooks/inference/useCoreMLManagement';
import { useAdapterSelection } from '@/hooks/inference/useAdapterSelection';
import { useInferenceUrlState } from '@/hooks/inference/useInferenceUrlState';
import { useAdapterStacks, useGetDefaultStack, useSetDefaultStack } from '@/hooks/admin/useAdmin';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useProgressiveHints } from '@/hooks/tutorial/useProgressiveHints';
import { useCancellableOperation } from '@/hooks/async/useCancellableOperation';
import { usePromptTemplates, PromptTemplate as PromptTemplateType } from '@/hooks/chat/usePromptTemplates';
import { getPageHints } from '@/data/page-hints';

// Icons
import {
  Zap,
  Clock,
  Split,
  FileText,
  Wifi,
  Layers,
  TrendingUp,
  Target,
} from 'lucide-react';

// API and utilities
import { apiClient } from '@/api/services';
import { InferRequest, InferResponse, InferenceConfig, BackendName, InferenceSession } from '@/api/types';
import { InferenceRequestSchema } from '@/schemas';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { ZodError } from 'zod';
import { LAST_MODEL_KEY } from './inference/constants';

interface InferencePlaygroundProps {
  selectedTenant: string;
}

function InferencePlaygroundContent({ selectedTenant }: InferencePlaygroundProps) {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();

  // URL state
  const urlState = useInferenceUrlState();

  // UI mode state
  const [mode, setMode] = useState<PlaygroundMode>('single');
  const [inferenceMode, setInferenceMode] = useState<InferenceMode>('standard');
  const [metrics, setMetrics] = useState<InferenceMetrics | null>(null);
  const [showTemplateManager, setShowTemplateManager] = useState(false);

  // Model selection with localStorage persistence
  const [selectedModelId, setSelectedModelId] = useState<string>(() => {
    try {
      return urlState.initialState.modelId || localStorage.getItem(LAST_MODEL_KEY) || '';
    } catch {
      return '';
    }
  });

  // Stack state
  const [selectedStackId, setSelectedStackId] = useState<string>(urlState.initialState.stackId || '');
  const { data: stacks = [] } = useAdapterStacks();
  const { data: defaultStack } = useGetDefaultStack(selectedTenant);
  const { mutateAsync: setDefaultStack } = useSetDefaultStack(selectedTenant);

  // Feature hooks
  const backend = useBackendSelection({ modelId: selectedModelId });
  const adapter = useAdapterSelection({ initialAdapterId: urlState.initialState.adapterId });
  const coreml = useCoreMLManagement({
    adapterId: adapter.selectedAdapterId,
    modelId: selectedModelId,
    selectedAdapter: adapter.selectedAdapter,
    coremlAvailable: backend.backendOptions.find((o) => o.name === 'coreml')?.available,
  });

  // Inference hooks
  const config = useInferenceConfig();
  const streaming = useStreamingInference({
    config: config.configA,
    adapterId: adapter.selectedAdapterId,
    stackId: selectedStackId,
  });
  const batch = useBatchInference({
    config: config.configA,
    adapterId: adapter.selectedAdapterId,
    stackId: selectedStackId,
  });
  const sessions = useInferenceSessions();

  // Cancellation support
  const { state: inferenceState, start: startInference, cancel: cancelInference } = useCancellableOperation();

  // Progressive hints
  const hints = getPageHints('inference').map((hint) => ({
    ...hint,
    condition: hint.id === 'no-adapters-inference' ? () => adapter.adapters.length === 0 : hint.condition,
  }));
  const { getVisibleHint, dismissHint } = useProgressiveHints({ pageKey: 'inference', hints });
  const visibleHint = getVisibleHint();

  // Load default stack on mount
  useEffect(() => {
    if (defaultStack && !selectedStackId) {
      setSelectedStackId(defaultStack.id);
    }
  }, [defaultStack, selectedStackId]);

  // Update URL when model changes
  const handleModelChange = useCallback(
    (modelId: string) => {
      setSelectedModelId(modelId);
      const { backend: preferredBackend } = backend.selectBackend(backend.selectedBackend);
      config.setConfigA((prev) => ({ ...prev, model: modelId, backend: preferredBackend }));
      config.setConfigB((prev) => ({ ...prev, model: modelId, backend: preferredBackend }));
      urlState.updateUrl('modelId', modelId);
      try {
        localStorage.setItem(LAST_MODEL_KEY, modelId);
      } catch {
        // Ignore storage errors
      }
    },
    [backend, config, urlState]
  );

  // Handle stack selection
  const handleStackChange = useCallback(
    (stackId: string) => {
      setSelectedStackId(stackId);
      urlState.updateUrl('stackId', stackId || undefined);
      if (stackId) {
        adapter.setSelectedAdapterId('none');
      }
    },
    [adapter, urlState]
  );

  // Set determinism mode
  const handleSetDeterminismMode = useCallback(
    (deterMode: 'deterministic' | 'adaptive') => {
      config.setConfigA((prev) => ({ ...prev, routing_determinism_mode: deterMode }));
      config.setConfigB((prev) => ({ ...prev, routing_determinism_mode: deterMode }));
    },
    [config]
  );

  // Save session
  const saveSession = useCallback(
    (cfg: InferenceConfig, response: InferResponse) => {
      const selectedStack = stacks.find((s) => s.id === selectedStackId);
      const session = sessions.saveCurrentSession(cfg, response);
      if (selectedStackId || selectedStack?.name) {
        session.stack_id = selectedStackId || undefined;
        session.stack_name = selectedStack?.name || undefined;
      }
      sessions.addSession(session);
    },
    [stacks, selectedStackId, sessions]
  );

  // Run inference
  const handleInfer = useCallback(async () => {
    clearError('inference');
    config.setIsLoadingA(true);
    config.setResponseA(null);

    try {
      const { backend: resolvedBackend, reason: backendReason } = backend.resolveBackendForRequest(
        config.configA.backend as BackendName
      );
      if (backendReason) toast.info(backendReason);
      backend.setLastBackendUsed(resolvedBackend);

      // Resolve adapters
      const adapterIds = selectedStackId
        ? stacks.find((s) => s.id === selectedStackId)?.adapter_ids
        : adapter.selectedAdapterId && adapter.selectedAdapterId !== 'none'
          ? [adapter.selectedAdapterId]
          : undefined;

      // Validate request
      await InferenceRequestSchema.parseAsync({
        prompt: config.configA.prompt,
        max_tokens: config.configA.max_tokens,
        temperature: config.configA.temperature,
        top_k: config.configA.top_k,
        top_p: config.configA.top_p,
        backend: resolvedBackend,
        model: selectedModelId || config.configA.model,
        seed: config.configA.seed,
        require_evidence: config.configA.require_evidence,
        adapter_stack: adapterIds,
      });

      await startInference(async (signal) => {
        const request: InferRequest = {
          ...config.configA,
          backend: resolvedBackend,
          model: selectedModelId || config.configA.model,
          adapter_stack: adapterIds,
        };
        const response = await apiClient.infer(request, {}, false, signal);
        backend.setLastBackendUsed(response.backend_used || response.backend || resolvedBackend);
        config.setResponseA(response);
        saveSession(config.configA, response);

        // Update metrics
        setMetrics({
          latency: response.latency_ms || 0,
          tokensPerSecond: response.tokens_generated && response.latency_ms
            ? (response.tokens_generated / response.latency_ms) * 1000
            : 0,
          totalTokens: response.tokens_generated || response.token_count || 0,
        });

        return response;
      }, `inference-${config.configA.id}`);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Inference failed');
      if (err instanceof ZodError) {
        addError('inference', `Validation error: ${error.message}`, () => handleInfer());
      } else {
        logger.error('Inference failed', { component: 'InferencePlayground' }, toError(err));
        addError('inference', error.message, () => handleInfer());
      }
    } finally {
      config.setIsLoadingA(false);
    }
  }, [backend, config, selectedModelId, selectedStackId, stacks, adapter, startInference, saveSession, clearError, addError]);

  // Run streaming inference
  const handleStreamingInfer = useCallback(async () => {
    clearError('inference');
    config.setIsLoadingA(true);
    config.setResponseA(null);

    try {
      const { backend: resolvedBackend, reason: backendReason } = backend.resolveBackendForRequest(
        config.configA.backend as BackendName
      );
      if (backendReason) toast.info(backendReason);
      backend.setLastBackendUsed(resolvedBackend);

      const streamingConfig: InferenceConfig = {
        ...config.configA,
        backend: resolvedBackend,
        model: selectedModelId || config.configA.model,
      };

      await streaming.startStreaming(streamingConfig.prompt, streamingConfig);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Streaming failed');
      addError('inference', error.message, () => handleStreamingInfer());
    } finally {
      config.setIsLoadingA(false);
    }
  }, [backend, config, selectedModelId, streaming, clearError, addError]);

  // Handle cancel
  const handleCancel = useCallback(() => {
    if (streaming.isStreaming) {
      streaming.cancelStreaming();
    } else {
      cancelInference();
    }
  }, [streaming, cancelInference]);

  // Export results
  const handleExport = useCallback(() => {
    if (!config.responseA) return;
    const data = {
      prompt: config.configA.prompt,
      config: config.configA,
      response: config.responseA,
      timestamp: new Date().toISOString(),
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `inference-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }, [config]);

  // Load session
  const handleLoadSession = useCallback(
    (session: InferenceSession) => {
      config.setConfigA({ ...config.configA, ...session.request, prompt: session.prompt });
      if (session.response) {
        config.setResponseA(session.response);
      }
    },
    [config]
  );

  // Template application
  const { recordTemplateUsage, substituteVariables, getRecentTemplates } = usePromptTemplates();
  const handleApplyTemplate = useCallback(
    (template: PromptTemplateType) => {
      recordTemplateUsage(template.id);
      if (template.variables.length === 0) {
        config.setConfigA({ ...config.configA, prompt: template.prompt });
      }
    },
    [config, recordTemplateUsage]
  );

  // Render streaming response
  const renderResponse = () => {
    if (inferenceMode === 'streaming' && streaming.isStreaming) {
      return (
        <InferenceOutput
          response={{
            schema_version: '1.0',
            id: `stream-${Date.now()}`,
            text: streaming.streamedText,
            token_count: streaming.streamingState.tokenCount,
            tokens_generated: streaming.streamingState.tokenCount,
            latency_ms: streaming.streamingState.startTime ? Date.now() - streaming.streamingState.startTime : 0,
            finish_reason: 'stop',
            adapters_used: [],
            tokens: [],
            trace: {
              adapters_used: [],
              latency_ms: streaming.streamingState.startTime ? Date.now() - streaming.streamingState.startTime : 0,
              router_decisions: [],
            },
          } as InferResponse}
          isLoading={false}
          metrics={{
            latency: streaming.streamingState.startTime ? Date.now() - streaming.streamingState.startTime : 0,
            tokensPerSecond: streaming.tokensPerSecond,
            totalTokens: streaming.streamingState.tokenCount,
          }}
          isStreaming={true}
        />
      );
    }
    return (
      <InferenceOutput
        response={config.responseA}
        isLoading={config.isLoadingA}
        metrics={metrics}
        isStreaming={false}
      />
    );
  };

  return (
    <div className="space-y-6" data-cy="inference-page">
      {/* Live region for screen reader announcements */}
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        {config.isLoadingA && 'Generating response...'}
        {streaming.isStreaming && `Streaming: ${streaming.streamingState.tokenCount} tokens generated`}
        {config.responseA && !config.isLoadingA && !streaming.isStreaming && 'Response complete'}
      </div>

      <PageErrors errors={errors} />

      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

      {/* Header */}
      <ToolPageHeader
        title="Inference Playground"
        description={
          selectedStackId
            ? `Using stack: ${stacks.find((s) => s.id === selectedStackId)?.name || selectedStackId}`
            : 'Test model inference with advanced configuration options'
        }
        secondaryActions={
          <div className="flex gap-2">
            {/* Inference Mode Toggle */}
            <div className="flex gap-1 border rounded-md p-1" role="group" aria-label="Inference mode">
              <Button
                variant={inferenceMode === 'standard' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('standard')}
                aria-pressed={inferenceMode === 'standard'}
              >
                <Zap className="h-3 w-3 mr-1" aria-hidden="true" />
                Standard
              </Button>
              <GlossaryTooltip termId="inference-stream">
                <Button
                  variant={inferenceMode === 'streaming' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setInferenceMode('streaming')}
                  aria-pressed={inferenceMode === 'streaming'}
                >
                  <Wifi className="h-3 w-3 mr-1" aria-hidden="true" />
                  Streaming
                </Button>
              </GlossaryTooltip>
              <Button
                variant={inferenceMode === 'batch' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('batch')}
                aria-pressed={inferenceMode === 'batch'}
              >
                <Layers className="h-3 w-3 mr-1" aria-hidden="true" />
                Batch
              </Button>
            </div>

            {/* View Mode Toggle */}
            <div className="flex gap-1 border rounded-md p-1" role="group" aria-label="View mode">
              <Button
                variant={mode === 'single' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setMode('single')}
                aria-pressed={mode === 'single'}
              >
                <FileText className="h-3 w-3 mr-1" aria-hidden="true" />
                Single
              </Button>
              <GlossaryTooltip termId="inference-compare-mode">
                <Button
                  variant={mode === 'comparison' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setMode('comparison')}
                  aria-pressed={mode === 'comparison'}
                >
                  <Split className="h-3 w-3 mr-1" aria-hidden="true" />
                  Compare
                </Button>
              </GlossaryTooltip>
            </div>
          </div>
        }
      />

      {/* Performance Metrics */}
      {metrics && (
        <Card className="mb-4">
          <CardContent className="pt-4">
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-1">
                <Clock className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.latency}ms</span>
              </div>
              <div className="flex items-center gap-1">
                <TrendingUp className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.tokensPerSecond.toFixed(1)} tokens/sec</span>
              </div>
              <div className="flex items-center gap-1">
                <Target className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.totalTokens} tokens</span>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Main Content */}
      {inferenceMode === 'batch' ? (
        <SectionErrorBoundary sectionName="Batch Processing">
          <BatchProcessor
            prompts={batch.batchPrompts}
            results={batch.batchResults}
            validation={batch.batchValidation}
            isProcessing={batch.isBatchRunning}
            config={{
              max_tokens: config.configA.max_tokens || 100,
              temperature: config.configA.temperature || 0.7,
              top_k: config.configA.top_k || 50,
              top_p: config.configA.top_p ?? undefined,
            }}
            canExecute={can('inference:execute')}
            onPromptsChange={batch.setBatchPrompts}
            onProcess={batch.executeBatch}
            onRetry={async () => { toast.info('Batch retry not yet implemented'); }}
            onExportJSON={batch.exportResultsJSON}
            onExportCSV={batch.exportResultsCSV}
          />
        </SectionErrorBoundary>
      ) : mode === 'single' ? (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Configuration Panel */}
          <ConfigurationPanel
            backend={backend}
            coreml={coreml}
            adapter={adapter}
            config={config}
            streaming={streaming}
            inferenceMode={inferenceMode}
            stacks={stacks as Stack[]}
            selectedStackId={selectedStackId}
            defaultStackId={defaultStack?.id}
            selectedModelId={selectedModelId}
            selectedTenant={selectedTenant}
            onModelChange={handleModelChange}
            onStackChange={handleStackChange}
            onSetDefaultStack={async (id) => {
              if (!selectedTenant) {
                toast.error('No tenant selected');
                return;
              }
              await setDefaultStack(id);
            }}
            onClearStack={() => {
              setSelectedStackId('');
              adapter.setSelectedAdapterId('none');
            }}
            onInfer={handleInfer}
            onStreamingInfer={handleStreamingInfer}
            onCancel={handleCancel}
            onExport={handleExport}
            onSaveAsTemplate={() => setShowTemplateManager(true)}
            onSetDeterminismMode={handleSetDeterminismMode}
            canExecute={can('inference:execute')}
            isLoading={config.isLoadingA}
            isStreaming={streaming.isStreaming}
            hasResponse={!!config.responseA}
          />

          {/* Output Panel */}
          <div className="lg:col-span-2 space-y-4">
            <Card className="min-h-[calc(var(--base-unit)*150)]">
              <CardHeader>
                <CardTitle className="text-base">Output</CardTitle>
              </CardHeader>
              <CardContent>
                <SectionErrorBoundary sectionName="Inference Output">
                  {renderResponse()}
                </SectionErrorBoundary>
              </CardContent>
            </Card>

            <RunReceiptPanel
              response={config.responseA}
              requestedBackend={config.configA.backend}
              requestedDeterminismMode={config.configA.routing_determinism_mode}
            />

            {config.responseA?.run_receipt?.trace_id && (
              <TraceEvidencePanel
                traceId={config.responseA.run_receipt.trace_id}
                tenantId={selectedTenant}
                receiptDigest={config.responseA.run_receipt.receipt_digest}
              />
            )}

            {/* Session History */}
            <SessionHistoryPanel
              sessions={sessions.recentSessions}
              onLoadSession={handleLoadSession}
            />
          </div>
        </div>
      ) : (
        <SectionErrorBoundary sectionName="Comparison Mode">
          <ComparisonMode
            prompt={config.configA.prompt}
            configA={config.configA}
            configB={config.configB}
            responseA={config.responseA}
            responseB={config.responseB}
            isLoadingA={config.isLoadingA}
            isLoadingB={config.isLoadingB}
            isRunning={inferenceState.isRunning}
            canExecute={can('inference:execute')}
            metrics={metrics}
            onPromptChange={(value) => {
              config.setConfigA({ ...config.configA, prompt: value });
              config.setConfigB({ ...config.configB, prompt: value });
            }}
            onConfigAChange={config.setConfigA}
            onConfigBChange={config.setConfigB}
            onRunA={() => handleInfer()}
            onRunB={() => {
              // Run with config B
              toast.info('Config B inference coming soon');
            }}
            onCancel={handleCancel}
            onCopy={(text) => {
              navigator.clipboard.writeText(text);
              toast.success('Copied to clipboard');
            }}
            renderAdvancedOptions={() => null}
          />
        </SectionErrorBoundary>
      )}

      {/* Template Manager Dialog */}
      <PromptTemplateManager
        open={showTemplateManager}
        onOpenChange={setShowTemplateManager}
        onSelectTemplate={handleApplyTemplate}
      />
    </div>
  );
}

export function InferencePlayground(props: InferencePlaygroundProps) {
  return (
    <PageErrorsProvider>
      <InferencePlaygroundContent {...props} />
    </PageErrorsProvider>
  );
}
