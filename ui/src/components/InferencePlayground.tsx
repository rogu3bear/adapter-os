import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { validatePrompt, ValidationResult, MAX_PROMPT_LENGTH } from './inference/PromptInput';
import { AdvancedOptions } from './inference/AdvancedOptions';
import { InferenceOutput } from './inference/InferenceOutput';
import { TemplateManager } from './inference/TemplateManager';
import { BatchProcessor } from './inference/BatchProcessor';
import { ComparisonMode } from './inference/ComparisonMode';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import {
  Play,
  Download,
  History,
  Zap,
  Clock,
  Split,
  FileText,
  AlertTriangle,
  Code,
  Square,
  Wifi,
  Layers,
  TrendingUp,
  Target,
  Plus,
  HelpCircle
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { InferRequest, InferResponse, InferenceSession, Adapter, InferenceConfig } from '../api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { logger, toError } from '../utils/logger';
import { useSearchParams } from 'react-router-dom';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { useRBAC } from '@/hooks/useRBAC';
import { useProgressiveHints } from '../hooks/useProgressiveHints';
import { getPageHints } from '../data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { ToolPageHeader } from './ui/page-headers/ToolPageHeader';
import { useFeatureDegradation } from '../hooks/useFeatureDegradation';
import { useCancellableOperation } from '../hooks/useCancellableOperation';
import { PromptTemplateManager } from './PromptTemplateManager';
import { usePromptTemplates, PromptTemplate as PromptTemplateType } from '../hooks/usePromptTemplates';
import { InferenceRequestSchema, BatchPromptSchema } from '../schemas';

interface InferencePlaygroundProps {
  selectedTenant: string;
}

// Security: Input sanitization to prevent XSS and other injection attacks
const sanitizeInput = (input: string): string => {
  if (!input) return input;

  // Basic XSS prevention - remove potentially dangerous HTML/script tags
  const sanitized = input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '') // Remove script tags
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '') // Remove iframe tags
    .replace(/javascript:/gi, '') // Remove javascript: protocols
    .replace(/on\w+\s*=/gi, '') // Remove event handlers
    .replace(/<[^>]*>/g, '') // Remove all HTML tags as final fallback
    .trim();

  // Log if input was modified for security monitoring
  if (sanitized !== input) {
    logger.warn('Input sanitized for security', {
      component: 'InferencePlayground',
      operation: 'input_sanitization',
      originalLength: input.length,
      sanitizedLength: sanitized.length
    });
  }

  return sanitized;
};

// Privacy-aware monitoring (anonymized metrics only)
const recordPrivacySafeMetrics = (operation: string, data: any) => {
  // Remove any personally identifiable information
  const anonymized = { ...data };
  delete anonymized.userId;
  delete anonymized.email;
  delete anonymized.ip;
  delete anonymized.sessionId;

  logger.info(`Privacy-safe ${operation}`, {
    component: 'InferencePlayground',
    operation: `privacy_${operation}`,
    ...anonymized
  });
};


interface StreamingToken {
  token: string;
  timestamp: number;
}

interface StreamingState {
  isStreaming: boolean;
  streamedText: string;
  tokenCount: number;
  startTime: number | null;
  tokensPerSecond: number;
}

