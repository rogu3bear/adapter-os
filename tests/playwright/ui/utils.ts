import { expect, Page } from '@playwright/test';

export const seeded = {
  adapterId: 'adapter-test',
  adapterName: 'Test Adapter',
  repoId: 'repo-e2e',
  adapterVersionId: 'adapter-version-e2e',
  trainingJobId: 'job-stub',
  traceId: 'trace-fixture',
  runId: 'trace-fixture',
  documentId: 'doc-fixture',
  stackId: 'stack-test',
};

export async function disableAnimations(page: Page): Promise<void> {
  await page.addStyleTag({
    content: `
      *, *::before, *::after {
        animation-duration: 0s !important;
        animation-delay: 0s !important;
        transition-duration: 0s !important;
        scroll-behavior: auto !important;
      }
    `,
  });
}

export async function waitForAppReady(page: Page): Promise<void> {
  const bootProgress = page.locator('#aos-boot-progress');
  if ((await bootProgress.count()) === 0) {
    return;
  }
  await bootProgress.waitFor({ state: 'hidden', timeout: 90_000 });
}

export async function ensureLoggedIn(page: Page): Promise<void> {
  const loginHeading = page.getByRole('heading', { name: 'Login', exact: true });
  if (!(await loginHeading.isVisible().catch(() => false))) {
    return;
  }
  await page.getByLabel('Username').fill('test@example.com');
  await page.getByLabel('Password').fill('password');
  await page.getByRole('button', { name: 'Log in' }).click();
  await waitForAppReady(page);
}

export async function gotoAndExpectHeading(
  page: Page,
  path: string,
  heading: string
): Promise<void> {
  await page.goto(path, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: heading, level: 1, exact: true })
  ).toBeVisible();
}

export async function expectEmptyState(
  page: Page,
  text: string
): Promise<void> {
  await expect(page.getByText(text)).toBeVisible();
}

export async function expectErrorState(page: Page): Promise<void> {
  const candidates = [
    page.getByRole('button', { name: 'Retry' }),
    page.getByText('Error', { exact: true }),
    page.getByRole('heading', { name: 'Authentication Error' }),
    page.getByRole('heading', { name: 'Authentication Timeout' }),
    page.getByRole('heading', { name: 'Not Found' }),
    page.getByText(/Not Found/i),
    page.locator('#aos-panic-overlay'),
    page.locator('#aos-panic-message'),
  ];

  for (const locator of candidates) {
    if (await locator.isVisible().catch(() => false)) {
      await expect(locator).toBeVisible();
      return;
    }
  }

  await expect(page.getByText('Error', { exact: true })).toBeVisible();
}
