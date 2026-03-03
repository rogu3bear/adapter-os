import { test } from '@playwright/test';
import type { DemoContext, DemoOperationMeta } from '../types';
import { createDemoContext, pacingFromEnv } from '../harness';
import { installMocks } from '../mocks';
import { buildStream } from '../../ui/helpers/sse';
import { gotoAndBootstrap, seeded, waitForAppReady } from '../../ui/utils';

export const meta: DemoOperationMeta = {
  id: 'trust-native',
  title: 'Trust-Native Chat Walkthrough',
  mocks: [
    'system-ready',
    'trace-detail',
    'replay',
  ],
  tags: ['demo', 'chat', 'trust'],
};

export async function run(demo: DemoContext): Promise<void> {
  const { page } = demo;
  const now = '2026-02-19T10:00:00Z';
  const documentId = 'doc-demo-handbook';

  // Stub document upload flow (not in centralized mocks — demo-specific)
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

  let inferCalls = 0;
  await page.route('**/v1/infer/stream', async (route) => {
    inferCalls += 1;
    const body =
      inferCalls === 1
        ? buildStream({
            runId: seeded.traceId,
            traceId: seeded.traceId,
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
              {
                adapter_id: seeded.adapterId,
                file_path: '/docs/handbook.pdf',
                chunk_id: 'chunk-7',
                page_number: 7,
                preview: 'Postmortem guidance',
                relevance_score: 0.83,
                rank: 1,
                citation_id: 'demo-cit-7',
              },
            ],
          })
        : buildStream({
            runId: `${seeded.traceId}-2`,
            traceId: `${seeded.traceId}-2`,
            tokens: ['Second', ' pass', ' with', ' adapter'],
            adaptersUsed: [seeded.adapterId, 'support-docs'],
            citations: [
              {
                adapter_id: seeded.adapterId,
                file_path: '/docs/handbook.pdf',
                chunk_id: 'chunk-9',
                page_number: 9,
                preview: 'Escalation checklist',
                relevance_score: 0.9,
                rank: 0,
                citation_id: 'demo-cit-9',
              },
            ],
          });
    await route.fulfill({
      status: 200,
      headers: { 'content-type': 'text/event-stream' },
      body,
    });
  });

  await demo.narrate('AdapterOS boots. The kernel is ready.');
  await demo.dwell(demo.pacing.afterNav);

  await demo.narrate('Drop a document to ground the AI in your data.');
  await page.getByTestId('chat-attach').click();
  await page.setInputFiles('#chat-attach-upload-file', {
    name: 'handbook.pdf',
    mimeType: 'application/pdf',
    buffer: new TextEncoder().encode('%PDF-1.4 demo fixture') as any,
  });
  await page.getByRole('button', { name: 'Create draft' }).click();
  const messages = page.getByRole('log', { name: 'Chat messages' });
  await messages.getByText('📎 handbook.pdf added', { exact: false }).waitFor();
  await demo.dwell(demo.pacing.afterAction);

  await demo.narrate('Ask a question. The answer cites your document.');
  await demo.typeHuman('chat-input', 'What is the incident escalation policy?');
  await page.getByTestId('chat-send').click();
  await messages.getByText('Grounded answer with citations').waitFor();
  await demo.dwell(demo.pacing.afterAction);

  await demo.narrate('Every answer carries proof. Click to verify.');
  await page.getByTestId('chat-receipt-link').click();
  await demo.dwell(demo.pacing.afterAction);
  await page.goBack();
  await page.getByTestId('chat-header').waitFor();

  await demo.narrate('The adapter is ready. Watch the model change.');
  await demo.typeHuman('chat-input', 'Run that again with the latest adapter');
  await page.getByTestId('chat-send').click();
  await messages.getByText('Second pass with adapter').waitFor();
  await messages.getByText('Adapters changed: +support-docs', { exact: false }).waitFor();

  await demo.narrate('Deterministic. Auditable. Yours.');
  await demo.dwell(demo.pacing.finalDwell);
}

test(meta.id, { tag: ['@demo'] }, async ({ page }) => {
  await installMocks(page, meta.mocks);
  const demo = createDemoContext(page, pacingFromEnv());

  await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
  await waitForAppReady(page);

  await run(demo);
});
