import { test, expect, type Page } from '@playwright/test';
import {
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrap,
  gotoChatEntryAndResolve,
  seeded,
} from './utils';
import { buildStream, stubInferStream, stubSystemStatus } from './helpers/sse';

// Chat visual baselines are intentionally deferred while reduced-shell stabilization is in progress.
const ENABLE_CHAT_VISUALS = false;

async function stubAttachUploadFlow(page: Page, opts?: { chunkCount?: number }): Promise<void> {
  const now = '2026-02-19T10:00:00Z';
  const chunkCount = opts?.chunkCount ?? 12;
  const documentId = 'doc-attach-visual';
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
        chunk_count: chunkCount,
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
}

test.describe('visual baselines', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await disableAnimations(page);
  });

  test('training detail', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
    await page
      .getByRole('row', { name: new RegExp(seeded.adapterName) })
      .click();
    await expect(
      page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
    ).toBeVisible();
    const trainingDetail = page.getByTestId('training-job-detail');
    await expect(trainingDetail).toBeVisible();
    const createdRow = trainingDetail.getByTestId('training-detail-created-row');
    await expect(createdRow).toBeVisible();
    await trainingDetail.evaluate((el) => {
      const node = el as HTMLElement;
      node.style.width = '436px';
      node.style.maxWidth = '436px';
    });
    const visibleMasks = [createdRow];
    const optionalMasks = [
      trainingDetail.getByTestId('training-detail-started-row'),
      trainingDetail.getByTestId('training-detail-completed-row'),
    ];
    for (const locator of optionalMasks) {
      if ((await locator.count()) > 0) {
        visibleMasks.push(locator);
      }
    }
    await expect(trainingDetail).toHaveScreenshot('training-detail.png', {
      maxDiffPixels: 10000,
      maxDiffPixelRatio: 0.05,
      mask: visibleMasks,
    });
  });

  test('adapters list', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/adapters', { mode: 'ui-only' });
    const adaptersListCard = page.getByTestId('adapters-list-card');
    await expect(adaptersListCard).toBeVisible();
    await expect(adaptersListCard).toHaveScreenshot('adapters.png', {
      maxDiffPixels: 150,
    });
  });

  test('chat trust strip with citations', { tag: ['@visual'] }, async ({ page }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await stubSystemStatus(page, { ready: true });
    await stubInferStream(
      page,
      buildStream({
        runId: 'trace-cited-visual',
        tokens: ['Grounded', ' response', ' with', ' citations'],
        totalTokens: 32,
        latencyMs: 95,
        adaptersUsed: ['adapter-test', 'support-docs'],
        citations: [
          {
            adapter_id: 'adapter-test',
            file_path: '/docs/handbook.pdf',
            chunk_id: 'chunk-3',
            page_number: 3,
            preview: 'The support escalation path...',
            relevance_score: 0.92,
            rank: 0,
            citation_id: 'cit-3',
          },
          {
            adapter_id: 'adapter-test',
            file_path: '/docs/handbook.pdf',
            chunk_id: 'chunk-7',
            page_number: 7,
            preview: 'Incident response policy...',
            relevance_score: 0.81,
            rank: 1,
            citation_id: 'cit-7',
          },
        ],
      })
    );

    const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
    if (entry.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
      return;
    }
    await ensureActiveChatSession(page);
    await page.getByTestId('chat-input').fill('What does the handbook say?');
    await page.getByTestId('chat-send').click();
    const messages = page.getByLabel('Chat messages');
    await expect(messages.getByText('Grounded response with citations')).toBeVisible();
    await expect(messages).toHaveScreenshot('chat-trust-strip-cited.png', {
      maxDiffPixels: 1200,
      mask: [page.getByTestId('chat-trace-link')],
    });
  });

  test('chat empty state when inference ready', { tag: ['@visual'] }, async ({ page }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await page.addInitScript(() => {
      window.localStorage.clear();
    });
    await stubSystemStatus(page, { ready: true, inferenceReady: true });
    await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
    const empty = page.getByTestId('chat-empty-state');
    await expect(empty).toBeVisible();
    await expect(empty).toHaveScreenshot('home-chat-empty-ready.png', {
      maxDiffPixels: 800,
    });
  });

  test('chat blocked state when inference not ready', { tag: ['@visual'] }, async ({ page }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await page.addInitScript(() => {
      window.localStorage.clear();
    });
    await stubSystemStatus(page, {
      ready: false,
      inferenceReady: false,
      blockers: ['no_model_loaded'],
    });
    await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
    const unavailable = page.getByTestId('chat-unavailable-state');
    await expect(unavailable).toBeVisible();
    await expect(unavailable).toHaveScreenshot('home-chat-empty-blocked.png', {
      maxDiffPixels: 900,
    });
  });

  test('active config line layout', { tag: ['@visual'] }, async ({ page }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await stubSystemStatus(page, { ready: true });
    const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
    if (entry.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
      return;
    }
    await ensureActiveChatSession(page);
    const activeConfig = page.getByTestId('chat-active-config-line');
    await expect(activeConfig).toBeVisible();
    await expect(activeConfig).toHaveScreenshot('chat-active-config-line.png', {
      maxDiffPixels: 150,
    });
  });

  test('system message style from attach + adapter change', { tag: ['@visual'] }, async ({
    page,
  }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await stubSystemStatus(page, { ready: true });
    await stubAttachUploadFlow(page, { chunkCount: 42 });
    await stubInferStream(
      page,
      buildStream({
        runId: 'trace-system-style-1',
        text: 'First response',
        adaptersUsed: ['base-only'],
      })
    );
    const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
    if (entry.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
      return;
    }
    await ensureActiveChatSession(page);
    await page.getByTestId('chat-attach').click();
    await page.setInputFiles('#chat-attach-upload-file', {
      name: 'handbook.pdf',
      mimeType: 'application/pdf',
      buffer: Buffer.from('%PDF-1.4 trust visual fixture'),
    });
    await page.getByRole('button', { name: 'Create draft' }).click();
    await expect(page.getByText('📎 handbook.pdf added (42 chunks).', { exact: false })).toBeVisible();

    await stubInferStream(
      page,
      buildStream({
        runId: 'trace-system-style-2',
        text: 'Second response',
        adaptersUsed: ['base-only', 'new-adapter'],
      })
    );
    await page.getByTestId('chat-input').fill('Trigger adapter set change');
    await page.getByTestId('chat-send').click();
    await expect(page.getByText('Adapters changed: +new-adapter')).toBeVisible();

    const messages = page.getByLabel('Chat messages');
    await expect(messages).toHaveScreenshot('chat-system-messages.png', {
      maxDiffPixels: 1400,
      mask: [page.getByTestId('chat-trace-link')],
    });
  });

  test('mobile trust strip compact and expanded', { tag: ['@visual'] }, async ({ page }) => {
    test.skip(!ENABLE_CHAT_VISUALS, 'Chat visuals deferred during reduced-shell stabilization.');
    await page.setViewportSize({ width: 375, height: 812 });
    await disableAnimations(page);
    await stubSystemStatus(page, { ready: true });
    await stubInferStream(
      page,
      buildStream({
        runId: 'trace-mobile-cited',
        tokens: ['Compact', ' trust', ' strip'],
        totalTokens: 16,
        latencyMs: 70,
        adaptersUsed: ['support-docs'],
        citations: [
          {
            adapter_id: 'support-docs',
            file_path: '/docs/guide.pdf',
            chunk_id: 'chunk-a',
            page_number: 12,
            preview: 'Getting started guide overview...',
            relevance_score: 0.88,
            rank: 0,
            citation_id: 'cit-a',
          },
          {
            adapter_id: 'support-docs',
            file_path: '/docs/guide.pdf',
            chunk_id: 'chunk-b',
            page_number: 17,
            preview: 'Advanced configuration steps...',
            relevance_score: 0.86,
            rank: 1,
            citation_id: 'cit-b',
          },
        ],
      })
    );
    const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
    if (entry.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
      return;
    }
    await ensureActiveChatSession(page);
    await page.getByTestId('chat-input').fill('Show compact trust strip');
    await page.getByTestId('chat-send').click();
    const messagesArea = page.getByLabel('Chat messages');
    await expect(messagesArea.getByText('Compact trust strip', { exact: true })).toBeVisible();
    const summary = messagesArea.locator('summary.cursor-pointer');
    await expect(summary).toContainText('sources');
    await expect(messagesArea).toHaveScreenshot('chat-mobile-trust-collapsed.png', {
      maxDiffPixels: 1200,
    });
    await summary.click();
    await expect(messagesArea).toHaveScreenshot('chat-mobile-trust-expanded.png', {
      maxDiffPixels: 1200,
    });
  });
});
