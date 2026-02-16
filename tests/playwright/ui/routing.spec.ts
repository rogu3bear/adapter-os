import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

test('routing management tab renders', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/routing', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Routing Debug', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Management')).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'No identity set selected' })
  ).toBeVisible();
});

test('routing decisions tab renders prompt input', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/routing', { mode: 'ui-only' });
  await page.getByRole('tab', { name: 'Decisions' }).click();
  await expect(
    page.getByRole('heading', { name: 'Routing Decisions', level: 2, exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Debug Router' }).click();
  await expect(
    page.getByPlaceholder('Enter a prompt to test routing...')
  ).toBeVisible();
});
