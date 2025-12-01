/**
 * Batch Inference Hook
 *
 * Provides batch inference state management and execution logic for processing
 * multiple prompts in a single request. Extracted from InferencePlayground.tsx
 * for better separation of concerns and reusability.
 *
 * @example
 * ```tsx
 * const {
 *   batchPrompts,
 *   setBatchPrompts,
 *   addPrompt,
 *   batchResults,
 *   isBatchRunning,
 *   executeBatch,
 *   exportResultsCSV,
 * } = useBatchInference({
 *   config: {
 *     max_tokens: 100,
 *     temperature: 0.7,
 *     top_k: 50,
 *     top_p: 0.9,
 *   },
 *   adapterId: 'my-adapter-id',
 *   onSuccess: (results) => {
 *     console.log('Batch complete:', results);
 *   },
 * });
 *
 * // Add prompts
 * addPrompt('What is the capital of France?');
 * addPrompt('Explain quantum computing');
 *
 * // Execute batch
 * await executeBatch();
 *
 * // Export results
 * exportResultsCSV();
 * ```
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import { useState, useCallback } from 'react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { validatePrompt, ValidationResult } from '@/components/inference/PromptInput';
import { BatchPromptSchema } from '@/schemas';
import { logger, toError } from '@/utils/logger';
import type { InferResponse, InferenceConfig } from '@/api/types';

/**
 * Options for the batch inference hook
 */
export interface UseBatchInferenceOptions {
  /**
   * Base inference configuration (temperature, max_tokens, etc.)
   */
  config: InferenceConfig;

  /**
   * Optional adapter ID to use for all batch requests
   */
  adapterId?: string;

  /**
   * Optional stack ID to use for all batch requests
   */
  stackId?: string;

  /**
   * Callback invoked when batch inference completes successfully
   */
  onSuccess?: (results: BatchInferenceResult[]) => void;

  /**
   * Callback invoked when batch inference fails
   */
  onError?: (error: Error) => void;
}

/**
 * Result of a single batch inference item
 */
export interface BatchInferenceResult {
  /**
   * Request ID
   */
  id: string;

  /**
   * Original prompt
   */
  prompt: string;

  /**
   * Inference response (null if error occurred)
   */
  response: InferResponse | null;

  /**
   * Error message if inference failed
   */
  error?: string;

  /**
   * Duration of the inference in milliseconds
   */
  duration: number;
}

/**
 * Batch processing metrics
 */
export interface BatchMetrics {
  /**
   * Total number of prompts processed
   */
  total: number;

  /**
   * Number of successful inferences
   */
  success: number;

  /**
   * Number of failed inferences
   */
  errors: number;

  /**
   * Total tokens generated across all successful inferences
   */
  totalTokens: number;

  /**
   * Total latency in milliseconds
   */
  totalLatency: number;

  /**
   * Average tokens per second across all inferences
   */
  avgTokensPerSecond: number;
}

/**
 * Return type for the useBatchInference hook
 */
export interface UseBatchInferenceReturn {
  /**
   * List of prompts to process
   */
  batchPrompts: string[];

  /**
   * Replace all batch prompts
   */
  setBatchPrompts: (prompts: string[]) => void;

  /**
   * Add a single prompt to the batch
   */
  addPrompt: (prompt: string) => void;

  /**
   * Remove a prompt at the specified index
   */
  removePrompt: (index: number) => void;

  /**
   * Validation results for each prompt
   */
  batchValidation: ValidationResult[];

  /**
   * Results from the batch inference
   */
  batchResults: BatchInferenceResult[];

  /**
   * Whether a batch inference is currently running
   */
  isBatchRunning: boolean;

  /**
   * Aggregated metrics from the last batch run
   */
  metrics: BatchMetrics | null;

  /**
   * Execute batch inference for all prompts
   */
  executeBatch: () => Promise<void>;

  /**
   * Cancel the current batch inference (if running)
   */
  cancelBatch: () => void;

  /**
   * Clear all batch results
   */
  clearResults: () => void;

  /**
   * Export results as CSV file
   */
  exportResultsCSV: () => void;

