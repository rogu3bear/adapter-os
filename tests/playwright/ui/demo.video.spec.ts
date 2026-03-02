import { test, expect } from '@playwright/test';
import {
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrap,
  resolveChatEntryState,
  seeded,
  waitForAppReady,
} from './utils';
import { buildStream } from './helpers/sse';

// Demo video spec:
// - Records video every run (even on pass)
// - Drives the happy path: create skill -> chat -> replay -> signed log verify
// - Reference lane for UI-level validation (not part of `npm run demo`)
test.use({ video: 'on', trace: 'on', screenshot: 'on' });
test.describe('demo: end-to-end happy path', () => {
  test.describe.configure({ mode: 'serial' });

  test('demo video', { tag: ['@demo'] }, async ({ page }) => {
    test.setTimeout(12 * 60_000);
    page.setDefaultTimeout(30_000);
    const traceId = seeded.traceId;
    const hotSwapTraceId = `${traceId}-hot-swap`;

    // Force a deterministic ready environment for demo capture.
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

    let inferCalls = 0;
    await page.route('**/v1/infer/stream', async (route) => {
      inferCalls += 1;
      const isHotSwapTurn = inferCalls > 1;
      const body = isHotSwapTurn
        ? buildStream({
            runId: hotSwapTraceId,
            traceId: hotSwapTraceId,
            tokens: ['Second', ' pass', ' with', ' adapter'],
            adaptersUsed: [seeded.adapterId, 'support-docs'],
          })
        : buildStream({
            runId: traceId,
            traceId: traceId,
            tokens: ['Grounded', ' answer', ' with', ' citations'],
            adaptersUsed: [seeded.adapterId],
            citations: [
              {
                adapter_id: seeded.adapterId,
                file_path: '/docs/handbook.pdf',
                chunk_id: 'chunk-3',
                page_number: 3,
                preview: 'Escalation policy',
                relevance_score: 0.92,
                rank: 0,
                citation_id: 'demo-cit-3',
              },
            ],
          });
      await route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body,
      });
    });

    await page.route('**/v1/ui/traces/inference/*', async (route) => {
      const url = new URL(route.request().url());
      const tokensAfter = url.searchParams.get('tokens_after');
      const traceMatch = url.pathname.match(/\/v1\/ui\/traces\/inference\/([^/?#]+)/);
      const requestedTraceId = traceMatch?.[1] ?? traceId;
      const isHotSwapTrace = requestedTraceId === hotSwapTraceId;
      const adapterIds = isHotSwapTrace ? [seeded.adapterId, 'support-docs'] : [seeded.adapterId];
      const base = {
        trace_id: requestedTraceId,
        request_id: null,
        created_at: new Date().toISOString(),
        latency_ms: 12,
        adapters_used: adapterIds,
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
                adapter_ids: adapterIds,
                gates_q15: [123],
                entropy: 0.1,
                decision_hash: 'hash-2',
                backend_id: 'mlx',
                kernel_version_id: 'k1',
              },
              {
                token_index: 3,
                token_id: null,
                adapter_ids: adapterIds,
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
                adapter_ids: adapterIds,
                gates_q15: [120],
                entropy: 0.1,
                decision_hash: 'hash-0',
                backend_id: 'mlx',
                kernel_version_id: 'k1',
              },
              {
                token_index: 1,
                token_id: null,
                adapter_ids: adapterIds,
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

    await disableAnimations(page);

    const assertNoPanic = async () => {
      const panicOverlay = page.locator('#aos-panic-overlay');
      const visible = await panicOverlay.isVisible().catch(() => false);
      if (visible) {
        throw new Error('UI panic overlay became visible during demo capture.');
      }
    };

    const dismissStatusCenter = async () => {
      const statusCenter = page.getByRole('dialog', { name: 'Status Center' });
      if (!(await statusCenter.isVisible().catch(() => false))) {
        return;
      }
      // Prefer keyboard close first; this avoids hangs when close controls render off-viewport.
      await page.keyboard.press('Escape').catch(() => {});
      if (await statusCenter.isVisible().catch(() => false)) {
        const closeButton = statusCenter.getByRole('button', { name: 'Close' }).first();
        if (await closeButton.isVisible().catch(() => false)) {
          await closeButton.scrollIntoViewIfNeeded().catch(() => {});
          await closeButton.click({ timeout: 1_500 }).catch(async () => {
            await closeButton.click({ force: true, timeout: 1_000 }).catch(() => {});
          });
        }
      }
      if (await statusCenter.isVisible().catch(() => false)) {
        await page.keyboard.press('Escape').catch(() => {});
      }
      await statusCenter.waitFor({ state: 'hidden', timeout: 2_500 }).catch(() => {});
    };

    const demoTrainingJobId = `demo-training-${Date.now().toString().slice(-6)}`;
    const now = '2026-02-19T10:00:00Z';
    const documentId = 'doc-demo-handbook';
    await page.route('**/v1/adapters/from-dataset/*', async (route) => {
      await route.fulfill({
        status: 202,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: '1.0',
          id: demoTrainingJobId,
          adapter_name: `demo-skill-${Date.now().toString().slice(-6)}`,
        }),
      });
    });
    await page.route('**/v1/documents/upload', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: '1.0',
          document_id: documentId,
          name: 'handbook.pdf',
          hash_b3: 'a'.repeat(64),
          size_bytes: 4096,
          mime_type: 'application/pdf',
          storage_path: './var/uploads/handbook.pdf',
          status: 'processing',
          chunk_count: null,
          tenant_id: 'dev',
          created_at: now,
          updated_at: now,
          deduplicated: false,
          retry_count: 0,
          max_retries: 3,
        }),
      });
    });
    await page.route(`**/v1/documents/${documentId}`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: '1.0',
          document_id: documentId,
          name: 'handbook.pdf',
          hash_b3: 'a'.repeat(64),
          size_bytes: 4096,
          mime_type: 'application/pdf',
          storage_path: './var/uploads/handbook.pdf',
          status: 'indexed',
          chunk_count: 12,
          tenant_id: 'dev',
          created_at: now,
          updated_at: now,
          deduplicated: false,
          retry_count: 0,
          max_retries: 3,
        }),
      });
    });
    await page.route('**/v1/collections', async (route) => {
      if (route.request().method() !== 'POST') {
        await route.fallback();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          schema_version: '1.0',
          collection_id: seeded.collectionId,
          name: 'Chat: handbook.pdf',
          description: 'Auto-created from chat attachment',
          document_count: 1,
          tenant_id: 'dev',
          created_at: now,
          updated_at: now,
        }),
      });
    });
    await page.route(`**/v1/collections/${seeded.collectionId}/documents`, async (route) => {
      if (route.request().method() !== 'POST') {
        await route.fallback();
        return;
      }
      await route.fulfill({ status: 204, body: '' });
    });

    // Ensure auth is established before wizard-driven creation.
    await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
    await waitForAppReady(page);
    await assertNoPanic();

    // 1) Start a brand-new skill through the training wizard (canonical from-dataset path).
    const createdSkillName = `video-skill-${Date.now().toString().slice(-6)}`;
    await page.getByRole('button', { name: 'Create Adapter', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Create Adapter', exact: true })).toBeVisible();

    const sourceSelects = page.locator('.wizard-step-content select');
    await sourceSelects.first().selectOption(seeded.datasetId);
    await page.getByRole('button', { name: 'Use selected dataset', exact: true }).click();
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await page.getByLabel('Adapter name').fill(createdSkillName);
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await page.getByRole('button', { name: 'Next', exact: true }).click();

    const canonicalStartResponse = page.waitForResponse(
      (resp) =>
        resp.request().method() === 'POST' &&
        /\/v1\/adapters\/from-dataset\/[^/]+$/.test(new URL(resp.url()).pathname),
      { timeout: 30_000 }
    );
    await page.getByRole('button', { name: 'Start training', exact: true }).click();
    const startResponse = await canonicalStartResponse;
    expect(startResponse.ok()).toBeTruthy();
    await assertNoPanic();

    // Demo assertions rely on full-shell Prompt Studio contracts; force profile override
    // to keep this lane deterministic when runtime defaults to HUD.
    await page.evaluate(() => {
      const settingsKey = 'adapteros_settings';
      const raw = window.localStorage.getItem(settingsKey);
      const defaults: Record<string, unknown> = {
        theme: 'system',
        compact_mode: false,
        show_timestamps: true,
        default_page: 'chat',
        api_endpoint: null,
        show_telemetry_overlay: false,
        glass_enabled: true,
        trust_display: 'full',
        knowledge_collection_id: null,
      };
      const settings = raw
        ? { ...defaults, ...(JSON.parse(raw) as Record<string, unknown>) }
        : defaults;
      settings.ui_profile = 'full';
      window.localStorage.setItem(settingsKey, JSON.stringify(settings));
    });
    await page.reload({ waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await assertNoPanic();

    // 2) Continue demo in chat and receipts flow.
    await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
    const initialChatEntryState = await resolveChatEntryState(page);
    test.skip(
      initialChatEntryState === 'unavailable',
      'Skipping demo flow because inference is unavailable in this environment.'
    );
    await assertNoPanic();

    const chatEntryState = await resolveChatEntryState(page);
    test.skip(
      chatEntryState === 'unavailable',
      'Skipping demo chat/receipt segment because inference is unavailable in this environment.'
    );

    await ensureActiveChatSession(page);
    const messages = page.getByRole('log', { name: 'Chat messages' });

    // Auto-attach context for grounded chat.
    await page.getByTestId('chat-attach').click();
    const uploadInput = page.locator('#chat-attach-upload-file');
    await expect(uploadInput).toBeVisible();
    await uploadInput.setInputFiles([{
      name: 'handbook.pdf',
      mimeType: 'application/pdf',
      buffer: Buffer.from('%PDF-1.4 demo fixture'),
    }]);
    await page.getByRole('button', { name: 'Create draft' }).click();
    await expect(messages.getByText('handbook.pdf added', { exact: false })).toBeVisible({
      timeout: 30_000,
    });

    const input = page.getByTestId('chat-input');
    await input.click();
    await input.fill('What is the incident escalation policy?');
    await page.keyboard.press('Enter');
    await expect(messages.getByText('Grounded answer with citations')).toBeVisible();

    await input.fill('Run that again with the latest adapter');
    await page.keyboard.press('Enter');
    await expect(messages.getByText('Second pass with adapter')).toBeVisible();
    await expect(messages.getByText('Adapters changed: +support-docs')).toBeVisible();

    // Wait for assistant to finish streaming (receipt link only renders when not streaming).
    await expect(page.getByTestId('chat-trace-links')).toBeVisible({
      timeout: 180_000,
    });
    await assertNoPanic();

    // 3) Open execution record and signed logs.
    await page.getByTestId('chat-receipt-link').last().click();
    await page.waitForURL(/\/runs\/[^/?#]+/, { timeout: 60_000 });
    await waitForAppReady(page);
    await assertNoPanic();
    await dismissStatusCenter();

    // Replay controls are only available in Full profile; switch if needed.
    const switchToFullProfile = page.locator('button[title*="switch to Full"]').first();
    if (await switchToFullProfile.isVisible().catch(() => false)) {
      await switchToFullProfile.click();
    }

    // 4) Replay exactly.
    const tabNav = page.getByRole('navigation').filter({ hasText: 'Overview' });
    const replayTab = tabNav.getByRole('button', {
      name: /^(System Execution Records|Replay)$/,
    });
    await expect(replayTab).toBeVisible({ timeout: 30_000 });
    await replayTab.click();
    const replayRequest = page.waitForResponse(
      (resp) =>
        resp.request().method() === 'POST' &&
        /\/v1\/replay\/sessions\/[^/]+\/execute/.test(new URL(resp.url()).pathname),
      { timeout: 30_000 }
    );
    const replayActionButton = page
      .getByRole('button', { name: /^(Replay Exactly|Execute Replay)$/ })
      .first();
    await expect(replayActionButton).toBeVisible({ timeout: 30_000 });
    await replayActionButton.click();

    const replayDialog = page.getByRole('dialog').filter({ hasText: /Replay/i }).first();
    if (await replayDialog.isVisible().catch(() => false)) {
      const dialogReplayButton = replayDialog
        .getByRole('button', { name: /^(Replay|Execute)$/ })
        .first();
      await expect(dialogReplayButton).toBeVisible({ timeout: 10_000 });
      await dialogReplayButton.click();
    }
    const replayResponse = await replayRequest;
    expect(replayResponse.ok()).toBeTruthy();
    if (await replayDialog.isVisible().catch(() => false)) {
      const closeReplayDialog = replayDialog
        .getByRole('button', { name: /^(Cancel|Close dialog|Close)$/ })
        .first();
      if (await closeReplayDialog.isVisible().catch(() => false)) {
        await closeReplayDialog.click().catch(() => {});
      } else {
        await page.keyboard.press('Escape').catch(() => {});
      }
    }
    await assertNoPanic();

    // 5) Signed log verification.
    const receiptTab = tabNav.getByRole('button', {
      name: /^(Receipt|Signed System Logs)$/i,
    });
    if (await receiptTab.isVisible().catch(() => false)) {
      await receiptTab.click();
    }
    await expect(
      page
        .getByText(/^(Signed Logs & Fingerprints|Signed Log Summary|Verify Signed Log Bundle)$/)
        .first()
    ).toBeVisible({ timeout: 30_000 });

    const verify = page.getByRole('button', { name: 'Verify on server' });
    if (await verify.isVisible().catch(() => false)) {
      await verify.click();
    }

    await Promise.any([
      page.getByText('Verified', { exact: true }).waitFor({ state: 'visible', timeout: 60_000 }),
      page.getByText('Signed log verified', { exact: false }).waitFor({ state: 'visible', timeout: 60_000 }),
    ]);
    await assertNoPanic();
  });
});
