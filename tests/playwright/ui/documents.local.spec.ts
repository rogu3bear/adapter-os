import { test, expect } from '@playwright/test';
import { firstDocumentId, gotoAndBootstrap } from './utils';

test('documents ingest surface (local)', { tag: ['@local', '@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/documents', { mode: 'ui-only' });

  await expect(page.getByRole('heading', { name: 'Documents', level: 1, exact: true })).toBeVisible();

  // Pipeline summary strip
  await expect(page.getByRole('button', { name: /Ready\/Indexed/i })).toBeVisible();
  await expect(page.getByRole('button', { name: /Processing/i })).toBeVisible();
  await expect(page.getByRole('button', { name: /Failed/i })).toBeVisible();

  // One-click filter: Failed
  await page.getByRole('button', { name: /Failed/i }).click();
  // Select is a native <select>, so validate its value updated to "failed".
  await expect(page.locator('select.select')).toHaveValue('failed');

  // If there is at least one document, verify detail page loads.
  const docId = await firstDocumentId(page);
  if (docId) {
    await gotoAndBootstrap(page, `/documents/${docId}`, { mode: 'ui-only' });
    await expect(
      page.getByRole('heading', { name: 'Document Details', level: 1, exact: true })
    ).toBeVisible();
  }
});
