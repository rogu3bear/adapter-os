/**
 * Shared SSE stub helpers for Playwright tests.
 *
 * Builds well-formed text/event-stream bodies that match the server's
 * wire format (see adapteros-server-api/src/handlers/streaming_infer.rs).
 */

import type { Page, Route } from '@playwright/test';

// ---------------------------------------------------------------------------
// SSE body builders
// ---------------------------------------------------------------------------

export interface RunEnvelope {
  run_id: string;
  schema_version?: string;
  workspace_id?: string;
  actor?: { subject: string };
  reasoning_mode?: boolean;
  determinism_version?: string;
  created_at?: string;
}

export interface TokenChunk {
  text: string;
}

export interface DoneEvent {
  total_tokens: number;
  latency_ms: number;
  trace_id?: string;
  prompt_tokens?: number;
  completion_tokens?: number;
  backend_used?: string;
  citations?: Array<{
    adapter_id: string;
    file_path: string;
    chunk_id: string;
    offset_start?: number;
    offset_end?: number;
    preview: string;
    page_number?: number;
    relevance_score?: number;
    rank?: number;
    citation_id?: string;
  }>;
  adapters_used?: string[];
}

export interface AdapterState {
  adapter_id: string;
  uses_per_minute?: number;
  is_active?: boolean;
}

export interface ErrorEvent {
  message: string;
  recoverable?: boolean;
}

/** Build an SSE line block. Empty trailing line acts as the event delimiter. */
function sseBlock(fields: { event?: string; data: string }): string {
  const lines: string[] = [];
  if (fields.event) lines.push(`event: ${fields.event}`);
  lines.push(`data: ${fields.data}`);
  lines.push('');
  return lines.join('\n');
}

/** Build an `aos.run_envelope` SSE event. */
export function runEnvelope(opts: Partial<RunEnvelope> & { run_id: string }): string {
  return sseBlock({
    event: 'aos.run_envelope',
    data: JSON.stringify({
      run_id: opts.run_id,
      schema_version: opts.schema_version ?? '1.0',
      workspace_id: opts.workspace_id ?? 'ws-fixture',
      actor: opts.actor ?? { subject: 'dev-bypass' },
      reasoning_mode: opts.reasoning_mode ?? false,
      determinism_version: opts.determinism_version ?? '1.0',
      created_at: opts.created_at ?? '2025-01-01T00:00:00Z',
    }),
  });
}

/** Build one or more Token SSE events. */
export function tokenChunks(chunks: (string | TokenChunk)[]): string {
  return chunks
    .map((c) => {
      const text = typeof c === 'string' ? c : c.text;
      return sseBlock({ data: JSON.stringify({ event: 'Token', text }) });
    })
    .join('\n');
}

/** Build an AdapterStateUpdate SSE event. */
export function adapterStateUpdate(adapters: AdapterState[]): string {
  return sseBlock({
    data: JSON.stringify({
      event: 'AdapterStateUpdate',
      adapters: adapters.map((a) => ({
        adapter_id: a.adapter_id,
        uses_per_minute: a.uses_per_minute ?? 10,
        is_active: a.is_active ?? true,
      })),
    }),
  });
}

/** Build a Done SSE event. */
export function doneEvent(opts: Partial<DoneEvent> = {}): string {
  // Ensure citation objects include all required fields for the API Citation struct.
  const citations = opts.citations?.map((c) => ({
    adapter_id: c.adapter_id,
    file_path: c.file_path,
    chunk_id: c.chunk_id,
    offset_start: c.offset_start ?? 0,
    offset_end: c.offset_end ?? 0,
    preview: c.preview ?? '',
    ...(c.page_number != null && { page_number: c.page_number }),
    ...(c.relevance_score != null && { relevance_score: c.relevance_score }),
    ...(c.rank != null && { rank: c.rank }),
    ...(c.citation_id && { citation_id: c.citation_id }),
  }));

  return sseBlock({
    data: JSON.stringify({
      event: 'Done',
      total_tokens: opts.total_tokens ?? 8,
      latency_ms: opts.latency_ms ?? 50,
      ...(opts.trace_id && { trace_id: opts.trace_id }),
      ...(opts.prompt_tokens != null && { prompt_tokens: opts.prompt_tokens }),
      ...(opts.completion_tokens != null && { completion_tokens: opts.completion_tokens }),
      ...(opts.backend_used && { backend_used: opts.backend_used }),
      ...(citations?.length && { citations }),
      ...(opts.adapters_used?.length && { adapters_used: opts.adapters_used }),
    }),
  });
}