function InferencePlaygroundContent({ selectedTenant }: InferencePlaygroundProps) {
  const [searchParams] = useSearchParams();
  const { can, userRole } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const [mode, setMode] = useState<'single' | 'comparison'>('single');
  const [inferenceMode, setInferenceMode] = useState<'standard' | 'streaming' | 'batch'>('standard');
  const [prompt, setPrompt] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapterId, setSelectedAdapterId] = useState<string>('none');

  // Template management
  const { recordTemplateUsage, substituteVariables, getRecentTemplates } = usePromptTemplates();
  const [showTemplateManager, setShowTemplateManager] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<PromptTemplateType | null>(null);
  const [templateVariables, setTemplateVariables] = useState<Record<string, string>>({});
  const [showVariableInputs, setShowVariableInputs] = useState(false);
  const [promptModifiedSinceTemplate, setPromptModifiedSinceTemplate] = useState(false);

  // Additional state for metrics and batch operations
  const [metrics, setMetrics] = useState<any>(null);
  const [batchPrompts, setBatchPrompts] = useState<string[]>([]);
  const [batchValidation, setBatchValidation] = useState<ValidationResult[]>([]);
  const [batchResults, setBatchResults] = useState<any[]>([]);
  const [isBatchRunning, setIsBatchRunning] = useState(false);
  const [templates, setTemplates] = useState<PromptTemplateType[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);
  const [promptValidation, setPromptValidation] = useState<ValidationResult | null>(null);
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight });

  // Cancellation support for inference operations
  const { state: inferenceState, start: startInference, cancel: cancelInference } = useCancellableOperation();

  // Streaming inference state
  const [streamingState, setStreamingState] = useState<StreamingState>({
    isStreaming: false,
    streamedText: '',
    tokenCount: 0,
    startTime: null,
    tokensPerSecond: 0,
  });
  const abortControllerRef = React.useRef<AbortController | null>(null);

  // Graceful degradation: Monitor adapter availability
  const adapterAvailability = useFeatureDegradation({
    featureId: 'adapters',
    healthCheck: () => {
      // Check current adapter state, don't reload (that's handled by useEffect)
      return adapters.length > 0;
    },
    checkInterval: 30000,
  });

  // Progressive hints
  const hints = getPageHints('inference').map(hint => ({
    ...hint,
    condition: hint.id === 'no-adapters-inference'
      ? () => adapters.length === 0
      : hint.condition
  }));
  const { getVisibleHint, dismissHint } = useProgressiveHints({
    pageKey: 'inference',
    hints
  });
  const visibleHint = getVisibleHint();

  // Inference configurations
  const [configA, setConfigA] = useState<InferenceConfig>({
    id: 'a',
    prompt: '',
    max_tokens: 100,
    temperature: 0.7,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [configB, setConfigB] = useState<InferenceConfig>({
    id: 'b',
    prompt: '',
    max_tokens: 100,
    temperature: 0.9,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [responseA, setResponseA] = useState<InferResponse | null>(null);
  const [responseB, setResponseB] = useState<InferResponse | null>(null);
  const [isLoadingA, setIsLoadingA] = useState(false);
  const [isLoadingB, setIsLoadingB] = useState(false);
  const [recentSessions, setRecentSessions] = useState<InferenceSession[]>([]);

  // Missing function implementations (stubs)
  const addManagedSession = useCallback((session: InferenceSession) => {
    // Stub implementation - would add session to managed sessions
    logger.info('Adding managed session', { session });
    setRecentSessions(prev => [session, ...prev].slice(0, 10));
  }, []);

  const executeBatchInference = useCallback(async (prompts: string[]) => {
    if (prompts.length === 0) {
      toast.error('No prompts to process');
      return;
    }

    // Validate all prompts first using both custom validation and schema
    const validations = await Promise.all(prompts.map(async (p) => {
      const customValidation = validatePrompt(p);
      if (!customValidation.valid) {
        return customValidation;
      }

      // Also validate against schema
      try {
        await BatchPromptSchema.parseAsync({
          prompt: p,
          max_tokens: configA.max_tokens,
          temperature: configA.temperature,
        });
        return customValidation;
      } catch (error) {
        if (error instanceof Error) {
          return {
            valid: false,
            error: error.message,
          };
        }
        return customValidation;
      }
    }));

    setBatchValidation(validations);

    if (validations.some(v => !v.valid)) {
      toast.error('Some prompts have validation errors. Please fix them before proceeding.');
      return;
    }

    setIsBatchRunning(true);
    setBatchResults([]);

    logger.info('Executing batch inference', {
      component: 'InferencePlayground',
      operation: 'executeBatchInference',
      count: prompts.length
    });

    try {
      // Create batch request items
      const batchItems = prompts.map((prompt, idx) => ({
        id: `batch-${Date.now()}-${idx}`,
        prompt: sanitizeInput(prompt),
        max_tokens: configA.max_tokens,
        temperature: configA.temperature,
        top_k: configA.top_k,
        top_p: configA.top_p,
        seed: configA.seed,
        require_evidence: configA.require_evidence,
        adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
      }));

      // Call batch inference API
      const response = await apiClient.batchInfer({ requests: batchItems });

      setBatchResults(response.responses);

      const successCount = response.responses.filter(r => r.response).length;
      const errorCount = response.responses.filter(r => r.error).length;

      toast.success(`Batch complete: ${successCount} succeeded, ${errorCount} failed`);

      logger.info('Batch inference completed', {
        component: 'InferencePlayground',
        operation: 'executeBatchInference',
        total: prompts.length,
        success: successCount,
        errors: errorCount
      });
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Batch inference failed');
      addError('batch-inference', error.message, () => executeBatchInference(prompts));
      toast.error(`Batch inference failed: ${error.message}`);
      logger.error('Batch inference failed', {
        component: 'InferencePlayground',
        operation: 'executeBatchInference',
      }, toError(err));
    } finally {
      setIsBatchRunning(false);
    }
  }, [configA, selectedAdapterId, addError]);

  const handleApplyTemplate = useCallback((template: PromptTemplateType) => {
    logger.info('Applying template', { templateId: template.id, templateName: template.name });

    // Record usage
    recordTemplateUsage(template.id);

    // Set template and show variable inputs if needed
    setSelectedTemplate(template);
    setTemplateVariables({});
    setPromptModifiedSinceTemplate(false);

    if (template.variables.length > 0) {
      // Show variable inputs for substitution
      setShowVariableInputs(true);
    } else {
      // No variables, apply directly
      setConfigA({ ...configA, prompt: template.prompt });
      setPrompt(template.prompt);
      setShowTemplates(false);
    }
  }, [recordTemplateUsage, configA]);

  const handleApplyVariableSubstitution = useCallback(() => {
    if (!selectedTemplate) return;

    const substituted = substituteVariables(selectedTemplate.id, templateVariables);
    if (substituted) {
      setConfigA({ ...configA, prompt: substituted });
      setPrompt(substituted);
      setShowVariableInputs(false);
      setShowTemplates(false);
      logger.info('Variables substituted', { templateId: selectedTemplate.id, variableCount: Object.keys(templateVariables).length });
    }
  }, [selectedTemplate, templateVariables, substituteVariables, configA]);

  const handleResetToTemplate = useCallback(() => {
    if (!selectedTemplate) return;

    if (confirm('Reset prompt to template? Any manual edits will be lost.')) {
      setConfigA({ ...configA, prompt: selectedTemplate.prompt });
      setPrompt(selectedTemplate.prompt);
      setTemplateVariables({});
      setShowVariableInputs(false);
      setPromptModifiedSinceTemplate(false);
      logger.info('Prompt reset to template', { templateId: selectedTemplate.id });
    }
  }, [selectedTemplate, configA]);

  const handleSavePromptAsTemplate = useCallback(() => {
    // Delegate to template manager
    setShowTemplateManager(true);
  }, []);


  useEffect(() => {
    // Load recent sessions from localStorage
    const stored = localStorage.getItem('inference_sessions');
    if (stored) {
      try {
        setRecentSessions(JSON.parse(stored));
      } catch (err) {

        logger.error('Failed to parse stored inference sessions', {
          component: 'InferencePlayground',
          operation: 'loadSessions',
        }, toError(err));
      }
    }

    // Load adapters
    const loadAdapters = async () => {
      try {
        const adapterList = await apiClient.listAdapters();
        setAdapters(adapterList);

        // Check for adapter query parameter
        const adapterParam = searchParams.get('adapter');
        if (adapterParam) {
          // Try to find the adapter by ID or adapter_id
          const targetAdapter = adapterList.find((a: Adapter) =>
            a.id === adapterParam || a.adapter_id === adapterParam
          );
          if (targetAdapter) {
            setSelectedAdapterId(targetAdapter.id);
            // Success - no need for toast, UI updates
            return;
          } else {
            logger.warn('Requested adapter not found', {
              component: 'InferencePlayground',
              operation: 'loadAdapters',
              requestedAdapter: adapterParam,
            });
          }
        }

        // Fallback: Select first active adapter if available
        const activeAdapter = adapterList.find((a: Adapter) => ['hot', 'warm', 'resident'].includes(a.current_state));
        if (activeAdapter) {
          setSelectedAdapterId(activeAdapter.id);
        }
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to load adapters');
        logger.error('Failed to load adapters', {
          component: 'InferencePlayground',
          operation: 'loadAdapters',
        }, error);
        addError('adapters-load', error.message || 'Failed to load adapters. Inference will use base model only.', () => {
          clearError('adapters-load');
          apiClient.listAdapters().then(setAdapters).catch(err => {
            addError('adapters-load', err instanceof Error ? err.message : 'Failed to load adapters');
          });
        });
        // Don't block inference - allow graceful degradation with base model
      }
    };
    loadAdapters();
  }, [searchParams]);

  const saveSession = (config: InferenceConfig, response: InferResponse) => {
    const session: InferenceSession = {
      id: Date.now().toString(),
      created_at: new Date().toISOString(),
      prompt: config.prompt,
      request: config,
      response,
      status: 'completed',
    };

    // Use managed sessions to prevent memory leaks
    addManagedSession(session);

    const updated = [session, ...recentSessions].slice(0, 10); // Keep last 10
    setRecentSessions(updated);
    localStorage.setItem('inference_sessions', JSON.stringify(updated));
  };

  const handleInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    clearError('inference');
    setLoading(true);
    setResponse(null);

    try {
      // Validate prompt against schema
      const validationResult = await InferenceRequestSchema.parseAsync({
        prompt: config.prompt,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_k: config.top_k,
        top_p: config.top_p,
        seed: config.seed,
        require_evidence: config.require_evidence,
        adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
      });

      await startInference(async (signal) => {
        // Include adapters array if selected
        const inferenceRequest: InferRequest = {
          ...config,
          adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
        };
        const response = await apiClient.infer(inferenceRequest, {}, false, signal);
        setResponse(response);
        saveSession(config, response);
        return response;
      }, `inference-${config.id}`);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Inference failed');

      if (error.name === 'ZodError') {
        logger.warn('Inference validation failed', {
          component: 'InferencePlayground',
          operation: 'validate',
          configId: config.id,
        });
        addError('inference', `Validation error: ${error.message}`, () => handleInfer(config, setResponse, setLoading));
      } else {
        logger.error('Inference request failed', {
          component: 'InferencePlayground',
          operation: 'infer',
          configId: config.id,
          tenantId: selectedTenant,
          adapterId: selectedAdapterId,
        }, toError(err));
        addError('inference', error.message || 'An unexpected error occurred during inference.', () => handleInfer(config, setResponse, setLoading));
      }
    } finally {
      setLoading(false);
    }
  };

  // Streaming inference handler
  const handleStreamingInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    clearError('inference');
    setLoading(true);
    setResponse(null);

    // Reset streaming state
    setStreamingState({
      isStreaming: true,
      streamedText: '',
      tokenCount: 0,
      startTime: Date.now(),
      tokensPerSecond: 0,
    });

    // Create abort controller for cancellation
    abortControllerRef.current = new AbortController();
    const startTime = Date.now();
    let tokenCount = 0;

    try {
      // Validate prompt against schema
      await InferenceRequestSchema.parseAsync({
        prompt: config.prompt,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_k: config.top_k,
        top_p: config.top_p,
        seed: config.seed,
        require_evidence: config.require_evidence,
        adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
      });

      await apiClient.streamInfer(
        {
          prompt: config.prompt,
          max_tokens: config.max_tokens,
          temperature: config.temperature,
          top_k: config.top_k,
          top_p: config.top_p,
          seed: config.seed,
          adapter_stack: selectedAdapterId && selectedAdapterId !== 'none' ? selectedAdapterId : undefined,
        },
        {
          onToken: (token, chunk) => {
            tokenCount++;
            const elapsed = (Date.now() - startTime) / 1000;
            const tokensPerSecond = elapsed > 0 ? tokenCount / elapsed : 0;

            setStreamingState(prev => ({
              ...prev,
              streamedText: prev.streamedText + token,
              tokenCount,
              tokensPerSecond,
            }));
          },
          onComplete: (fullText, finishReason) => {
            const elapsed = Date.now() - startTime;

            // Map streaming finish reason to InferResponse finish reason
            const mapFinishReason = (reason: string | null): 'stop' | 'length' | 'error' => {
              if (reason === 'length') return 'length';
              if (reason === 'content_filter' || reason === 'error' || reason === 'cancelled') return 'error';
              return 'stop';
            };

            // Build final response (partial - streaming doesn't have all fields)
            const finalResponse = {
              schema_version: '1.0',
              id: `stream-${Date.now()}`,
              text: fullText,
              tokens_generated: tokenCount,
              token_count: tokenCount,
              latency_ms: elapsed,
              adapters_used: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : [],
              finish_reason: mapFinishReason(finishReason),
            } as InferResponse;

            setResponse(finalResponse);
            saveSession(config, finalResponse);

            // Update metrics
            setMetrics({
              latency: elapsed,
              tokensPerSecond: tokenCount / (elapsed / 1000),
              totalTokens: tokenCount,
            });

            setStreamingState(prev => ({
              ...prev,
              isStreaming: false,
            }));
            setLoading(false);

            logger.info('Streaming inference completed', {
              component: 'InferencePlayground',
              operation: 'streamingInfer',
              tokenCount,
              latencyMs: elapsed,
              finishReason,
            });
          },
          onError: (error) => {
            addError('inference', error.message || 'Streaming inference failed.', () => handleStreamingInfer(config, setResponse, setLoading));
            setStreamingState(prev => ({
              ...prev,
              isStreaming: false,
            }));
            setLoading(false);

            logger.error('Streaming inference failed', {
              component: 'InferencePlayground',
              operation: 'streamingInfer',
              configId: config.id,
            }, error);
          },
        },
        abortControllerRef.current.signal
      );
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Streaming inference failed');

      if (error.name === 'ZodError') {
        logger.warn('Streaming inference validation failed', {
          component: 'InferencePlayground',
          operation: 'validate',
          configId: config.id,
        });
        addError('inference', `Validation error: ${error.message}`, () => handleStreamingInfer(config, setResponse, setLoading));
      } else {
        logger.error('Streaming inference request failed', {
          component: 'InferencePlayground',
          operation: 'streamingInfer',
          configId: config.id,
          tenantId: selectedTenant,
          adapterId: selectedAdapterId,
        }, toError(err));
        addError('inference', error.message || 'An unexpected error occurred during streaming inference.', () => handleStreamingInfer(config, setResponse, setLoading));
      }

      setStreamingState(prev => ({
        ...prev,
        isStreaming: false,
      }));
      setLoading(false);
    }
  };

  // Cancel streaming inference
  const cancelStreamingInfer = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);


  const handleExport = (config: InferenceConfig, response: InferResponse | null) => {
    if (!response) return;

    const data = {
      prompt: config.prompt,
      config,
      response,
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
    // Success - browser download feedback is sufficient
  };

  const handleBatchExportJSON = useCallback(() => {
    if (batchResults.length === 0) return;

    const data = {
      batchSize: batchResults.length,
      timestamp: new Date().toISOString(),
      config: {
        max_tokens: configA.max_tokens,
        temperature: configA.temperature,
        top_k: configA.top_k,
        top_p: configA.top_p,
        seed: configA.seed,
        require_evidence: configA.require_evidence,
        adapter: selectedAdapterId !== 'none' ? selectedAdapterId : null,
      },
      results: batchResults.map((result, idx) => ({
        id: result.id,
        prompt: batchPrompts[idx] || '',
        response: result.response?.text,
        token_count: result.response?.token_count,
        latency_ms: result.response?.latency_ms,
        finish_reason: result.response?.finish_reason,
        error: result.error?.error,
      })),
    };

    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `batch-inference-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    logger.info('Batch results exported as JSON', {
      component: 'InferencePlayground',
      operation: 'exportJSON',
      resultCount: batchResults.length,
    });
  }, [batchResults, batchPrompts, configA, selectedAdapterId]);

  const handleBatchExportCSV = useCallback(() => {
    if (batchResults.length === 0) return;

    // CSV header
    const headers = ['ID', 'Prompt', 'Status', 'Response', 'Token Count', 'Latency (ms)', 'Finish Reason', 'Error'];

    // CSV rows
    const rows = batchResults.map((result, idx) => {
      const prompt = (batchPrompts[idx] || '').replace(/"/g, '""'); // Escape quotes
      const response = (result.response?.text || '').replace(/"/g, '""');
      const error = (result.error?.error || '').replace(/"/g, '""');
      const status = result.error ? 'Error' : result.response ? 'Success' : 'Pending';

      return [
        result.id,
        `"${prompt}"`,
        status,
        `"${response}"`,
        result.response?.token_count || '',
        result.response?.latency_ms || '',
        result.response?.finish_reason || '',
        `"${error}"`,
      ].join(',');
    });

    const csv = [headers.join(','), ...rows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `batch-inference-${Date.now()}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    logger.info('Batch results exported as CSV', {
      component: 'InferencePlayground',
      operation: 'exportCSV',
      resultCount: batchResults.length,
    });
  }, [batchResults, batchPrompts]);

  const handleBatchRetry = useCallback(async (itemId: string) => {
    const index = batchResults.findIndex(r => r.id === itemId);
    if (index === -1) return;

    const prompt = batchPrompts[index];
    if (!prompt) return;

    logger.info('Retrying batch item', {
      component: 'InferencePlayground',
      operation: 'retryBatchItem',
      itemId,
    });

    try {
      const batchItem = {
        id: `retry-${Date.now()}`,
        prompt: sanitizeInput(prompt),
        max_tokens: configA.max_tokens,
        temperature: configA.temperature,
        top_k: configA.top_k,
        top_p: configA.top_p,
        seed: configA.seed,
        require_evidence: configA.require_evidence,
        adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
      };

      const response = await apiClient.batchInfer({ requests: [batchItem] });

      // Update the result in the batch results
      const newResults = [...batchResults];
      newResults[index] = response.responses[0];
      setBatchResults(newResults);

      if (response.responses[0].error) {
        toast.error('Retry failed');
      } else {
        toast.success('Retry successful');
      }
    } catch (err) {
      toast.error('Retry failed');
      logger.error('Batch retry failed', {
        component: 'InferencePlayground',
        operation: 'retryBatchItem',
        itemId,
      }, toError(err));
    }
  }, [batchResults, batchPrompts, configA, selectedAdapterId]);

  const loadSession = (session: InferenceSession) => {
    setPrompt(session.prompt);
    setConfigA({ ...configA, prompt: session.prompt, ...session.request });
    if (session.response) {
      setResponseA(session.response);
    }
    // Success - UI updates are sufficient feedback
  };

  const handleReplay = async (bundleId: string) => {
    try {
      logger.info('Replay requested', {
        component: 'InferencePlayground',
        operation: 'handleReplay',
        bundleId
      });

      const session = await apiClient.getReplaySession(bundleId);

      if (session) {
        // Restore prompt from session
        setPrompt(session.prompt || '');
        setConfigA(prev => ({
          ...prev,
          prompt: session.prompt || '',
          max_tokens: session.config?.max_tokens || prev.max_tokens,
          temperature: session.config?.temperature || prev.temperature,
        }));

        toast.success('Session restored from replay');
        logger.info('Replay session loaded', {
          component: 'InferencePlayground',
          operation: 'handleReplay',
          sessionId: session.id
        });
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load replay session';
      logger.error('Replay failed', {
        component: 'InferencePlayground',
        operation: 'handleReplay',
        bundleId
      }, error instanceof Error ? error : new Error(errorMessage));
      toast.error(`Failed to load replay: ${errorMessage}`);
    }
  };

  const renderAdvancedOptions = (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => (
    <AdvancedOptions
      values={{
        max_tokens: config.max_tokens || 100,
        temperature: config.temperature || 0.7,
        top_k: config.top_k || 50,
        top_p: config.top_p || 0.9,
        seed: config.seed,
        require_evidence: config.require_evidence || false
      }}
      onChange={(values) => setConfig({ ...config, ...values })}
      isOpen={showAdvanced}
      onOpenChange={setShowAdvanced}
    />
  );


  const renderResponse = (response: InferResponse | null, isLoading: boolean) => {
    // When streaming is active, show streaming output
    if (inferenceMode === 'streaming' && streamingState.isStreaming) {
      return (
        <InferenceOutput
          response={{
            text: streamingState.streamedText,
            token_count: streamingState.tokenCount,
            latency_ms: streamingState.startTime ? Date.now() - streamingState.startTime : 0,
            finish_reason: null,
          } as InferResponse}
          isLoading={false}
          metrics={{
            latency: streamingState.startTime ? Date.now() - streamingState.startTime : 0,
            tokensPerSecond: streamingState.tokensPerSecond,
            totalTokens: streamingState.tokenCount,
          }}
          isStreaming={true}
        />
      );
    }

    return (
      <InferenceOutput
        response={response}
        isLoading={isLoading}
        metrics={metrics}
        isStreaming={inferenceMode === 'streaming'}
      />
    );
  };

  return (
    <div className="space-y-6">

      {/* Consolidated Error Display */}
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
        description="Test model inference with advanced configuration options"
        secondaryActions={
          <div className="flex gap-2">
            <div className="flex gap-1 border rounded-md p-1">
              <Button
                variant={inferenceMode === 'standard' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('standard')}
              >
                <Zap className="h-3 w-3 mr-1" />
                Standard
              </Button>
              <HelpTooltip helpId="inference-stream">
                <Button
                  variant={inferenceMode === 'streaming' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setInferenceMode('streaming')}
                >
                  <Wifi className="h-3 w-3 mr-1" />
                  Streaming
                </Button>
              </HelpTooltip>
              <Button
                variant={inferenceMode === 'batch' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('batch')}
              >
                <Layers className="h-3 w-3 mr-1" />
                Batch
              </Button>
            </div>

            {/* Single vs Comparison Mode */}
            <div className="flex gap-1 border rounded-md p-1">
              <Button
                variant={mode === 'single' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setMode('single')}
              >
                <FileText className="h-3 w-3 mr-1" />
                Single
              </Button>
              <HelpTooltip helpId="inference-compare-mode">
                <Button
                  variant={mode === 'comparison' ? 'default' : 'ghost'}
                  size="sm"
                  onClick={() => setMode('comparison')}
                >
                  <Split className="h-3 w-3 mr-1" />
                  Compare
                </Button>
              </HelpTooltip>
            </div>
          </div>
        }
      />

      {/* Performance Metrics Display */}
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

      {inferenceMode === 'batch' ? (
        /* Batch Mode */
        <BatchProcessor
          prompts={batchPrompts}
          results={batchResults}
          validation={batchValidation}
          isProcessing={isBatchRunning}
          config={{
            max_tokens: configA.max_tokens || 100,
            temperature: configA.temperature || 0.7,
            top_k: configA.top_k || 50,
            top_p: configA.top_p,
          }}
          canExecute={can('inference:execute')}
          onPromptsChange={setBatchPrompts}
          onProcess={executeBatchInference}
          onRetry={handleBatchRetry}
          onExportJSON={handleBatchExportJSON}
          onExportCSV={handleBatchExportCSV}
        />
      ) : mode === 'single' ? (
        /* Single Mode */
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Configuration Panel */}
          <div className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Configuration</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">

                {/* Graceful degradation alert */}
                {adapterAvailability.isDegraded && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      {adapters.length === 0
                        ? 'No adapters available. Inference will use base model only.'
                        : 'Adapter loading issues detected. Some adapters may be unavailable.'}
                      {adapterAvailability.failureCount === 0 && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => adapterAvailability.checkHealth()}
                          className="ml-2"
                        >
                          Retry
                        </Button>
                      )}
                    </AlertDescription>
                  </Alert>
                )}
                <div className="space-y-2">
                  <Label htmlFor="adapter" className="flex items-center gap-1">
                    Adapter {adapters.length === 0 && <span className="text-muted-foreground text-xs">(None - base model only)</span>}
                    <HelpTooltip helpId="inference-adapter-stack">
                      <span className="cursor-help text-muted-foreground hover:text-foreground">
                        <HelpCircle className="h-3 w-3" />
                      </span>
                    </HelpTooltip>
                  </Label>
                  <Select value={selectedAdapterId} onValueChange={setSelectedAdapterId} disabled={adapters.length === 0}>
                    <SelectTrigger id="adapter">
                      <SelectValue placeholder={adapters.length === 0 ? "No adapters available" : "Select adapter or use default..."} />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">Default (No adapter)</SelectItem>
                      {adapters.filter(adapter => adapter.id && adapter.id !== '').map((adapter) => (
                        <SelectItem key={adapter.id} value={adapter.id}>
                          <div className="flex items-center gap-2">
                            <Code className="h-4 w-4" aria-hidden="true" />
                            <span>{adapter.name}</span>
                            <span className="text-xs text-muted-foreground">
                              ({adapter.current_state})
                            </span>
                          </div>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    {adapters.length === 0
                      ? 'No adapters available. Inference will use base model only.'
                      : 'Select a trained adapter to use for inference. Leave empty to use base model.'}
                  </p>
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="prompt" className="flex items-center gap-1">
                      Prompt
                      <HelpTooltip helpId="inference-prompt">
                        <span className="cursor-help text-muted-foreground hover:text-foreground">
                          <HelpCircle className="h-3 w-3" />
                        </span>
                      </HelpTooltip>
                      <span className="sr-only">
                        Use Ctrl+G or Cmd+G to generate, Ctrl+S or Cmd+S to toggle streaming mode, Ctrl+B or Cmd+B to toggle batch mode, Escape to cancel
                      </span>
                    </Label>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setShowTemplates(!showTemplates)}
                        className="h-8 px-2"
                        aria-label={showTemplates ? "Hide prompt templates" : "Show prompt templates"}
                      >
                        <FileText className="h-3 w-3 mr-1" />
                        Templates
                      </Button>
                    </div>
                  </div>
                  <Textarea
                    id="prompt"
                    placeholder="Enter your prompt here..."
                    value={configA.prompt}

                    onChange={(e) => {
                      const sanitized = sanitizeInput(e.target.value);
                      setConfigA({ ...configA, prompt: sanitized });
                      setPromptValidation(validatePrompt(sanitized));

                      // Track if prompt has been modified since template was applied
                      if (selectedTemplate) {
                        setPromptModifiedSinceTemplate(sanitized !== selectedTemplate.prompt);
                      }
                    }}
                    rows={6}
                    className={promptValidation?.valid === false ? 'border-destructive' : ''}
                    aria-describedby={promptValidation?.error ? "prompt-error" : promptValidation?.warning ? "prompt-warning" : undefined}
                    aria-invalid={promptValidation?.valid === false}
                  />
                  {promptValidation?.error && (
                    <Alert variant="destructive" className="text-sm" id="prompt-error">
                      <AlertTriangle className="h-4 w-4" />
                      <AlertDescription>
                        <strong>Validation Error:</strong> {promptValidation.error}
                        {promptValidation.suggestion && (
                          <div className="mt-1 text-sm opacity-90">
                            <strong>Suggestion:</strong> {promptValidation.suggestion}
                          </div>
                        )}
                      </AlertDescription>
                    </Alert>
                  )}
                  {promptValidation?.warning && (
                    <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50" id="prompt-warning">
                      <AlertTriangle className="h-4 w-4 text-yellow-600" />
                      <AlertDescription className="text-yellow-800">
                        <strong>Warning:</strong> {promptValidation.warning}
                      </AlertDescription>
                    </Alert>
                  )}
                  {promptValidation?.valid === false && !promptValidation.error && (
                    <div className="text-xs text-muted-foreground">
                      Character count: {configA.prompt.length.toLocaleString()} / {MAX_PROMPT_LENGTH.toLocaleString()}
                    </div>
                  )}
                  {windowSize.width < 768 && (
                    <div className="text-xs text-muted-foreground mt-1">
                      💡 Swipe left/right to change modes, swipe up for templates
                    </div>
                  )}
                  {/* Template Management */}
                  <TemplateManager
                    templates={templates}
                    recentTemplates={getRecentTemplates()}
                    selectedTemplate={selectedTemplate}
                    templateVariables={templateVariables}
                    showTemplates={showTemplates}
                    showVariableInputs={showVariableInputs}
                    promptModifiedSinceTemplate={promptModifiedSinceTemplate}
                    onSelect={handleApplyTemplate}
                    onApplyVariables={handleApplyVariableSubstitution}
                    onResetToTemplate={handleResetToTemplate}
                    onSaveAsTemplate={handleSavePromptAsTemplate}
                    onManageTemplates={() => setShowTemplateManager(true)}
                    onToggleTemplates={() => setShowTemplates(!showTemplates)}
                    onCancelVariables={() => {
                      setShowVariableInputs(false);
                      setSelectedTemplate(null);
                      setTemplateVariables({});
                    }}
                    onVariableChange={(variable, value) =>
                      setTemplateVariables({
                        ...templateVariables,
                        [variable]: value,
                      })
                    }
                    substituteVariables={substituteVariables}
                  />
                </div>

                {renderAdvancedOptions(configA, setConfigA)}


                <div className="flex gap-2">
                  <Button
                    className={`flex-1 ${!can('inference:execute') ? 'opacity-50 cursor-not-allowed' : ''}`}
                    onClick={() => {
                      if (inferenceMode === 'streaming') {
                        handleStreamingInfer(configA, setResponseA, setIsLoadingA);
                      } else {
                        handleInfer(configA, setResponseA, setIsLoadingA);
                      }
                    }}
                    disabled={isLoadingA || streamingState.isStreaming || !can('inference:execute')}
                    aria-label="Run inference with current configuration"
                    title={!can('inference:execute') ? 'Requires inference:execute permission' : undefined}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    {isLoadingA || streamingState.isStreaming ? 'Generating...' : 'Generate'}
                  </Button>
                  {(inferenceState.isRunning || streamingState.isStreaming) && (
                    <Button
                      variant="outline"
                      onClick={() => {
                        if (streamingState.isStreaming) {
                          cancelStreamingInfer();
                        } else {
                          cancelInference();
                        }
                      }}
                      aria-label="Cancel inference"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>

                {responseA && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => handleExport(configA, responseA)}
                  >
                    <Download className="h-4 w-4 mr-2" />
                    Export
                  </Button>
                )}

                {configA.prompt && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={handleSavePromptAsTemplate}
                  >
                    <Plus className="h-4 w-4 mr-2" />
                    Save Prompt as Template
                  </Button>
                )}
              </CardContent>
            </Card>

            {/* Recent Sessions */}
            {recentSessions.length > 0 && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <History className="h-4 w-4" aria-hidden="true" />
                    Recent Sessions
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-2">
                  {recentSessions.slice(0, 5).map((session) => (
                    <Button
                      key={session.id}
                      variant="ghost"
                      className="w-full justify-start text-left h-auto py-2"
                      onClick={() => loadSession(session)}
                    >
                      <div className="truncate">
                        <p className="text-sm truncate">{session.prompt}</p>
                        <p className="text-xs text-muted-foreground">
                          {new Date(session.created_at).toLocaleString()}
                        </p>
                      </div>
                    </Button>
                  ))}
                </CardContent>
              </Card>
            )}
          </div>

          {/* Response Panel */}
          <div className="lg:col-span-2">
            <Card className="min-h-[600px]">
              <CardHeader>
                <CardTitle className="text-base">Output</CardTitle>
              </CardHeader>
              <CardContent>
                {renderResponse(responseA, isLoadingA)}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        /* Comparison Mode */
        <ComparisonMode
          prompt={prompt}
          configA={configA}
          configB={configB}
          responseA={responseA}
          responseB={responseB}
          isLoadingA={isLoadingA}
          isLoadingB={isLoadingB}
          isRunning={inferenceState.isRunning}
          canExecute={can('inference:execute')}
          metrics={metrics}
          onPromptChange={(value) => {
            setPrompt(value);
            setConfigA({ ...configA, prompt: value });
            setConfigB({ ...configB, prompt: value });
          }}
          onConfigAChange={setConfigA}
          onConfigBChange={setConfigB}
          onRunA={() => handleInfer(configA, setResponseA, setIsLoadingA)}
          onRunB={() => handleInfer(configB, setResponseB, setIsLoadingB)}
          onCancel={cancelInference}
          onCopy={(text) => {
            navigator.clipboard.writeText(text);
            toast.success('Copied to clipboard');
          }}
          renderAdvancedOptions={renderAdvancedOptions}
        />
      )}

      {/* Prompt Template Manager Dialog */}
      <PromptTemplateManager
        open={showTemplateManager}
        onOpenChange={setShowTemplateManager}
        onSelectTemplate={handleApplyTemplate}
      />
    </div>
  );
}

// Wrap with PageErrorsProvider
export function InferencePlayground(props: InferencePlaygroundProps) {
  return (
    <PageErrorsProvider>
      <InferencePlaygroundContent {...props} />
    </PageErrorsProvider>
  );
}
