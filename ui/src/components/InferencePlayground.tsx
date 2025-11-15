import React, { useState, useEffect, useCallback } from 'react';
import { useCancellableOperation } from '../hooks/useCancellableOperation';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Slider } from './ui/slider';
import { Checkbox } from './ui/checkbox';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import {
  Play,
  Copy,
  Download,
  History,
  Settings2,
  ChevronDown,
  Zap,
  Clock,
  BarChart3,
  Split,
  FileText,
  AlertTriangle,
  CheckCircle,
  Code,
  Square,
  Wifi,
  Layers,
  TrendingUp,
  Target
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { InferRequest, InferResponse, InferenceSession, Adapter } from '../api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
// 【ui/src/components/InferencePlayground.tsx§1-39】 - Replace toast errors with ErrorRecovery
import { TraceVisualizer } from './TraceVisualizer';
import { logger, toError } from '../utils/logger';
import { useSearchParams } from 'react-router-dom';
import { ErrorRecovery, ErrorRecoveryTemplates } from '@/components/ui/error-recovery';
import { useProgressiveHints } from '../hooks/useProgressiveHints';
import { getPageHints } from '../data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { ToolPageHeader } from './ui/page-headers/ToolPageHeader';
import { useFeatureDegradation } from '../hooks/useFeatureDegradation';

interface InferencePlaygroundProps {
  selectedTenant: string;
}

interface ValidationResult {
  valid: boolean;
  error?: string;
  warning?: string;
  suggestion?: string;
}

// Input validation utilities for edge cases
const MAX_PROMPT_LENGTH = 50000; // 50KB character limit
const MAX_PROMPT_BYTES = 100000; // 100KB byte limit

const validatePromptLength = (prompt: string): ValidationResult => {
  if (prompt.length > MAX_PROMPT_LENGTH) {
    return {
      valid: false,
      error: `Prompt too long (${prompt.length.toLocaleString()} characters). Maximum: ${MAX_PROMPT_LENGTH.toLocaleString()}`,
      suggestion: 'Consider breaking into smaller chunks or using batch processing for large inputs'
    };
  }

  const byteLength = new Blob([prompt]).size;
  if (byteLength > MAX_PROMPT_BYTES) {
    return {
      valid: false,
      error: `Prompt size too large (${(byteLength / 1024).toFixed(1)}KB). Maximum: ${(MAX_PROMPT_BYTES / 1024).toFixed(0)}KB`,
      suggestion: 'Reduce content size or consider using file upload for large documents'
    };
  }

  if (prompt.length > MAX_PROMPT_LENGTH * 0.8) {
    return {
      valid: true,
      warning: `Approaching character limit (${prompt.length.toLocaleString()}/${MAX_PROMPT_LENGTH.toLocaleString()})`
    };
  }

  return { valid: true };
};

const validateUnicodeContent = (text: string): ValidationResult => {
  try {
    // Normalize to NFC form for consistent processing
    const normalized = text.normalize('NFC');

    // Check for problematic Unicode ranges (control characters except common whitespace)
    const hasProblematicUnicode = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D]/.test(normalized);
    if (hasProblematicUnicode) {
      return {
        valid: false,
        error: 'Prompt contains unsupported control or invisible characters',
        suggestion: 'Remove or replace invisible characters, zero-width spaces, or control characters'
      };
    }

    // Check for excessive emoji usage (potential spam/abuse)
    const emojiCount = (normalized.match(/\p{Emoji}/gu) || []).length;
    const textLength = normalized.replace(/\p{Emoji}/gu, '').length;
    if (emojiCount > textLength * 0.5 && emojiCount > 20) {
      return {
        valid: false,
        error: 'Too many emojis detected',
        suggestion: 'Reduce emoji usage or use descriptive text instead'
      };
    }

    return { valid: true };
  } catch (error) {
    return {
      valid: false,
      error: 'Unicode processing failed - text may contain invalid characters',
      suggestion: 'Try re-entering the text or copy from a different source'
    };
  }
};

