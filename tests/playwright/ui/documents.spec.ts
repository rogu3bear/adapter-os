import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from './utils';

test('documents list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/documents', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: /^(Documents|Files)$/, level: 1 })
  ).toBeVisible();
  await expect(page.getByText('Fixture Document')).toBeVisible();

  await gotoAndBootstrap(page, `/documents/${seeded.documentId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: /^(Document Details|File Details)$/, level: 1 })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});

test('documents upload dialog opens and cancels', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/documents', { mode: 'ui-only' });
  await page.getByRole('button', { name: 'Upload Document' }).click();
  await expect(
    page.getByRole('heading', { name: 'Upload Document', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(
    page.getByRole('heading', { name: 'Upload Document', exact: true })
  ).toHaveCount(0);
});

// NOTE: The evidence-fixture seeds an inference_evidence record that links chunk-fixture
// to trace-fixture. However, there is no UI page to view inference evidence directly.
// Evidence is only viewable via /v1/chat/messages/{message_id}/evidence, which requires
// a chat message linked to the inference - not seeded by global-setup.
// The document chunks API response structure would need investigation to test this fixture.
