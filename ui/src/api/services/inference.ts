/**
 * Inference service - handles inference operations with proper type transformations.
 *
 * This service transforms between backend (snake_case) and frontend (camelCase) representations.
 * It uses domain types from domain-types.ts and runtime transformers for proper type safety.
 */

import type { ApiClient } from '@/api/client';
import { logger } from '@/utils/logger';
import { toCamelCase, toSnakeCase } from '@/api/transformers';

// Type extension for ApiClient streaming method (implementation pending)
 
interface ApiClientWithStreaming extends ApiClient {
  streamInfer(
    request: any,
    callbacks: {
      onToken: (token: string, chunk: any) => void;
      onComplete: (text: string, finishReason: string | null, metadata?: any) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void>;
}
import type {
  InferRequest,
  InferResponse,
  BatchInferRequest,
  BatchInferResponse,
  RunReceipt,
  StreamingInferRequest,
  StreamingChunk,
  Citation,
} from '@/api/domain-types';
import {
  InferRequestSchema,
  InferResponseSchema,
  BatchInferRequestSchema,
  BatchInferResponseSchema,
  RunReceiptSchema,
} from '@/api/schemas/inference.zod';

// ============================================================================
// Inference Service Implementation
// ============================================================================

export class InferenceService {
  constructor(private client: ApiClient) {}

  /**
   * Perform inference with automatic type transformation
   *
   * POST /v1/infer
   *
   * @param data - Inference request (frontend camelCase format)
   * @param options - Optional request options
   * @param skipRetry - Whether to skip retry logic
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Inference response (frontend camelCase format)
   */
  async infer(
    data: InferRequest,
    options: RequestInit = {},
    skipRetry: boolean = false,
    cancelToken?: AbortSignal
  ): Promise<InferResponse> {
    logger.info('Inference requested', {
      component: 'InferenceService',
      operation: 'infer',
      prompt_length: data.prompt.length,
      model: data.model,
      backend: data.backend,
    });

    // Transform request to snake_case for backend
    const backendRequest = toSnakeCase(data);

    // Validate request with Zod schema
    const validatedRequest = InferRequestSchema.parse(backendRequest);

    // Make API request
    const backendResponse = await this.client.request<unknown>(
      '/v1/infer',
      {
        method: 'POST',
        body: JSON.stringify(validatedRequest),
        ...options,
      },
      skipRetry,
      cancelToken
    );

    // Validate response with Zod schema (snake_case from backend)
    const validatedResponse = InferResponseSchema.parse(backendResponse);

    // Transform response to camelCase for frontend
    // Use 'unknown' as intermediate to handle passthrough fields from Zod schema
    const frontendResponse = toCamelCase(validatedResponse) as unknown as InferResponse;

    // Ensure critical fields are properly transformed
    if (frontendResponse.runReceipt) {
      frontendResponse.runReceipt = this.transformRunReceipt(frontendResponse.runReceipt);
    }

    logger.info('Inference completed', {
      component: 'InferenceService',
      operation: 'infer',
      id: frontendResponse.id,
      tokensGenerated: frontendResponse.tokensGenerated,
      latencyMs: frontendResponse.latencyMs,
      adaptersUsed: frontendResponse.adaptersUsed,
    });

    return frontendResponse;
  }

  /**
   * Perform batch inference with automatic type transformation
   *
   * POST /v1/infer/batch
   *
   * @param data - Batch inference request (frontend camelCase format)
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Batch inference response (frontend camelCase format)
   */
  async batchInfer(
    data: BatchInferRequest,
    cancelToken?: AbortSignal
  ): Promise<BatchInferResponse> {
    // Calculate batch size from camelCase request
    const batchSize = data.requests?.length ?? 0;

    logger.info('Batch inference requested', {
      component: 'InferenceService',
      operation: 'batchInfer',
      batchSize,
    });

    // Transform request to snake_case for backend
    const backendRequest = toSnakeCase(data);

    // Validate request with Zod schema (snake_case)
    const validatedRequest = BatchInferRequestSchema.parse(backendRequest);

    // Make API request
    const backendResponse = await this.client.request<unknown>(
      '/v1/infer/batch',
      {
        method: 'POST',
        body: JSON.stringify(validatedRequest),
      },
      false,
      cancelToken
    );

    // Validate response with Zod schema (snake_case from backend)
    const validatedResponse = BatchInferResponseSchema.parse(backendResponse);

    // Transform response to camelCase for frontend
    // Use 'unknown' as intermediate to handle passthrough fields from Zod schema
    const frontendResponse = toCamelCase(validatedResponse) as unknown as BatchInferResponse;

    // Transform all responses (the domain type uses 'responses', not 'results')
    // Each response item has a 'response' field containing the InferResponse
    if (frontendResponse.responses) {
      frontendResponse.responses = frontendResponse.responses.map((item) => {
        if (item.response?.runReceipt) {
          item.response.runReceipt = this.transformRunReceipt(item.response.runReceipt);
        }
        return item;
      });
    }

    logger.info('Batch inference completed', {
      component: 'InferenceService',
      operation: 'batchInfer',
      batchSize: frontendResponse.responses.length,
    });

    return frontendResponse;
  }

  /**
   * Stream inference using the /v1/infer/stream endpoint with SSE
   *
   * POST /v1/infer/stream
   *
   * Note: This method delegates to ApiClient.streamInfer() because streaming
   * requires direct access to private client internals (baseUrl, token, etc).
   *
   * @param data - The streaming inference request payload (frontend camelCase format)
   * @param callbacks - Event callbacks for streaming tokens
   * @param cancelToken - Optional abort signal for cancellation
   * @returns Promise that resolves when stream completes
   */
  async streamInfer(
    data: StreamingInferRequest,
    callbacks: {
      onToken: (token: string, chunk: StreamingChunk) => void;
      onComplete: (
        fullText: string,
        finishReason: string | null,
        metadata?: {
          requestId?: string;
          unavailablePinnedAdapters?: string[];
          pinnedRoutingFallback?: 'stack_only' | 'partial' | null;
          citations?: Citation[];
        }
      ) => void;
      onError: (error: Error) => void;
    },
    cancelToken?: AbortSignal
  ): Promise<void> {
    logger.info('Stream inference requested', {
      component: 'InferenceService',
      operation: 'streamInfer',
      prompt_length: data.prompt.length,
    });

    // Transform request to snake_case for backend
    const backendRequest = toSnakeCase(data);

    // Delegate to ApiClient which has access to private members needed for streaming
    // TODO: Implement ApiClient.streamInfer method or move implementation here
    // Type assertion needed because streamInfer method is not yet implemented on ApiClient
    return (this.client as ApiClientWithStreaming).streamInfer(backendRequest, callbacks, cancelToken);
  }

  /**
   * Transform RunReceipt to ensure all critical fields are properly transformed.
   * This is critical for determinism - receipt fields must be correctly named.
   *
   * @param receipt - Already camelCase receipt from toCamelCase transformation
   * @returns Normalized receipt with all fields properly typed
   */
  private transformRunReceipt(receipt: RunReceipt): RunReceipt {
    // Receipt is already in camelCase from toCamelCase transformation
    // This method ensures all critical fields are present and properly typed
    // No additional transformation needed - just return normalized structure
    return {
      traceId: receipt.traceId,
      runHeadHash: receipt.runHeadHash,
      outputDigest: receipt.outputDigest,
      receiptDigest: receipt.receiptDigest,
      signature: receipt.signature,
      attestation: receipt.attestation,

      // Token accounting (required)
      logicalPromptTokens: receipt.logicalPromptTokens,
      prefixCachedTokenCount: receipt.prefixCachedTokenCount,
      billedInputTokens: receipt.billedInputTokens,
      logicalOutputTokens: receipt.logicalOutputTokens,
      billedOutputTokens: receipt.billedOutputTokens,

      // Optional fields
      stopReasonCode: receipt.stopReasonCode,
      stopReasonTokenIndex: receipt.stopReasonTokenIndex,
      stopPolicyDigestB3: receipt.stopPolicyDigestB3,
      tenantKvQuotaBytes: receipt.tenantKvQuotaBytes,
      tenantKvBytesUsed: receipt.tenantKvBytesUsed,
      kvEvictions: receipt.kvEvictions,
      kvResidencyPolicyId: receipt.kvResidencyPolicyId,
      kvQuotaEnforced: receipt.kvQuotaEnforced,
      prefixKvKeyB3: receipt.prefixKvKeyB3,
      prefixCacheHit: receipt.prefixCacheHit,
      prefixKvBytes: receipt.prefixKvBytes,
      modelCacheIdentityV2DigestB3: receipt.modelCacheIdentityV2DigestB3,
    };
  }
}
