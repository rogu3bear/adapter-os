/* eslint-disable no-console */
/**
 * Example usage of the Inference Service
 *
 * This file demonstrates how to use the new inference service with proper type safety
 * and automatic transformation between snake_case (backend) and camelCase (frontend).
 */

import { inference } from '@/api/services';
import type { InferRequest, InferResponse, BatchInferRequest } from '@/api/domain-types';

// ============================================================================
// Basic Inference Example
// ============================================================================

async function basicInferenceExample() {
  // Request with camelCase fields (frontend format)
  const request: InferRequest = {
    prompt: 'Write a hello world function in Python',
    maxTokens: 100,
    temperature: 0.7,
    backend: 'coreml',
    adapterStack: ['python-expert', 'code-style'],
  };

  try {
    // Service automatically transforms to snake_case for backend,
    // then transforms response back to camelCase
    const response: InferResponse = await inference.infer(request);

    // Access response fields in camelCase
    console.log('Generated text:', response.text);
    console.log('Tokens generated:', response.tokensGenerated); // Note: camelCase!
    console.log('Latency:', response.latencyMs); // Note: camelCase!
    console.log('Adapters used:', response.adaptersUsed); // Note: camelCase!

    // RunReceipt is also properly transformed
    if (response.runReceipt) {
      console.log('Trace ID:', response.runReceipt.traceId); // Note: camelCase!
      console.log('Run head hash:', response.runReceipt.runHeadHash); // Note: camelCase!
      console.log('Billed input tokens:', response.runReceipt.billedInputTokens); // Note: camelCase!
    }

    return response;
  } catch (error) {
    console.error('Inference failed:', error);
    throw error;
  }
}

// ============================================================================
// Batch Inference Example
// ============================================================================

async function batchInferenceExample() {
  // Batch request with multiple prompts as individual requests
  const batchRequest: BatchInferRequest = {
    requests: [
      {
        id: 'req-1',
        prompt: 'Explain recursion in simple terms',
        maxTokens: 150,
        temperature: 0.5,
      },
      {
        id: 'req-2',
        prompt: 'What is a closure in JavaScript?',
        maxTokens: 150,
        temperature: 0.5,
      },
      {
        id: 'req-3',
        prompt: 'How does async/await work?',
        maxTokens: 150,
        temperature: 0.5,
      },
    ],
  };

  try {
    const response = await inference.batchInfer(batchRequest);

    // All responses have properly transformed fields
    response.responses.forEach((item) => {
      console.log(`Response ${item.id}:`);
      if (item.response) {
        console.log('  Text:', item.response.text);
        console.log('  Tokens:', item.response.tokensGenerated); // camelCase
        console.log('  Adapters:', item.response.adaptersUsed); // camelCase
      } else if (item.error) {
        console.log('  Error:', item.error.error);
      }
    });

    return response;
  } catch (error) {
    console.error('Batch inference failed:', error);
    throw error;
  }
}

// ============================================================================
// Streaming Inference Example
// ============================================================================

async function streamInferenceExample() {
  const request: InferRequest = {
    prompt: 'Write a short story about a robot learning to code',
    maxTokens: 500,
    temperature: 0.8,
    stream: true, // Enable streaming
  };

  try {
    let fullText = '';

    await inference.streamInfer(request as any, {
      // Called for each token as it arrives
      onToken: (token, chunk) => {
        fullText += token;
        console.log('Received token:', token);
        // chunk has properly typed fields
        console.log('Chunk:', chunk);
      },

      // Called when streaming completes
      onComplete: (text, finishReason, metadata) => {
        console.log('Stream complete!');
        console.log('Full text:', text);
        console.log('Finish reason:', finishReason);
        console.log('Metadata:', metadata);

        // Metadata fields are properly typed
        if (metadata?.citations) {
          metadata.citations.forEach(citation => {
            console.log('Citation:', citation.filePath); // camelCase
          });
        }
      },

      // Called if streaming encounters an error
      onError: (error) => {
        console.error('Streaming error:', error);
      },
    });

    return fullText;
  } catch (error) {
    console.error('Stream inference failed:', error);
    throw error;
  }
}

// ============================================================================
// Advanced Example: With Evidence and RAG
// ============================================================================

async function inferenceWithEvidenceExample() {
  const request: InferRequest = {
    prompt: 'What are the key features of our product?',
    maxTokens: 200,
    requireEvidence: true, // Request evidence
    ragEnabled: true, // Enable RAG
    collectionId: 'product-docs',
    adapterStack: ['product-expert'],
  };

  try {
    const response = await inference.infer(request);

    console.log('Response:', response.text);

    // Citations are included when evidence is requested
    if (response.citations) {
      console.log('Evidence found in', response.citations.length, 'sources:');
      response.citations.forEach(citation => {
        // All citation fields are in camelCase
        console.log('- File:', citation.filePath);
        console.log('  Chunk:', citation.chunkId);
        console.log('  Relevance:', citation.relevanceScore);
        console.log('  Preview:', citation.preview);
      });
    }

    return response;
  } catch (error) {
    console.error('Inference with evidence failed:', error);
    throw error;
  }
}

// ============================================================================
// Export examples for reference
// ============================================================================

export {
  basicInferenceExample,
  batchInferenceExample,
  streamInferenceExample,
  inferenceWithEvidenceExample,
};
