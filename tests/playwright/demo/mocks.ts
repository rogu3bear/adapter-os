/**
 * Centralized mock factory for demo operations.
 *
 * Wraps existing helpers from ui/helpers/sse.ts and extracts inline mocks
 * from demo.video.spec.ts into reusable preset installers.
 */

import type { Page } from '@playwright/test';
import type { MockPreset } from './types';
import { stubSystemStatus, stubInferStream, buildStream } from '../ui/helpers/sse';
import { seeded } from '../ui/utils';

// ---------------------------------------------------------------------------
// Individual mock installers
// ---------------------------------------------------------------------------

async function mockSystemReady(page: Page): Promise<void> {
  await stubSystemStatus(page, { ready: true, inferenceReady: true });
}

async function mockInferStream(page: Page): Promise<void> {
  const body = buildStream({
    runId: seeded.traceId,
    text: 'Hello from AdapterOS demo.',
    totalTokens: 8,
    latencyMs: 50,
  });
  await stubInferStream(page, body);
}

async function mockTraceDetail(page: Page): Promise<void> {
  const traceId = seeded.traceId;
  const adapterId = seeded.adapterId;

  await page.route(`**/v1/ui/traces/inference/${traceId}*`, async (route) => {
    const url = new URL(route.request().url());
    const tokensAfter = url.searchParams.get('tokens_after');

    const base = {
      trace_id: traceId,
      request_id: null,
      created_at: new Date().toISOString(),
      latency_ms: 12,
      adapters_used: [adapterId],
      stack_id: null,
      model_id: null,
      policy_id: null,
      timing_breakdown: {
        total_ms: 12,
        routing_ms: 1,
        inference_ms: 10,
        policy_ms: 1,
        prefill_ms: null,
        decode_ms: null,
      },
      receipt: {
        receipt_digest: 'receipt-digest',
        run_head_hash: 'run-head-hash',
        output_digest: 'output-digest',
        input_digest_b3: 'input-digest',
        seed_lineage_hash: 'seed-lineage',
        backend_attestation_b3: 'backend-attestation',
        logical_prompt_tokens: 8,
        logical_output_tokens: 12,
        stop_reason_code: 'stop',
        stop_reason_token_index: 12,
        verified: true,
        processor_id: 'processor',
        engine_version: 'engine',
        ane_version: 'ane',
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
      },
      backend_id: 'mlx',
    };

    if (tokensAfter) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          ...base,
          token_decisions: [
            {
              token_index: 2,
              token_id: null,
              adapter_ids: [adapterId],
              gates_q15: [123],
              entropy: 0.1,
              decision_hash: 'hash-2',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
            {
              token_index: 3,
              token_id: null,
              adapter_ids: [adapterId],
              gates_q15: [124],
              entropy: 0.1,
              decision_hash: 'hash-3',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
          ],
          token_decisions_next_cursor: null,
          token_decisions_has_more: false,
        }),
      });
    } else {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          ...base,
          token_decisions: [
            {
              token_index: 0,
              token_id: null,
              adapter_ids: [adapterId],
              gates_q15: [120],
              entropy: 0.1,
              decision_hash: 'hash-0',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
            {
              token_index: 1,
              token_id: null,
              adapter_ids: [adapterId],
              gates_q15: [121],
              entropy: 0.1,
              decision_hash: 'hash-1',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
          ],
          token_decisions_next_cursor: 1,
          token_decisions_has_more: true,
        }),
      });
    }
  });
}

async function mockReplay(page: Page): Promise<void> {
  const traceId = seeded.traceId;

  await page.route('**/v1/diag/export', async (route) => {
    const now = Date.now();
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        format: 'json',
        run: {
          id: traceId,
          trace_id: traceId,
          status: 'completed',
          started_at_unix_ms: now - 1_000,
          completed_at_unix_ms: now,
          request_hash: 'reqhash',
          request_hash_verified: true,
          manifest_hash: 'manihash',
          manifest_hash_verified: true,
          total_events_count: 2,
          dropped_events_count: 0,
          duration_ms: 1000,
          created_at: new Date(now - 1_000).toISOString(),
        },
        events: [],
        timing_summary: [],
        metadata: {
          exported_at: new Date().toISOString(),
          events_exported: 0,
          events_total: 0,
          truncated: false,
        },
      }),
    });
  });

  await page.route('**/v1/replay/verify/trace', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ pass: true, reasons: [] }),
    });
  });

  await page.route('**/v1/replay/sessions/*/execute', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        session_id: `${traceId}-replay`,
        output: 'Hello from AdapterOS demo.',
        degraded: false,
        missing_doc_ids: [],
        no_rag_state_stored: false,
        latency_ms: 42,
        verified_at: new Date().toISOString(),
      }),
    });
  });
}

async function mockAdaptersList(page: Page): Promise<void> {
  await page.route('**/v1/adapters', async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          adapter_id: seeded.adapterId,
          name: seeded.adapterName,
          hash_b3: 'a'.repeat(64),
          rank: 8,
          tier: 'warm',
          languages: ['English'],
          framework: 'General',
          category: 'code',
          scope: 'global',
          created_at: '2025-01-01T00:00:00Z',
          updated_at: '2025-01-01T00:00:00Z',
        },
      ]),
    });
  });
}

async function mockTrainingStatus(page: Page): Promise<void> {
  await page.route('**/v1/training/jobs', async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          job_id: seeded.trainingJobId,
          adapter_id: seeded.adapterId,
          status: 'completed',
          progress: 1.0,
          epochs_completed: 3,
          epochs_total: 3,
          created_at: '2025-01-01T00:00:00Z',
          updated_at: '2025-01-01T01:00:00Z',
        },
      ]),
    });
  });
}

async function mockDocumentsList(page: Page): Promise<void> {
  await page.route('**/v1/documents', async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        documents: [
          {
            id: seeded.documentId,
            filename: 'architecture.md',
            content_type: 'text/markdown',
            size_bytes: 4096,
            chunk_count: 3,
            status: 'processed',
            created_at: '2025-01-01T00:00:00Z',
          },
        ],
        total: 1,
      }),
    });
  });
}

async function mockDatasetsList(page: Page): Promise<void> {
  await page.route('**/v1/datasets', async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        datasets: [
          {
            id: seeded.datasetId,
            name: 'demo-dataset',
            format: 'jsonl',
            row_count: 100,
            size_bytes: 8192,
            status: 'ready',
            created_at: '2025-01-01T00:00:00Z',
          },
        ],
        total: 1,
      }),
    });
  });
}

// ---------------------------------------------------------------------------
// Preset registry & public API
// ---------------------------------------------------------------------------

const PRESET_INSTALLERS: Record<MockPreset, (page: Page) => Promise<void>> = {
  'system-ready': mockSystemReady,
  'infer-stream': mockInferStream,
  'trace-detail': mockTraceDetail,
  'replay': mockReplay,
  'adapters-list': mockAdaptersList,
  'training-status': mockTrainingStatus,
  'documents-list': mockDocumentsList,
  'datasets-list': mockDatasetsList,
};

/** Install a specific set of mock presets. */
export async function installMocks(page: Page, presets: MockPreset[]): Promise<void> {
  for (const preset of presets) {
    const installer = PRESET_INSTALLERS[preset];
    await installer(page);
  }
}

/** Install all available mock presets. */
export async function installAllMocks(page: Page): Promise<void> {
  const allPresets = Object.keys(PRESET_INSTALLERS) as MockPreset[];
  await installMocks(page, allPresets);
}
