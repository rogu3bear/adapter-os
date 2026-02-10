import { test, expect } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { disableAnimations, ensureLoggedIn, waitForAppReady } from './utils';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const readmePath = path.resolve(repoRoot, 'README.md');

// Demo video spec:
// - Records video every run (even on pass)
// - Drives the happy path: upload README -> dataset upload -> create training job -> chat -> receipt verify
test.use({ video: 'on', trace: 'on', screenshot: 'on' });
test.describe('demo: end-to-end happy path', () => {
  test.describe.configure({ mode: 'serial' });

  test('demo video', { tag: ['@demo'] }, async ({ page }) => {
    test.setTimeout(12 * 60_000);

    await disableAnimations(page);
    await page.goto('/documents', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);

    // 1) Upload README.md as a Document
    await page.getByRole('button', { name: 'Upload Document' }).click();
    const uploadDialog = page.getByRole('dialog', { name: 'Upload Document' });
    await expect(uploadDialog).toBeVisible();

    await uploadDialog.locator('input[type="file"]').setInputFiles(readmePath);
    await uploadDialog.getByRole('button', { name: 'Upload' }).click();

    // On success, the dialog closes and we navigate to /documents/:id.
    await page.waitForURL(/\/documents\/[^/?#]+/, { timeout: 120_000 });
    await waitForAppReady(page);

    // 2) Start training flow from the document context (wizard auto-opens)
    const docId = new URL(page.url()).pathname.split('/').filter(Boolean)[1] ?? '';
    expect(docId).toBeTruthy();

    await page.goto(`/training?source=document&document_id=${encodeURIComponent(docId)}`, {
      waitUntil: 'domcontentloaded',
    });
    await waitForAppReady(page);
    await ensureLoggedIn(page);

    const wizard = page.getByRole('dialog', { name: 'New Training Job' });
    await expect(wizard).toBeVisible();

    // 2a) Upload dataset from README.md (Text / Markdown + Echo).
    await wizard.getByRole('button', { name: 'Upload Dataset' }).click();
    const dsWizard = page.getByRole('dialog', { name: 'Upload Training Dataset' });
    await expect(dsWizard).toBeVisible();

    await dsWizard.getByRole('button', { name: 'Text / Markdown' }).click();
    await dsWizard.locator('input[type="file"]').setInputFiles(readmePath);
    await expect(dsWizard.getByText('Preview', { exact: true })).toBeVisible();

    await dsWizard.getByRole('button', { name: 'Upload dataset' }).click();
    await expect(dsWizard).toBeHidden({ timeout: 180_000 });

    // 2b) Model step
    await wizard.getByRole('button', { name: 'Next' }).click();
    await wizard.getByLabel('Adapter Name').fill('demo-readme-adapter');
    await wizard.getByLabel('Base Model').fill('mistral-7b-instruct-v0.3-4bit');
    await wizard.getByLabel('Category').selectOption('docs');

    // 2c) Config step (defaults)
    await wizard.getByRole('button', { name: 'Next' }).click();

    // 2d) Review step -> start training (don't wait for completion)
    await wizard.getByRole('button', { name: 'Next' }).click();
    await wizard.getByRole('button', { name: 'Start Training' }).click();
    await expect(wizard).toBeHidden({ timeout: 120_000 });

    // Show the job exists in the list.
    await expect(page.getByRole('heading', { name: 'Training Jobs', level: 1, exact: true })).toBeVisible();
    await expect(page.getByText('demo-readme-adapter', { exact: false })).toBeVisible({
      timeout: 60_000,
    });

    // 3) Chat: type a prompt, wait for assistant response, open receipt link
    await page.goto('/chat', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);

    // Ensure session exists (New Chat button is present on empty state).
    const newChat = page.getByRole('button', { name: 'New Chat' });
    if (await newChat.isVisible().catch(() => false)) {
      await newChat.click();
    }

    const input = page.getByPlaceholder('Type your message...');
    await input.click();
    await input.fill('Say hello in exactly 5 words.');
    await page.keyboard.press('Enter');

    // Wait for assistant to finish streaming (receipt link only renders when not streaming).
    await expect(page.locator('[data-testid="chat-trace-links"]')).toBeVisible({
      timeout: 180_000,
    });

    await page.locator('[data-testid="chat-receipt-link"]').click();
    await page.waitForURL(/\/runs\/[^/?#]+/, { timeout: 60_000 });
    await waitForAppReady(page);

    // 4) Receipt: verify on server and show verified status
    const tabNav = page.getByRole('navigation').filter({ hasText: 'Overview' });
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
  });
});