const validatePromptContent = (prompt: string): ValidationResult => {
  if (!prompt || prompt.trim().length === 0) {
    return {
      valid: false,
      error: 'Prompt cannot be empty',
      suggestion: 'Please enter a question or instruction for the AI model'
    };
  }

  // Check for invisible Unicode characters that would be trimmed
  const visibleChars = prompt.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D\s]/g, '');
  if (visibleChars.length === 0) {
    return {
      valid: false,
      error: 'Prompt contains only invisible characters or whitespace',
      suggestion: 'Please enter meaningful text content'
    };
  }

  // Minimum meaningful length check (accounting for Unicode)
  const normalizedLength = prompt.normalize('NFC').trim().length;
  if (normalizedLength < 3) {
    return {
      valid: false,
      error: 'Prompt too short',
      suggestion: 'Please provide more context (minimum 3 characters)'
    };
  }

  return { valid: true };
};

const validatePrompt = (prompt: string): ValidationResult => {
  // Run all validations in order
  const lengthValidation = validatePromptLength(prompt);
  if (!lengthValidation.valid) return lengthValidation;

  const contentValidation = validatePromptContent(prompt);
  if (!contentValidation.valid) return contentValidation;

  const unicodeValidation = validateUnicodeContent(prompt);
  if (!unicodeValidation.valid) return unicodeValidation;

  // Combine warnings if any
  const warnings = [lengthValidation.warning, contentValidation.warning, unicodeValidation.warning]
    .filter(Boolean)
    .join('; ');

  return {
    valid: true,
    ...(warnings && { warning: warnings })
  };
};

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

interface InferenceConfig extends InferRequest {
  id: string;
}

interface StreamingToken {
  token: string;
  timestamp: number;
}

interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  prompt: string;
  category: string;
}

