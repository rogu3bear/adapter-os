import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from '../ui/utils';

test('stacks list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/stacks', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Runtime Stacks', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('stack.test')).toBeVisible();

  await gotoAndBootstrap(page, `/stacks/${seeded.stackId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Stack Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});
