/**
 * Example usage patterns for Zod API validation schemas.
 * These examples show how to use the validation schemas in real-world scenarios.
 */

import {
  // Adapter schemas
  AdapterSchema,
  AdapterSummarySchema,
  AdapterResponseSchema,
  ListAdaptersResponseSchema,
  AdapterHealthResponseSchema,
  PublishAdapterRequestSchema,
  PublishAdapterResponseSchema,

  // Stack schemas
  AdapterStackSchema,
  AdapterStackResponseSchema,
  ListAdapterStacksResponseSchema,
  PolicyPreflightResponseSchema,

  // Inference schemas
  InferRequestSchema,
  InferResponseSchema,
  RunReceiptSchema,
  BatchInferRequestSchema,
  BatchInferResponseSchema,

  // Validation helpers
  safeParseApiResponse,
  parseApiResponse,
  safeParseApiArray,
} from './index';

// ============================================================================
// Example 1: Validating Adapter API Responses
// ============================================================================

async function fetchAdapter(adapterId: string) {
  const response = await fetch(`/api/adapters/${adapterId}`);
  const data = await response.json();

  // Safe parsing - returns null on error, logs to console
  const adapter = safeParseApiResponse(
    AdapterResponseSchema,
    data,
    `GET /api/adapters/${adapterId}`
  );

  if (!adapter) {
    console.error('Failed to parse adapter response');
    return null;
  }

  // TypeScript knows adapter.adapter is validated
  return adapter.adapter;
}

async function fetchAdaptersList() {
  const response = await fetch('/api/adapters');
  const data = await response.json();

  // Validate the full list response
  const listResponse = safeParseApiResponse(
    ListAdaptersResponseSchema,
    data,
    'GET /api/adapters'
  );

  if (!listResponse) {
    return [];
  }

  // All adapters are validated
  return listResponse.adapters;
}

// ============================================================================
// Example 2: Validating Adapter Health
// ============================================================================

async function checkAdapterHealth(adapterId: string) {
  const response = await fetch(`/api/adapters/${adapterId}/health`);
  const data = await response.json();

  const health = safeParseApiResponse(
    AdapterHealthResponseSchema,
    data,
    `GET /api/adapters/${adapterId}/health`
  );

  if (!health) {
    return { isHealthy: false, issues: [] };
  }

  return {
    isHealthy: health.health === 'healthy',
    issues: health.subcodes.map(s => s.message || s.code),
    driftMetric: health.drift_summary?.current,
  };
}

// ============================================================================
// Example 3: Publishing an Adapter with Validation
// ============================================================================

async function publishAdapter(
  adapterId: string,
  publishData: unknown
) {
  // Validate request before sending
  const validRequest = parseApiResponse(
    PublishAdapterRequestSchema,
    publishData,
    'Publish adapter request'
  );

  const response = await fetch(`/api/adapters/${adapterId}/publish`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(validRequest),
  });

  const data = await response.json();

  // Validate response
  const publishResponse = safeParseApiResponse(
    PublishAdapterResponseSchema,
    data,
    `POST /api/adapters/${adapterId}/publish`
  );

  if (!publishResponse) {
    throw new Error('Failed to validate publish response');
  }

  return publishResponse;
}

// ============================================================================
// Example 4: Stack Management with Policy Preflight
// ============================================================================

async function createStackWithPreflight(stackData: {
  name: string;
  adapters: Array<{ adapter_id: string; gate: number }>;
  description?: string;
}) {
  // First, run preflight check
  const preflightResponse = await fetch('/api/stacks/preflight', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(stackData),
  });

  const preflightData = await preflightResponse.json();
  const preflight = safeParseApiResponse(
    PolicyPreflightResponseSchema,
    preflightData,
    'POST /api/stacks/preflight'
  );

  if (!preflight?.can_proceed) {
    const errors = preflight?.checks
      .filter(c => !c.passed && c.severity === 'error')
      .map(c => c.message) || [];
    throw new Error(`Stack creation blocked: ${errors.join(', ')}`);
  }

  // Preflight passed, create the stack
  const createResponse = await fetch('/api/stacks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(stackData),
  });

  const createData = await createResponse.json();
  const stackResponse = safeParseApiResponse(
    AdapterStackResponseSchema,
    createData,
    'POST /api/stacks'
  );

  if (!stackResponse) {
    throw new Error('Failed to create stack');
  }

  return stackResponse.stack;
}

// ============================================================================
// Example 5: Inference with Request/Response Validation
// ============================================================================

async function runInference(requestData: {
  prompt: string;
  model?: string;
  max_tokens?: number;
  temperature?: number;
  stack_id?: string;
}) {
  // Validate request before sending
  const validRequest = parseApiResponse(
    InferRequestSchema,
    requestData,
    'Inference request'
  );

  const response = await fetch('/api/infer', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(validRequest),
  });

  const data = await response.json();

  // Validate response
  const inferResponse = safeParseApiResponse(
    InferResponseSchema,
    data,
    'POST /api/infer'
  );

  if (!inferResponse) {
    throw new Error('Invalid inference response');
  }

  return {
    text: inferResponse.text,
    tokensGenerated: inferResponse.tokens_generated,
    latencyMs: inferResponse.latency_ms,
    adaptersUsed: inferResponse.adapters_used,
    receipt: inferResponse.run_receipt,
  };
}

