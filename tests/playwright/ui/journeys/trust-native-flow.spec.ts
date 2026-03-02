/**
 * Journey: trust-native chat flows (citations, adapter deltas, replay link).
 */

import { expect, test, type Page, type Route } from '@playwright/test';
import { ensureActiveChatSession, gotoChatEntryAndResolve, seeded } from '../utils';
import { buildStream, stubInferStream, stubSystemStatus } from '../helpers/sse';
import { useConsoleCatcher } from '../helpers/console-catcher';

useConsoleCatcher(test);

async function stubDocumentAttachFlow(page: Page): Promise<void> {
  const now = '2026-02-19T10:00:00Z';
  const documentId = 'doc-trust-journey';
  await page.route('**/v1/documents/upload', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        document_id: documentId,
        name: 'handbook.pdf',
        hash_b3: 'b'.repeat(64),
        size_bytes: 16384,
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
        hash_b3: 'b'.repeat(64),
        size_bytes: 16384,
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
        collection_id: 'col-session',
        name: 'Chat: handbook.pdf',
        description: 'Auto-created from chat attachment',
        document_count: 1,
        tenant_id: 'dev',
        created_at: now,
        updated_at: now,
      }),
    });
  });
  await page.route('**/v1/collections/col-session/documents', async (route) => {
    if (route.request().method() !== 'POST') {
      await route.fallback();
      return;
    }
    await route.fulfill({ status: 204, body: '' });
  });
}

test('document attach to cited answer renders trust strip', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(120_000);
  await stubSystemStatus(page, { ready: true });
  await stubDocumentAttachFlow(page);
  await stubInferStream(
    page,
    buildStream({
      runId: 'trace-cited-journey',
      traceId: 'trace-cited-journey',
      tokens: ['Grounded', ' answer', ' from', ' handbook'],
      latencyMs: 88,
      totalTokens: 24,
      adaptersUsed: ['adapter-test', 'support-docs'],
      citations: [
        {
          adapter_id: 'adapter-test',
          file_path: '/docs/handbook.pdf',
          chunk_id: 'chunk-3',
          page_number: 3,
          preview: 'Escalation policy for incidents',
          relevance_score: 0.91,
          rank: 0,
          citation_id: 'cit-3',
        },
        {
          adapter_id: 'adapter-test',
          file_path: '/docs/handbook.pdf',
          chunk_id: 'chunk-7',
          page_number: 7,
          preview: 'Postmortem requirements',
          relevance_score: 0.82,
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

  const messages = page.getByRole('log', { name: 'Chat messages' });

  await page.getByTestId('chat-attach').click();
  const uploadInput = page.locator('#chat-attach-upload-file');
  await expect(uploadInput).toBeVisible();
  await uploadInput.setInputFiles([{
    name: 'handbook.pdf',
    mimeType: 'application/pdf',
    buffer: Buffer.from('%PDF-1.4 trust journey fixture'),
  }]);
  // The "Selected" text appears inside the attach dialog, not in the messages log.
  await expect(page.getByText('Selected: handbook.pdf')).toBeVisible();
  await page.getByRole('button', { name: 'Create draft' }).click();
  // Upload → poll (1s) → create collection → add document → system message
  await expect(messages.getByText('handbook.pdf added', { exact: false })).toBeVisible({ timeout: 30_000 });

  await page.getByTestId('chat-input').fill('What does the handbook say about escalation?');
  await page.getByTestId('chat-send').click();
  await expect(messages.getByText('Grounded answer from handbook')).toBeVisible();

  const trustCitations = page.getByTestId('chat-citation-chips');
  await expect(trustCitations).toContainText('p3');
  await expect(trustCitations).toContainText('p7');
  await expect(page.getByTestId('chat-adapter-chips')).toContainText('adapter-test');
});

test('adapter change notice fires only when set changes', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(90_000);
  await stubSystemStatus(page, { ready: true });

  let inferCalls = 0;
  await page.route('**/v1/infer/stream', async (route: Route) => {
    inferCalls += 1;
    const body =
      inferCalls === 1
        ? buildStream({
            runId: 'trace-adapters-1',
            traceId: 'trace-adapters-1',
            text: 'First adapter set response',
            adaptersUsed: ['base-only'],
          })
        : buildStream({
            runId: 'trace-adapters-2',
            traceId: 'trace-adapters-2',
            text: 'Second adapter set response',
            adaptersUsed: ['base-only', 'new-adapter'],
          });
    await route.fulfill({
      status: 200,
      headers: { 'content-type': 'text/event-stream' },
      body,
    });
  });

  const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
  if (entry.state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);

  const messages = page.getByRole('log', { name: 'Chat messages' });

  await page.getByTestId('chat-input').fill('Turn one');
  await page.getByTestId('chat-send').click();
  await expect(messages.getByText('First adapter set response')).toBeVisible();
  await expect(messages.getByText('Adapters changed:', { exact: false })).toHaveCount(0);

  await page.getByTestId('chat-input').fill('Turn two');
  await page.getByTestId('chat-send').click();
  await expect(messages.getByText('Second adapter set response')).toBeVisible();
  await expect(messages.getByText('Adapters changed: +new-adapter')).toBeVisible();
});

test('receipt link navigates to replay/trace surfaces', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(90_000);
  await stubSystemStatus(page, { ready: true });
  await stubInferStream(
    page,
    buildStream({
      runId: seeded.traceId,
      traceId: seeded.traceId,
      text: 'Replay me',
      adaptersUsed: ['adapter-test'],
    })
  );

  const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
  if (entry.state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);

  const messages = page.getByRole('log', { name: 'Chat messages' });

  await page.getByTestId('chat-input').fill('Generate a replayable answer');
  await page.getByTestId('chat-send').click();
  await expect(messages.getByText('Replay me')).toBeVisible();

  await page.getByTestId('chat-receipt-link').click();
  await expect(page).toHaveURL(new RegExp(`/runs/${seeded.traceId}\\?tab=receipt`));
});