/** Build an Error SSE event. */
export function errorEvent(opts: ErrorEvent): string {
  return sseBlock({
    data: JSON.stringify({
      event: 'Error',
      message: opts.message,
      ...(opts.recoverable != null && { recoverable: opts.recoverable }),
    }),
  });
}

// ---------------------------------------------------------------------------
// Composite stream builders
// ---------------------------------------------------------------------------

export interface StreamOpts {
  runId?: string;
  traceId?: string;
  text?: string;
  tokens?: string[];
  adapters?: AdapterState[];
  totalTokens?: number;
  latencyMs?: number;
  citations?: DoneEvent['citations'];
  adaptersUsed?: string[];
}

/**
 * Build a complete SSE stream body: run_envelope → tokens → optional
 * adapter update → done.
 */
export function buildStream(opts: StreamOpts = {}): string {
  const runId = opts.runId ?? 'trace-fixture';
  const parts: string[] = [];

  parts.push(runEnvelope({ run_id: runId }));

  if (opts.tokens?.length) {
    parts.push(tokenChunks(opts.tokens));
  } else {
    parts.push(tokenChunks([opts.text ?? 'Hello from stub']));
  }

  if (opts.adapters?.length) {
    parts.push(adapterStateUpdate(opts.adapters));
  }

  parts.push(
    doneEvent({
      total_tokens: opts.totalTokens ?? 8,
      latency_ms: opts.latencyMs ?? 50,
      trace_id: opts.traceId ?? runId,
      citations: opts.citations,
      adapters_used: opts.adaptersUsed,
    })
  );

  // Trailing newline so the last event gets its \n\n terminator.
  // Without this, the WASM SSE parser leaves the Done event unprocessed.
  return parts.join('\n') + '\n';
}

// ---------------------------------------------------------------------------
// Route interceptors
// ---------------------------------------------------------------------------

/** Intercept POST /v1/infer/stream with a static SSE body. */
export async function stubInferStream(page: Page, body: string): Promise<void> {
  await page.route('**/v1/infer/stream', async (route: Route) => {
    await route.fulfill({
      status: 200,
      headers: { 'content-type': 'text/event-stream' },
      body,
    });
  });
}

/** Intercept POST /v1/infer/stream with a 503 error. */
export async function stubInferStreamError(
  page: Page,
  opts: { status?: number; code?: string; message?: string } = {}
): Promise<void> {
  await page.route('**/v1/infer/stream', async (route: Route) => {
    await route.fulfill({
      status: opts.status ?? 503,
      contentType: 'application/json',
      body: JSON.stringify({
        message: opts.message ?? 'No model loaded',
        code: opts.code ?? 'MODEL_NOT_READY',
      }),
    });
  });
}

/** Intercept GET /v1/system/status with a ready (or not-ready) response. */
export async function stubSystemStatus(
  page: Page,
  opts: { ready?: boolean; inferenceReady?: boolean; blockers?: string[] } = {}
): Promise<void> {
  const ready = opts.ready ?? true;
  const inferenceReady = opts.inferenceReady ?? ready;
  await page.route('**/v1/system/status', async (route: Route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        timestamp: new Date().toISOString(),
        integrity: {
          mode: 'best_effort',
          is_federated: false,
          strict_mode: false,
          pf_deny_ok: true,
          drift: { level: 'ok' },
        },
        readiness: {
          overall: ready ? 'ready' : 'not_ready',
          checks: {
            db: { status: 'ready' },
            migrations: { status: 'ready' },
            workers: { status: 'ready' },
            models: ready
              ? { status: 'ready' }
              : { status: 'not_ready', reason: 'no model loaded' },
          },
        },
        inference_ready: inferenceReady ? 'true' : 'false',
        inference_blockers: opts.blockers ?? (inferenceReady ? [] : ['no_model_loaded']),
      }),
    });
  });
}

/** Intercept chat-session tag fetches used by chat workspace hydration. */
export async function stubChatSessionTags(page: Page): Promise<void> {
  await page.route('**/v1/chat/sessions/*/tags', async (route: Route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([]),
    });
  });
}
