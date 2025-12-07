import { test, expect } from '@playwright/test';

const routesToCheck = [
  '/',
  '/base-models',
  '/security/audit',
  '/router-config',
  '/federation',
  '/training',
];

const viewports = [
  { width: 360, height: 900 },
  { width: 768, height: 1024 },
  { width: 1280, height: 900 },
];

test.describe('layout overflow', () => {
  for (const route of routesToCheck) {
    for (const viewport of viewports) {
      test(`no horizontal overflow on ${route} at ${viewport.width}px`, async ({ page }) => {
        await page.setViewportSize(viewport);
        await page.goto(route);
        await page.waitForTimeout(500);

        const { scrollWidth, innerWidth } = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
        }));

        expect(scrollWidth).toBeLessThanOrEqual(innerWidth);
      });
    }
  }

  test.skip('persona analytics layout (path TBD)', 'TODO: add stable persona analytics route and enable test');
});