// ============================================================================
// Example 6: Batch Inference
// ============================================================================

async function runBatchInference(prompts: string[]) {
  const requestData = {
    prompts,
    max_tokens: 100,
    temperature: 0.7,
  };

  // Validate request
  const validRequest = parseApiResponse(
    BatchInferRequestSchema,
    requestData,
    'Batch inference request'
  );

  const response = await fetch('/api/batch-infer', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(validRequest),
  });

  const data = await response.json();

  // Validate response
  const batchResponse = safeParseApiResponse(
    BatchInferResponseSchema,
    data,
    'POST /api/batch-infer'
  );

  if (!batchResponse) {
    throw new Error('Invalid batch inference response');
  }

  return {
    results: batchResponse.results,
    totalTokens: batchResponse.total_tokens,
    totalLatency: batchResponse.total_latency_ms,
  };
}

// ============================================================================
// Example 7: Validating Partial Data with Custom Schemas
// ============================================================================

async function fetchAdapterSummaries() {
  const response = await fetch('/api/adapters?summary=true');
  const data = await response.json();

  // Use the summary schema for lightweight data
  const summaries = safeParseApiArray(
    AdapterSummarySchema,
    data.adapters,
    'GET /api/adapters?summary=true'
  );

  return summaries.map(s => ({
    id: s.adapter_id,
    name: s.name,
    state: s.current_state,
    memoryMb: s.memory_bytes ? s.memory_bytes / (1024 * 1024) : 0,
  }));
}

// ============================================================================
// Example 8: Error Handling Patterns
// ============================================================================

async function fetchAdapterWithErrorHandling(adapterId: string) {
  try {
    const response = await fetch(`/api/adapters/${adapterId}`);

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    const data = await response.json();

    // Try to parse with strict validation
    const adapter = parseApiResponse(
      AdapterResponseSchema,
      data,
      `GET /api/adapters/${adapterId}`
    );

    return { success: true, data: adapter.adapter };
  } catch (error) {
    if (error instanceof Error) {
      console.error('Failed to fetch adapter:', error.message);
      return { success: false, error: error.message };
    }
    return { success: false, error: 'Unknown error' };
  }
}

// ============================================================================
// Example 9: React Hook with Validation
// ============================================================================

import { useQuery } from '@tanstack/react-query';

function useAdapter(adapterId: string) {
  return useQuery({
    queryKey: ['adapter', adapterId],
    queryFn: async () => {
      const response = await fetch(`/api/adapters/${adapterId}`);
      const data = await response.json();

      // Validate in the query function
      const validated = safeParseApiResponse(
        AdapterResponseSchema,
        data,
        `useAdapter(${adapterId})`
      );

      if (!validated) {
        throw new Error('Invalid adapter response');
      }

      return validated.adapter;
    },
  });
}

function useAdaptersList() {
  return useQuery({
    queryKey: ['adapters'],
    queryFn: async () => {
      const response = await fetch('/api/adapters');
      const data = await response.json();

      const validated = safeParseApiResponse(
        ListAdaptersResponseSchema,
        data,
        'useAdaptersList'
      );

      if (!validated) {
        throw new Error('Invalid adapters list response');
      }

      return validated.adapters;
    },
  });
}

// ============================================================================
// Example 10: Type-Safe API Client
// ============================================================================

class AdapterApiClient {
  private baseUrl = '/api';

  async getAdapter(adapterId: string) {
    const response = await fetch(`${this.baseUrl}/adapters/${adapterId}`);
    const data = await response.json();

    return parseApiResponse(
      AdapterResponseSchema,
      data,
      'AdapterApiClient.getAdapter'
    );
  }

  async listAdapters() {
    const response = await fetch(`${this.baseUrl}/adapters`);
    const data = await response.json();

    return parseApiResponse(
      ListAdaptersResponseSchema,
      data,
      'AdapterApiClient.listAdapters'
    );
  }

  async publishAdapter(
    adapterId: string,
    request: { attach_mode: 'free' | 'requires_dataset'; short_description?: string }
  ) {
    // Validate request
    const validRequest = parseApiResponse(
      PublishAdapterRequestSchema,
      request,
      'AdapterApiClient.publishAdapter'
    );

    const response = await fetch(`${this.baseUrl}/adapters/${adapterId}/publish`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(validRequest),
    });

    const data = await response.json();

    return parseApiResponse(
      PublishAdapterResponseSchema,
      data,
      'AdapterApiClient.publishAdapter'
    );
  }

  async runInference(request: {
    prompt: string;
    model?: string;
    max_tokens?: number;
  }) {
    // Validate request
    const validRequest = parseApiResponse(
      InferRequestSchema,
      request,
      'AdapterApiClient.runInference'
    );

    const response = await fetch(`${this.baseUrl}/infer`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(validRequest),
    });

    const data = await response.json();

    return parseApiResponse(
      InferResponseSchema,
      data,
      'AdapterApiClient.runInference'
    );
  }
}

export const adapterApi = new AdapterApiClient();