  /**
   * Export results as JSON file
   */
  exportResultsJSON: () => void;
}

/**
 * Security: Input sanitization to prevent XSS and other injection attacks
 */
const sanitizeInput = (input: string): string => {
  if (!input) return input;

  const sanitized = input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '')
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '')
    .replace(/javascript:/gi, '')
    .replace(/on\w+\s*=/gi, '')
    .replace(/<[^>]*>/g, '')
    .trim();

  if (sanitized !== input) {
    logger.warn('Input sanitized for security', {
      component: 'useBatchInference',
      operation: 'sanitizeInput',
      originalLength: input.length,
      sanitizedLength: sanitized.length,
    });
  }

  return sanitized;
};

/**
 * Batch Inference Hook
 *
 * Manages state and execution logic for batch inference operations.
 * Handles validation, API calls, error handling, and result export.
 */
export function useBatchInference(options: UseBatchInferenceOptions): UseBatchInferenceReturn {
  const { config, adapterId, stackId, onSuccess, onError } = options;

  const [batchPrompts, setBatchPrompts] = useState<string[]>([]);
  const [batchValidation, setBatchValidation] = useState<ValidationResult[]>([]);
  const [batchResults, setBatchResults] = useState<BatchInferenceResult[]>([]);
  const [isBatchRunning, setIsBatchRunning] = useState(false);
  const [metrics, setMetrics] = useState<BatchMetrics | null>(null);

  /**
   * Add a single prompt to the batch
   */
  const addPrompt = useCallback((prompt: string) => {
    setBatchPrompts(prev => [...prev, prompt]);
  }, []);

  /**
   * Remove a prompt at the specified index
   */
  const removePrompt = useCallback((index: number) => {
    setBatchPrompts(prev => prev.filter((_, i) => i !== index));
    setBatchValidation(prev => prev.filter((_, i) => i !== index));
  }, []);

  /**
   * Clear all batch results
   */
  const clearResults = useCallback(() => {
    setBatchResults([]);
    setMetrics(null);
  }, []);

  /**
   * Cancel the current batch inference
   */
  const cancelBatch = useCallback(() => {
    // Note: Currently we don't support cancellation of batch inference
    // This is a placeholder for future implementation
    logger.info('Batch cancellation requested', {
      component: 'useBatchInference',
      operation: 'cancelBatch',
    });
    toast.info('Batch cancellation is not yet supported');
  }, []);

  /**
   * Execute batch inference for all prompts
   */
  const executeBatch = useCallback(async () => {
    if (batchPrompts.length === 0) {
      toast.error('No prompts to process');
      return;
    }

    // Validate all prompts first using both custom validation and schema
    const validations = await Promise.all(
      batchPrompts.map(async (p) => {
        const customValidation = validatePrompt(p);
        if (!customValidation.valid) {
          return customValidation;
        }

        // Also validate against schema
        try {
          await BatchPromptSchema.parseAsync({
            prompt: p,
            max_tokens: config.max_tokens,
            temperature: config.temperature,
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
      })
    );

    setBatchValidation(validations);

    if (validations.some(v => !v.valid)) {
      toast.error('Some prompts have validation errors. Please fix them before proceeding.');
      return;
    }

    setIsBatchRunning(true);
    setBatchResults([]);

    logger.info('Executing batch inference', {
      component: 'useBatchInference',
      operation: 'executeBatch',
      count: batchPrompts.length,
    });

    try {
      // Create batch request items
      const batchItems = batchPrompts.map((prompt, idx) => ({
        id: `batch-${Date.now()}-${idx}`,
        prompt: sanitizeInput(prompt),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_k: config.top_k,
        top_p: config.top_p,
        backend: config.backend || 'auto',
        seed: config.seed,
        require_evidence: config.require_evidence,
        adapters: adapterId && adapterId !== 'none' ? [adapterId] : undefined,
        adapter_stack: stackId ? [stackId] : undefined,
      }));

      // Call batch inference API
      const response = await apiClient.batchInfer({
        backend: config.backend || 'auto',
        requests: batchItems,
      });

      // Transform API response to BatchInferenceResult format
      const results: BatchInferenceResult[] = response.responses.map((apiResult, idx) => ({
        id: apiResult.id || `batch-${Date.now()}-${idx}`,
        prompt: batchPrompts[idx],
        response: apiResult.error ? null : apiResult,
        error: apiResult.error,
        duration: apiResult.latency_ms || 0,
      }));

      setBatchResults(results);

      const successCount = results.filter(r => r.response).length;
      const errorCount = results.filter(r => r.error).length;

      // Calculate metrics
      const totalTokens = results.reduce(
        (sum, r) => sum + (r.response?.tokens_generated || 0),
        0
      );
      const totalLatency = results.reduce((sum, r) => sum + r.duration, 0);
      const avgTokensPerSecond = totalLatency > 0 ? (totalTokens / (totalLatency / 1000)) : 0;

      const batchMetrics: BatchMetrics = {
        total: batchPrompts.length,
        success: successCount,
        errors: errorCount,
        totalTokens,
        totalLatency,
        avgTokensPerSecond,
      };

      setMetrics(batchMetrics);

      toast.success(`Batch complete: ${successCount} succeeded, ${errorCount} failed`);

      logger.info('Batch inference completed', {
        component: 'useBatchInference',
        operation: 'executeBatch',
        total: batchPrompts.length,
        success: successCount,
        errors: errorCount,
      });

      if (onSuccess) {
        onSuccess(results);
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Batch inference failed');
      toast.error(`Batch inference failed: ${error.message}`);
      logger.error('Batch inference failed', {
        component: 'useBatchInference',
        operation: 'executeBatch',
      }, toError(err));

      if (onError) {
        onError(error);
      }
    } finally {
      setIsBatchRunning(false);
    }
  }, [batchPrompts, config, adapterId, stackId, onSuccess, onError]);

  /**
   * Export results as JSON file
   */
  const exportResultsJSON = useCallback(() => {
    if (batchResults.length === 0) {
      toast.error('No results to export');
      return;
    }

    const data = {
      batchSize: batchResults.length,
      timestamp: new Date().toISOString(),
      config: {
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_k: config.top_k,
        top_p: config.top_p,
        seed: config.seed,
        require_evidence: config.require_evidence,
        adapter: adapterId !== 'none' ? adapterId : null,
        stack: stackId || null,
      },
      metrics,
      results: batchResults.map((result, idx) => ({
        id: result.id,
        prompt: result.prompt,
        response: result.response?.text,
        token_count: result.response?.token_count || result.response?.tokens_generated,
        latency_ms: result.response?.latency_ms,
        finish_reason: result.response?.finish_reason,
        error: result.error,
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
      component: 'useBatchInference',
      operation: 'exportJSON',
      resultCount: batchResults.length,
    });
  }, [batchResults, config, adapterId, stackId, metrics]);

  /**
   * Export results as CSV file
   */
  const exportResultsCSV = useCallback(() => {
    if (batchResults.length === 0) {
      toast.error('No results to export');
      return;
    }

    // CSV header
    const headers = ['ID', 'Prompt', 'Status', 'Response', 'Token Count', 'Latency (ms)', 'Finish Reason', 'Error'];

    // CSV rows
    const rows = batchResults.map((result, idx) => {
      const prompt = (result.prompt || '').replace(/"/g, '""'); // Escape quotes
      const response = (result.response?.text || '').replace(/"/g, '""');
      const error = (result.error || '').replace(/"/g, '""');
      const status = result.error ? 'Error' : result.response ? 'Success' : 'Pending';

      return [
        result.id,
        `"${prompt}"`,
        status,
        `"${response}"`,
        result.response?.token_count || result.response?.tokens_generated || '',
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
      component: 'useBatchInference',
      operation: 'exportCSV',
      resultCount: batchResults.length,
    });
  }, [batchResults]);

  return {
    batchPrompts,
    setBatchPrompts,
    addPrompt,
    removePrompt,
    batchValidation,
    batchResults,
    isBatchRunning,
    metrics,
    executeBatch,
    cancelBatch,
    clearResults,
    exportResultsCSV,
    exportResultsJSON,
  };
}
