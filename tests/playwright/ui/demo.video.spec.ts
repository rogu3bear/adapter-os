import { test, expect } from '@playwright/test';
import {
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrap,
  resolveChatEntryState,
  seeded,
  waitForAppReady,
} from './utils';

// Demo video spec:
// - Records video every run (even on pass)
// - Drives the happy path: create skill -> chat -> replay -> signed log verify
test.use({ video: 'on', trace: 'on', screenshot: 'on' });
test.describe('demo: end-to-end happy path', () => {
  test.describe.configure({ mode: 'serial' });

  test('demo video', { tag: ['@demo'] }, async ({ page }) => {
    test.setTimeout(12 * 60_000);
    page.setDefaultTimeout(30_000);
    const traceId = seeded.traceId;

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

    await page.route('**/v1/infer/stream', async (route) => {
      const body = [
        'event: aos.run_envelope',
        `data: {"run_id":"${traceId}","schema_version":"1.0","workspace_id":"ws-fixture","actor":{"subject":"dev-bypass"},"reasoning_mode":false,"determinism_version":"1.0","created_at":"2025-01-01T00:00:00Z"}`,
        '',
        'data: {"event":"Token","text":"Hello from AdapterOS demo."}',
        '',
        'data: {"event":"Done","total_tokens":8,"latency_ms":50}',
        '',
      ].join('\n');
      await route.fulfill({
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
        body,
      });
    });

    await page.route(`**/v1/ui/traces/inference/${traceId}*`, async (route) => {
      const url = new URL(route.request().url());
      const tokensAfter = url.searchParams.get('tokens_after');
      const base = {
        trace_id: traceId,
        request_id: null,
        created_at: new Date().toISOString(),
        latency_ms: 12,
        adapters_used: [seeded.adapterId],
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
                adapter_ids: [seeded.adapterId],
                gates_q15: [123],
                entropy: 0.1,
                decision_hash: 'hash-2',
                backend_id: 'mlx',
                kernel_version_id: 'k1',
              },
              {
                token_index: 3,
                token_id: null,
                adapter_ids: [seeded.adapterId],
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
                adapter_ids: [seeded.adapterId],
                gates_q15: [120],
                entropy: 0.1,
                decision_hash: 'hash-0',
                backend_id: 'mlx',
                kernel_version_id: 'k1',
              },
              {
                token_index: 1,
                token_id: null,
                adapter_ids: [seeded.adapterId],
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

    const csrfHeaders = async () => {
      const cookies = await page.context().cookies();
      const csrfToken = cookies.find((cookie) => cookie.name === 'csrf_token')?.value;
      return csrfToken ? { 'X-CSRF-Token': csrfToken } : {};
    };

    // Ensure auth + CSRF cookie are established before mutating API calls.
    await gotoAndBootstrap(page, '/adapters', { mode: 'ui-only' });
    await waitForAppReady(page);
    await assertNoPanic();

    // 1) Create a brand-new skill.
    const createdSkillName = `Video Skill ${Date.now().toString().slice(-6)}`;
    const adaptersResp = await page.request.get('/v1/adapters', { timeout: 30_000 });
    if (!adaptersResp.ok()) {
      const body = await adaptersResp.text().catch(() => '');
      throw new Error(`Failed to list adapters: ${adaptersResp.status()} ${body}`);
    }
    const adaptersPayload = (await adaptersResp.json()) as Array<{
      adapter_id?: string;
      id?: string;
      name?: string;
    }>;
    const sourceAdapterId = adaptersPayload[0]?.adapter_id ?? adaptersPayload[0]?.id;

    let createdAdapterId: string | undefined;
    let duplicateFailure: string | undefined;
    if (sourceAdapterId) {
      const duplicateResp = await page.request.post(`/v1/adapters/${sourceAdapterId}/duplicate`, {
        data: { name: createdSkillName },
        headers: await csrfHeaders(),
        timeout: 30_000,
      });
      if (duplicateResp.ok()) {
        const duplicatePayload = (await duplicateResp.json()) as {
          adapter_id?: string;
          id?: string;
        };
        createdAdapterId = duplicatePayload.adapter_id ?? duplicatePayload.id;
      } else {
        const body = await duplicateResp.text().catch(() => '');
        duplicateFailure = `Failed to duplicate adapter ${sourceAdapterId}: ${duplicateResp.status()} ${body}`;
      }
    }
    if (!createdAdapterId) {
      const fallbackAdapterId = `video-skill-${Date.now().toString().slice(-6)}`;
      const registerResp = await page.request.post('/v1/adapters/register', {
        data: {
          adapter_id: fallbackAdapterId,
          name: createdSkillName,
          hash_b3: 'a'.repeat(64),
          rank: 8,
          tier: 'warm',
          languages: ['English'],
          framework: 'General',
          category: 'code',
          scope: 'global',
        },
        headers: await csrfHeaders(),
        timeout: 30_000,
      });
      if (!registerResp.ok()) {
        const body = await registerResp.text().catch(() => '');
        const duplicateContext = duplicateFailure ? ` | ${duplicateFailure}` : '';
        throw new Error(
          `Failed to register fallback adapter: ${registerResp.status()} ${body}${duplicateContext}`
        );
      }
      const registerPayload = (await registerResp.json()) as { adapter_id?: string; id?: string };
      createdAdapterId = registerPayload.adapter_id ?? registerPayload.id ?? fallbackAdapterId;
    }

    expect(createdAdapterId).toBeTruthy();

    await gotoAndBootstrap(page, '/adapters', { mode: 'ui-only' });
    await expect(page.getByRole('heading', { name: /Skill Library|Adapters/, level: 1 })).toBeVisible();
    await expect(page.getByText(createdSkillName, { exact: false })).toBeVisible({
      timeout: 60_000,
    });
    await assertNoPanic();

    // 2) Open Studio on the new skill and send a message.
    const createdSkillRow = page
      .locator('tr', { has: page.getByText(createdSkillName, { exact: false }) })
      .first();
    await createdSkillRow.getByRole('button', { name: /Open Studio|Chat/, exact: false }).click();
    await waitForAppReady(page);
    await assertNoPanic();

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

    const input = page.getByTestId('chat-input');
    await input.click();
    await input.fill('Say hello in exactly 5 words.');
    await page.keyboard.press('Enter');

    // Wait for assistant to finish streaming (receipt link only renders when not streaming).
    await expect(page.getByTestId('chat-trace-links')).toBeVisible({
      timeout: 180_000,
    });
    await assertNoPanic();

    // 3) Open restore point and signed logs.
    await page.getByTestId('chat-receipt-link').click();
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
      name: /^(System Restore Points|Replay)$/,
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
    const receiptTab = tabNav.getByRole('button', { name: 'Receipt', exact: true });
    if (await receiptTab.isVisible().catch(() => false)) {
      await receiptTab.click();
    }
    await expect(page.getByText('Receipts & Hashes')).toBeVisible({ timeout: 30_000 });

    const verify = page.getByRole('button', { name: 'Verify on server' });
    if (await verify.isVisible().catch(() => false)) {
      await verify.click();
    }

    await expect(page.getByText('Verified', { exact: true })).toBeVisible({ timeout: 60_000 });
    await assertNoPanic();
  });
});
