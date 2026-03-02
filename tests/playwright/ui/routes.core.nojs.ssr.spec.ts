import { expect, test, type Page } from '@playwright/test';
import { coreRoutes } from './core-routes';

test.use({ javaScriptEnabled: false });

async function assertServerShell(page: Page, path: string): Promise<void> {
  const response = await page.goto(path, { waitUntil: 'domcontentloaded' });
  expect(response, `missing initial response for ${path}`).not.toBeNull();

  if (!response) {
    return;
  }

  expect(response.status(), `unexpected response status for ${path}`).toBeLessThan(500);
  expect(response.headers()['content-type'] ?? '', `non-html response for ${path}`).toContain(
    'text/html'
  );
  expect(response.headers()['x-aos-ssr'] ?? '', `SSR header missing for ${path}`).toBe('1');

  const ssrMarker = page.locator('#aos-boot-probe');
  await expect(ssrMarker).toContainText('HTML: OK');

  const routeRoot = page.locator('#aos-root');
  await expect(routeRoot).toBeVisible();
  const routeHtml = (await routeRoot.innerHTML()).trim();
  expect(routeHtml.length, `empty SSR route content for ${path}`).toBeGreaterThan(0);

  const shell = page.locator('#aos-boot-progress');
  await expect(shell).toBeVisible();

  const shellText = (await shell.innerText()).trim();
  expect(shellText.length, `empty server-rendered shell for ${path}`).toBeGreaterThan(0);
}

for (const route of coreRoutes) {
  test(
    `route no-js SSR shell coverage (core): ${route.path}`,
    { tag: ['@smoke', '@ssr'] },
    async ({ page }) => {
      test.setTimeout(90_000);
      await assertServerShell(page, route.path);
    }
  );
}