export function InferencePlayground({ selectedTenant }: InferencePlaygroundProps) {
  const [searchParams] = useSearchParams();
  const [mode, setMode] = useState<'single' | 'comparison'>('single');
  const [inferenceMode, setInferenceMode] = useState<'standard' | 'streaming' | 'batch'>('standard');
  const [prompt, setPrompt] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapterId, setSelectedAdapterId] = useState<string>('none');
  const [inferenceError, setInferenceError] = useState<Error | null>(null);
  const [adaptersLoadError, setAdaptersLoadError] = useState<Error | null>(null);

  // Additional state for streaming, metrics, and batch operations
  const [metrics, setMetrics] = useState<any>(null);
  const [isStreaming, setIsStreaming] = useState(false);
  const [throttledStreamingTokens, setThrottledStreamingTokens] = useState<string>('');
  const [streamingTokens, setStreamingTokens] = useState<string>('');
  const [streamController, setStreamController] = useState<AbortController | null>(null);
  const streamingRef = React.useRef<boolean>(false);
  const [batchPrompts, setBatchPrompts] = useState<string[]>([]);
  const [batchValidation, setBatchValidation] = useState<ValidationResult[]>([]);
  const [batchResults, setBatchResults] = useState<any[]>([]);
  const [isBatchRunning, setIsBatchRunning] = useState(false);
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);
  const [promptValidation, setPromptValidation] = useState<ValidationResult | null>(null);
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight });

  // Cancellation support for inference operations
  const { state: inferenceState, start: startInference, cancel: cancelInference } = useCancellableOperation();

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
  const executeInference = useCallback(async (config: any) => {
    // Stub implementation - would execute inference request
    logger.info('Executing inference', { config });
    return null;
  }, []);

  const addManagedSession = useCallback((session: InferenceSession) => {
    // Stub implementation - would add session to managed sessions
    logger.info('Adding managed session', { session });
    setRecentSessions(prev => [session, ...prev].slice(0, 10));
  }, []);

  const executeBatchInference = useCallback(async (prompts: string[]) => {
    // Stub implementation - would execute batch inference
    logger.info('Executing batch inference', { count: prompts.length });
    setIsBatchRunning(true);
    // Simulate batch processing
    setTimeout(() => setIsBatchRunning(false), 1000);
    return [];
  }, []);

  const applyTemplate = useCallback((template: PromptTemplate) => {
    // Stub implementation - would apply prompt template
    logger.info('Applying template', { template });
    setPrompt(template.prompt);
    setShowTemplates(false);
  }, []);

  // Enhanced error recovery with contextual suggestions
  const getInferenceErrorRecovery = useCallback((error: Error) => {
    const errorMessage = error.message.toLowerCase();

    if (errorMessage.includes('network') || errorMessage.includes('fetch')) {
      return ErrorRecoveryTemplates.networkError(() => executeInference(configA));
    }

    if (errorMessage.includes('auth') || errorMessage.includes('401')) {
      return ErrorRecoveryTemplates.authError(() => executeInference(configA));
    }

    return ErrorRecoveryTemplates.genericError(
      error.message,
      () => executeInference(configA)
    );
  }, [configA]);

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
        setAdaptersLoadError(error);
        // Don't set inferenceError - allow graceful degradation with base model
      }
    };
    loadAdapters();
  }, [searchParams]);

  const saveSession = (config: InferenceConfig, response: InferResponse) => {
    // Convert InferResponse to EnhancedInferResponse for session storage
    const enhancedResponse = {
      ...response,
      token_count: response.token_count || 0,
      finish_reason: response.finish_reason || 'stop',
      latency_ms: response.latency_ms || 0,
      trace: response.trace,
    };
    
    const session: InferenceSession = {
      id: Date.now().toString(),
      created_at: new Date().toISOString(),
      prompt: config.prompt,
      request: config,
      response: enhancedResponse as any, // Type compatibility
      status: 'completed',
    };

    // Use managed sessions to prevent memory leaks
    addManagedSession(session);
  };

  const handleInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    if (!config.prompt.trim()) {
      setInferenceError(new Error('Please enter a prompt'));
      return;
    }

    setInferenceError(null);
    setLoading(true);
    setResponse(null);

    try {
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
      if (err) { // Only set error if it's not a cancellation
        const error = err instanceof Error ? err : new Error('Inference failed');
        setInferenceError(error);
        logger.error('Inference request failed', {
          component: 'InferencePlayground',
          operation: 'infer',
          configId: config.id,
          tenantId: selectedTenant,
          adapterId: selectedAdapterId,
        }, toError(err));
      }
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    // Success - no need for toast, UI feedback is sufficient
  };

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

  const loadSession = (session: InferenceSession) => {
    setPrompt(session.prompt);
    setConfigA({ ...configA, prompt: session.prompt, ...session.request });
    if (session.response) {
      setResponseA(session.response);
    }
    // Success - UI updates are sufficient feedback
  };

  const handleReplay = async (bundleId: string) => {
    // TODO: Implement replay functionality when API client supports it
    logger.info('Replay requested', { bundleId });
    // const trace = await apiClient.getReplayBundle(bundleId);
    // setTrace(trace.data); // Display bundle
  };

  const renderAdvancedOptions = (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => (
    <Collapsible open={showAdvanced} onOpenChange={setShowAdvanced}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between" aria-label="Toggle advanced options" aria-expanded={showAdvanced}>
          <span className="flex items-center gap-2">
            <Settings2 className="h-4 w-4" aria-hidden="true" />
            Advanced Options
          </span>
          <ChevronDown className={`h-4 w-4 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-4 pt-4">
        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Max Tokens</Label>
            <span className="text-sm text-muted-foreground">{config.max_tokens}</span>
          </div>
          <Slider
            value={[config.max_tokens || 100]}
            onValueChange={(v) => setConfig({ ...config, max_tokens: v[0] })}
            min={10}
            max={2000}
            step={10}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Temperature</Label>
            <span className="text-sm text-muted-foreground">{config.temperature?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.temperature || 0.7]}
            onValueChange={(v) => setConfig({ ...config, temperature: v[0] })}
            min={0}
            max={2}
            step={0.1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top K</Label>
            <span className="text-sm text-muted-foreground">{config.top_k}</span>
          </div>
          <Slider
            value={[config.top_k || 50]}
            onValueChange={(v) => setConfig({ ...config, top_k: v[0] })}
            min={1}
            max={100}
            step={1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top P</Label>
            <span className="text-sm text-muted-foreground">{config.top_p?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.top_p || 0.9]}
            onValueChange={(v) => setConfig({ ...config, top_p: v[0] })}
            min={0}
            max={1}
            step={0.05}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="seed">Seed (Optional)</Label>
          <Input
            id="seed"
            type="number"
            placeholder="Random seed"
            value={config.seed || ''}
            onChange={(e) => setConfig({ ...config, seed: parseInt(e.target.value) || undefined })}
          />
        </div>

        <div className="flex items-center space-x-2">
          <Checkbox
            id="evidence"
            checked={config.require_evidence || false}
            onCheckedChange={(checked) => setConfig({ ...config, require_evidence: !!checked })}
          />
          <Label htmlFor="evidence">Require Evidence (RAG)</Label>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );

  const renderResponse = (response: InferResponse | null, isLoading: boolean, isStreamingMode: boolean = false, streamingTokens: StreamingToken[] = []) => {
    // Handle streaming mode with throttled updates for performance
    if (isStreamingMode && isStreaming) {
      const streamingText = throttledStreamingTokens.map(t => t.token).join('');
      return (
        <div className="space-y-4">
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base flex items-center gap-2">
                  <Wifi className="h-4 w-4 text-green-500 animate-pulse" />
                  Live Streaming
                </CardTitle>
                <div className="flex gap-2">
                  {streamController && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => streamController.abort()}
                      aria-label="Stop streaming"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                  <Badge variant="outline" className="gap-1">
                    <TrendingUp className="h-3 w-3" />
                    {throttledStreamingTokens.length} tokens
                  </Badge>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div
                ref={streamingRef}
                className="relative"
              >
                <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg min-h-[100px]">
                  {streamingText}
                  <span className="animate-pulse text-primary">▊</span>
                </pre>
                <Button
                  variant="ghost"
                  size="sm"
                  className="absolute top-2 right-2"
                  onClick={() => handleCopy(streamingText)}
                  disabled={!streamingText.trim()}
                >
                  <Copy className="h-4 w-4" aria-hidden="true" />
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      );
    }

    if (isLoading) {
      return (
        <div className="flex items-center justify-center p-8">
          <div className="text-center space-y-2">
            <Zap className="h-8 w-8 animate-pulse mx-auto text-primary" />
            <p className="text-sm text-muted-foreground">Generating response...</p>
          </div>
        </div>
      );
    }

    if (!response) {
      return (
        <div className="flex items-center justify-center p-8 text-muted-foreground">
          <FileText className="h-8 w-8 mr-2" />
          <p>No response yet. Click "Generate" to run inference.</p>
        </div>
      );
    }

    return (
      <div className="space-y-4">
        {/* Response Text */}
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base flex items-center gap-2">
                {inferenceMode === 'streaming' && <CheckCircle className="h-4 w-4 text-green-500" />}
                Response
              </CardTitle>
              <div className="flex gap-2">
                <Badge variant="outline" className="gap-1">
                  <Clock className="h-3 w-3" />
                  {response.latency_ms || ('trace' in response && response.trace && 'latency_ms' in response.trace ? (response.trace as any).latency_ms : 0)}ms
                </Badge>
                <Badge variant="outline" className="gap-1">
                  <FileText className="h-3 w-3" />
                  {response.token_count || 0} tokens
                </Badge>
                {metrics && (
                  <Badge variant="outline" className="gap-1">
                    <TrendingUp className="h-3 w-3" />
                    {metrics.tokensPerSecond.toFixed(1)} t/s
                  </Badge>
                )}
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="relative">
              <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg">
                {response.text}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => handleCopy(response.text)}
              >
                <Copy className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Trace Information */}
        {response.trace && 'latency_ms' in response.trace && (
          <TraceVisualizer trace={response.trace as any} />
        )}

        {/* Enhanced Metadata */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="flex items-center gap-2">
            <CheckCircle className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Finish Reason</div>
              <div className="text-xs text-muted-foreground">{response.finish_reason || 'unknown'}</div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Target className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Router Decisions</div>
              <div className="text-xs text-muted-foreground">
                {response.trace?.router_decisions?.length || 0} steps
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Evidence Spans</div>
              <div className="text-xs text-muted-foreground">
                {response.trace?.evidence_spans?.length || 0} found
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* Error Recovery */}
      {inferenceError && ErrorRecoveryTemplates.genericError(
        inferenceError,
        () => { setInferenceError(null); setPrompt(''); }
      )}

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
              <Button
                variant={inferenceMode === 'streaming' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('streaming')}
              >
                <Wifi className="h-3 w-3 mr-1" />
                Streaming
              </Button>
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
              <Button
                variant={mode === 'comparison' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setMode('comparison')}
              >
                <Split className="h-3 w-3 mr-1" />
                Compare
              </Button>
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
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <Layers className="h-5 w-5" />
                Batch Inference
              </CardTitle>
              <p className="text-sm text-muted-foreground">
                Process multiple prompts simultaneously with shared configuration
              </p>
            </CardHeader>
            <CardContent className="space-y-4">
              {/* Batch Prompts Input */}
              <div className="space-y-2">
                <Label>Prompts (one per line)</Label>
                <Textarea
                  placeholder="Enter one prompt per line...
Write a Python function to calculate fibonacci
Explain quantum computing in simple terms
What is the capital of France?"
                  value={batchPrompts.join('\n')}
                  onChange={(e) => setBatchPrompts(e.target.value.split('\n').filter(p => p.trim()))}
                  rows={8}
                  className={batchValidation.some(v => !v.valid) ? 'border-destructive' : ''}
                />
                <p className="text-xs text-muted-foreground">
                  {batchPrompts.filter(p => p.trim()).length} prompts ready for batch processing
                </p>

                {/* Batch validation errors */}
                {batchValidation.some(v => !v.valid) && (
                  <Alert variant="destructive" className="text-sm">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Validation Errors:</strong>
                      <ul className="mt-1 space-y-1">
                        {batchValidation
                          .map((validation, index) => ({ validation, index }))
                          .filter(({ validation }) => !validation.valid)
                          .slice(0, 3) // Show first 3 errors
                          .map(({ validation, index }) => (
                            <li key={index}>
                              Prompt {index + 1}: {validation.error}
                            </li>
                          ))}
                        {batchValidation.filter(v => !v.valid).length > 3 && (
                          <li>... and {batchValidation.filter(v => !v.valid).length - 3} more</li>
                        )}
                      </ul>
                    </AlertDescription>
                  </Alert>
                )}

                {/* Batch validation warnings */}
                {batchValidation.some(v => v.warning) && (
                  <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50">
                    <AlertTriangle className="h-4 w-4 text-yellow-600" />
                    <AlertDescription className="text-yellow-800">
                      <strong>Warnings:</strong> Some prompts have warnings (long content, etc.)
                    </AlertDescription>
                  </Alert>
                )}
              </div>

              {/* Shared Configuration Preview */}
              <div className="p-3 bg-muted rounded-md">
                <h4 className="text-sm font-medium mb-2">Shared Configuration</h4>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
                  <div>Max Tokens: {configA.max_tokens}</div>
                  <div>Temperature: {configA.temperature}</div>
                  <div>Top K: {configA.top_k}</div>
                  <div>Top P: {configA.top_p?.toFixed(2)}</div>
                </div>
              </div>

              <Button
                onClick={executeBatchInference}
                disabled={batchPrompts.filter(p => p.trim()).length === 0 || isBatchRunning}
                className="w-full"
              >
                {isBatchRunning ? (
                  <>
                    <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white mr-2" />
                    Processing Batch...
                  </>
                ) : (
                  <>
                    <Layers className="h-4 w-4 mr-2" />
                    Run Batch Inference ({batchPrompts.filter(p => p.trim()).length} prompts)
                  </>
                )}
              </Button>
            </CardContent>
          </Card>

          {/* Batch Results */}
          {batchResults && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Batch Results</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-4">
                  {batchResults.responses?.map((item: any, index: number) => (
                    <Card key={item.id || index}>
                      <CardHeader className="pb-2">
                        <div className="flex items-center justify-between">
                          <CardTitle className="text-sm">Prompt {index + 1}</CardTitle>
                          {item.error ? (
                            <Badge variant="destructive">Error</Badge>
                          ) : (
                            <Badge variant="default">Success</Badge>
                          )}
                        </div>
                      </CardHeader>
                      <CardContent className="pt-0">
                        <div className="text-sm text-muted-foreground mb-2">
                          {batchPrompts[index]}
                        </div>
                        {item.response ? (
                          <div className="text-sm bg-muted p-3 rounded">
                            {item.response.text}
                          </div>
                        ) : item.error ? (
                          <div className="text-sm text-destructive bg-destructive/10 p-3 rounded">
                            {item.error}
                          </div>
                        ) : null}
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </div>
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
                      {!adaptersLoadError && (
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
                  <Label htmlFor="adapter">
                    Adapter {adapters.length === 0 && <span className="text-muted-foreground text-xs">(None - base model only)</span>}
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
                  <Label htmlFor="adapter">
                    Adapter Selection {adapters.length === 0 && <span className="text-muted-foreground text-xs">(None - base model only)</span>}
                  </Label>
                  <Select value={selectedAdapterId} onValueChange={setSelectedAdapterId} disabled={adapters.length === 0}>
                    <SelectTrigger id="adapter">
                      <SelectValue placeholder={
                        adapters.length === 0
                          ? "No adapters available"
                          : selectedAdapterId === 'none'
                            ? "🤖 Auto-select (Router chooses best adapters)"
                            : "Select adapter..."
                      } />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">
                          <div className="flex items-center gap-2">
                          <Target className="h-4 w-4 text-primary" />
                          <div>
                            <div className="font-medium">Auto-select (Recommended)</div>
                            <div className="text-xs text-muted-foreground">Router chooses optimal adapters based on your prompt</div>
                          </div>
                          </div>
                        </SelectItem>
                      {adapters.filter(adapter => adapter.id && adapter.id !== '').map((adapter) => {
                        const getStateIcon = () => {
                          switch (adapter.current_state) {
                            case 'hot': return <div className="w-2 h-2 bg-green-500 rounded-full" />;
                            case 'warm': return <div className="w-2 h-2 bg-yellow-500 rounded-full" />;
                            case 'resident': return <div className="w-2 h-2 bg-blue-500 rounded-full" />;
                            case 'cold': return <div className="w-2 h-2 bg-gray-400 rounded-full" />;
                            default: return <div className="w-2 h-2 bg-gray-300 rounded-full" />;
                          }
                        };

                        const getStateColor = () => {
                          switch (adapter.current_state) {
                            case 'hot': return 'text-green-600';
                            case 'warm': return 'text-yellow-600';
                            case 'resident': return 'text-blue-600';
                            case 'cold': return 'text-gray-600';
                            default: return 'text-gray-500';
                          }
                        };

                        return (
                          <SelectItem key={adapter.id} value={adapter.id}>
                            <div className="flex items-center gap-2 w-full">
                              {getStateIcon()}
                              <Code className="h-4 w-4 flex-shrink-0" />
                              <div className="flex-1 min-w-0">
                                <div className="font-medium truncate">{adapter.name}</div>
                                <div className={`text-xs ${getStateColor()}`}>
                                  {adapter.current_state || 'unknown'} • {adapter.languages?.join(', ') || 'general'}
                                </div>
                              </div>
                            </div>
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>
                  <div className="text-xs text-muted-foreground space-y-1">
                    <p>
                      {selectedAdapterId === 'none'
                        ? '🤖 Router will automatically select the best adapters for your prompt using K-sparse routing.'
                        : 'Using selected adapter for inference. Router decisions will still apply if multiple adapters match.'}
                    </p>
                    {adapters.length > 0 && (
                      <p>
                        💡 <strong>Pro tip:</strong> Try "Auto-select" to let the router optimize adapter selection based on your prompt content.
                      </p>
                    )}
                  </div>
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="prompt">
                      Prompt
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
                    }}
                    rows={6}
                    className={promptValidation.valid ? '' : 'border-destructive'}
                    aria-describedby={promptValidation.error ? "prompt-error" : promptValidation.warning ? "prompt-warning" : undefined}
                    aria-invalid={!promptValidation.valid}
                  />
                  {promptValidation.error && (
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
                  {promptValidation.warning && (
                    <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50" id="prompt-warning">
                      <AlertTriangle className="h-4 w-4 text-yellow-600" />
                      <AlertDescription className="text-yellow-800">
                        <strong>Warning:</strong> {promptValidation.warning}
                      </AlertDescription>
                    </Alert>
                  )}
                  {!promptValidation.valid && !promptValidation.error && (
                    <div className="text-xs text-muted-foreground">
                      Character count: {configA.prompt.length.toLocaleString()} / {MAX_PROMPT_LENGTH.toLocaleString()}
                    </div>
                  )}
                  {windowSize.width < 768 && (
                    <div className="text-xs text-muted-foreground mt-1">
                      💡 Swipe left/right to change modes, swipe up for templates
                    </div>
                  )}
                  {showTemplates && (
                    <div className="border rounded-md p-3 bg-muted/50">
                      <div className="text-sm font-medium mb-2">Prompt Templates</div>
                      <div className="space-y-2 max-h-48 overflow-y-auto">
                        {templates.map((template) => (
                          <Button
                            key={template.id}
                            variant="ghost"
                            className="w-full justify-start text-left h-auto p-2"
                            onClick={() => applyTemplate(template)}
                          >
                            <div>
                              <div className="font-medium text-sm">{template.name}</div>
                              <div className="text-xs text-muted-foreground">{template.description}</div>
                              <Badge variant="outline" className="mt-1 text-xs">
                                {template.category}
                              </Badge>
                            </div>
                          </Button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>

                {renderAdvancedOptions(configA, setConfigA)}

                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                    disabled={isLoadingA}
                    aria-label="Run inference with current configuration"
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    {isLoadingA ? 'Generating...' : 'Generate'}
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
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
                {renderResponse(responseA, isLoadingA, inferenceMode === 'streaming' && isStreaming, streamingTokens)}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        /* Comparison Mode */
        <div className="space-y-4">
          {/* Shared Prompt */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Shared Prompt</CardTitle>
            </CardHeader>
            <CardContent>
              <Textarea
                placeholder="Enter prompt to compare..."
                value={prompt}
                onChange={(e) => {
                  setPrompt(e.target.value);
                  setConfigA({ ...configA, prompt: e.target.value });
                  setConfigB({ ...configB, prompt: e.target.value });
                }}
                rows={4}
              />
            </CardContent>
          </Card>

          {/* Side-by-Side Configurations */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Config A */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration A</CardTitle>
                  <Badge>Temperature: {configA.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configA, setConfigA)}
                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                    disabled={isLoadingA || !prompt.trim()}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    Generate A
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
                      aria-label="Cancel inference A"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>
                {renderResponse(responseA, isLoadingA)}
              </CardContent>
            </Card>

            {/* Config B */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration B</CardTitle>
                  <Badge>Temperature: {configB.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configB, setConfigB)}
                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configB, setResponseB, setIsLoadingB)}
                    disabled={isLoadingB || !prompt.trim()}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    Generate B
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
                      aria-label="Cancel inference B"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>
                {renderResponse(responseB, isLoadingB)}
              </CardContent>
            </Card>
          </div>

          {/* Comparison Summary */}
          {responseA && responseB && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <BarChart3 className="h-4 w-4" aria-hidden="true" />
                  Comparison Summary
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div>
                    <p className="text-sm font-medium">Latency</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.latency_ms || 0}ms</Badge>
                      <Badge variant="outline">B: {responseB.latency_ms || 0}ms</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Tokens</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.token_count || 0}</Badge>
                      <Badge variant="outline">B: {responseB.token_count || 0}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Finish Reason</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">{responseA.finish_reason || 'unknown'}</Badge>
                      <Badge variant="outline">{responseB.finish_reason || 'unknown'}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Winner</p>
                    <Badge className="mt-1">
                      {(responseA.latency_ms || 0) < (responseB.latency_ms || 0) ? 'A (Faster)' : 'B (Faster)'}
                    </Badge>
                  </div>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
