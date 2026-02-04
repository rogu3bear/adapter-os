import { test, expect } from '@playwright/test';
import { seeded, waitForAppReady } from './utils';

test('documents list and detail', async ({ page }) => {
  await page.goto('/documents', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Documents', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Fixture Document')).toBeVisible();

  await page.goto(`/documents/${seeded.documentId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Document Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});

test('documents upload dialog opens and cancels', async ({ page }) => {
  await page.goto('/documents', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await page.getByRole('button', { name: 'Upload Document' }).click();
  await expect(
    page.getByRole('heading', { name: 'Upload Document', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(
    page.getByRole('heading', { name: 'Upload Document', exact: true })
  ).toHaveCount(0);
});
