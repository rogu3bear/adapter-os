import { test, expect } from '@playwright/test';
import { ensureActiveChatSession, gotoAndBootstrap } from './utils';

const streamStub = [
  'event: aos.run_envelope',
  'data: {"run_id":"trace-fixture","schema_version":"1.0","workspace_id":"ws-fixture","actor":{"subject":"dev-bypass"},"reasoning_mode":false,"determinism_version":"1.0","created_at":"2025-01-01T00:00:00Z"}',
  '',
  'data: {"event":"Token","text":"Primary lane response"}',
  '',
  'data: {"event":"Done","total_tokens":120,"latency_ms":150}',
  '',
].join('\n');

test('primary lane end-to-end', { tag: ['@flow'] }, async ({ page }) => {
  await page.route('**/v1/system/status', async (route) => {
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
          overall: 'ready',
          checks: {
            db: { status: 'ready' },
            migrations: { status: 'ready' },
            workers: { status: 'ready' },
            models: { status: 'ready' },
          },
        },
        inference_ready: 'true',
        inference_blockers: [],
      }),
    });
  });

  await page.route('**/v1/infer/stream', async (route) => {
    await route.fulfill({
      status: 200,
      headers: { 'content-type': 'text/event-stream' },
      body: streamStub,
    });
  });

  await page.route('**/v1/diag/export', async (route) => {
    const now = Date.now();
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        format: 'json',
        run: {
          id: 'trace-fixture',
          trace_id: 'trace-fixture',
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

  await page.route('**/v1/ui/traces/inference/trace-fixture*', async (route) => {
    const url = new URL(route.request().url());
    const tokensAfter = url.searchParams.get('tokens_after');
    const base = {
      trace_id: 'trace-fixture',
      request_id: null,
      created_at: new Date().toISOString(),
      latency_ms: 12,
      adapters_used: ['adapter-test'],
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
              adapter_ids: ['adapter-test'],
              gates_q15: [123],
              entropy: 0.1,
              decision_hash: 'hash-2',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
            {
              token_index: 3,
              token_id: null,
              adapter_ids: ['adapter-test'],
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
              adapter_ids: ['adapter-test'],
              gates_q15: [120],
              entropy: 0.1,
              decision_hash: 'hash-0',
              backend_id: 'mlx',
              kernel_version_id: 'k1',
            },
            {
              token_index: 1,
              token_id: null,
              adapter_ids: ['adapter-test'],
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

  await gotoAndBootstrap(page, '/chat', {
    mode: 'ui-only',
  });

  await ensureActiveChatSession(page);

  await page.getByTestId('chat-input').fill('hello');
  await page.getByTestId('chat-send').click();

  await expect(
    page.getByLabel('Chat messages').getByText('Primary lane response')
  ).toBeVisible();

  const runLink = page.getByTestId('chat-run-link').first();
  await expect(runLink).toBeVisible();
  const runHref = await runLink.getAttribute('href');
  expect(runHref).toBeTruthy();
  await gotoAndBootstrap(page, runHref as string, { mode: 'ui-only' });
  await expect(page.getByRole('heading', { name: 'Run Detail', level: 2 })).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Run Summary', level: 3 })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Provenance', level: 3, exact: true })
  ).toBeVisible();
  await expect(page.getByTestId('run-provenance-summary')).toBeVisible();
  await expect(
    page.getByTestId('run-provenance-summary').getByText('Receipt status')
  ).toBeVisible();
  await expect(
    page.getByTestId('run-provenance-summary').getByText('Verified')
  ).toBeVisible();

  await page.getByRole('button', { name: 'Trace' }).click();

  const tokenDecisions = page.getByTestId('token-decisions');
  await tokenDecisions.getByRole('button').click();
  await expect(page.getByTestId('token-decisions-list')).toBeVisible();
  await expect(page.getByTestId('token-decision-row')).toHaveCount(2);

  await page.getByTestId('token-decisions-show-more').click();
  await expect(page.getByTestId('token-decision-row')).toHaveCount(4);
});

test('primary lane guardrail: no model loaded', { tag: ['@flow'] }, async ({ page }) => {
  await page.route('**/v1/system/status', async (route) => {
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
          overall: 'ready',
          checks: {
            db: { status: 'ready' },
            migrations: { status: 'ready' },
            workers: { status: 'ready' },
            models: { status: 'not_ready', reason: 'no model loaded' },
          },
        },
        inference_ready: 'false',
        inference_blockers: ['no_model_loaded'],
      }),
    });
  });

  await page.route('**/v1/infer/stream', async (route) => {
    await route.fulfill({
      status: 503,
      contentType: 'application/json',
      body: JSON.stringify({
        message: 'No model loaded',
        code: 'MODEL_NOT_READY',
      }),
    });
  });

  await gotoAndBootstrap(page, '/chat', {
    mode: 'ui-only',
  });

  await expect(page.getByText(/No model loaded/i).first()).toBeVisible();
  await expect(page.getByTestId('chat-unavailable-action')).toBeVisible();
});
