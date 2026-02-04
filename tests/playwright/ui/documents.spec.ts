import { test, expect } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from './utils';

test('documents list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await page.goto('/documents', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Documents', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Fixture Document')).toBeVisible();

  await page.goto(`/documents/${seeded.documentId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Document Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});

test('documents upload dialog opens and cancels', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/documents', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
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
