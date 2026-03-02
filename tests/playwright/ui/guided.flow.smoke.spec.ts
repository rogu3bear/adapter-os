import { test, expect, type Page } from '@playwright/test';
import { gotoAndBootstrap, resolveChatEntryState } from './utils';

async function ensureSidebarExpanded(page: Page): Promise<void> {
  const expandSidebar = page.getByRole('button', { name: 'Expand sidebar', exact: true });
  if (await expandSidebar.isVisible().catch(() => false)) {
    await expandSidebar.click();
  }
}

test('guided flow surfaces stay reachable with clear page actions', { tag: ['@smoke'] }, async ({ page }) => {
  test.setTimeout(90_000);
  await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
  await ensureSidebarExpanded(page);

  const sidebar = page.getByRole('navigation', { name: 'Main navigation' });

  await expect(page.getByRole('heading', { name: 'Guided Flow', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Teach New Skill', exact: true }).first()).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open Prompt Studio', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open Restore Points', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open Update Center', exact: true })).toBeVisible();
  await expect(page.getByText('Start here', { exact: true })).toBeVisible();

  await sidebar.getByRole('link', { name: 'Adapter Training', exact: true }).click();
  await expect(page).toHaveURL(/\/training(\/|$|\?)/);
  await expect(page.getByRole('button', { name: 'Create Job', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh', exact: true })).toBeVisible();
  await expect(page.getByText('Filters', { exact: true })).toBeVisible();

  await sidebar.getByRole('link', { name: 'Prompt Studio', exact: true }).click();
  await expect(page).toHaveURL(/\/chat(\/|$|\?)/);
  const chatEntryState = await resolveChatEntryState(page);
  if (chatEntryState === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
  } else {
    await expect(page.getByTestId('chat-header')).toBeVisible();
    await expect(page.getByTestId('chat-advanced-adapter-controls')).toBeVisible();
  }

  await sidebar.getByRole('link', { name: 'Restore Points', exact: true }).click();
  await expect(page).toHaveURL(/\/runs(\/|$|\?)/);
  await expect(page.getByRole('heading', { name: 'Flight Recorder', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh', exact: true }).first()).toBeVisible();

  await sidebar.getByRole('link', { name: 'Update Center', exact: true }).click();
  await expect(page).toHaveURL(/\/update-center(\/|$|\?)/);
  await expect(page.getByRole('heading', { name: 'Update Center', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Teach New Skill', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh', exact: true })).toBeVisible();
});

test(
  'primary profile defaults to guided groups and full keeps govern/org collapsed',
  { tag: ['@smoke'] },
  async ({ page }) => {
    test.setTimeout(90_000);
    await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
    await ensureSidebarExpanded(page);

    const sidebar = page.getByRole('navigation', { name: 'Main navigation' });
    const switchToFull = page.locator('button[title*="switch to Full"]').first();

    await expect(switchToFull).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Adapter Training', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Prompt Studio', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Restore Points', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Update Center', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('button', { name: /^Govern/ })).toHaveCount(0);
    await expect(sidebar.getByRole('button', { name: /^Org/ })).toHaveCount(0);

    await switchToFull.click();
    await expect(page.locator('button[title*="switch to Primary"]').first()).toBeVisible();

    const governGroup = sidebar.getByRole('button', { name: /^Govern/ });
    const orgGroup = sidebar.getByRole('button', { name: /^Org/ });
    await expect(governGroup).toBeVisible();
    await expect(orgGroup).toBeVisible();

    await expect(sidebar.getByRole('link', { name: 'Safety Shield', exact: true })).toHaveCount(0);
    await expect(sidebar.getByRole('link', { name: 'Event Viewer', exact: true })).toHaveCount(0);
    await expect(sidebar.getByRole('link', { name: 'Safety Queue', exact: true })).toHaveCount(0);
    await expect(sidebar.getByRole('link', { name: 'Automation Agents (Beta)', exact: true })).toHaveCount(0);
    await expect(sidebar.getByRole('link', { name: 'Files', exact: true })).toHaveCount(0);
    await expect(sidebar.getByRole('link', { name: 'Admin', exact: true })).toHaveCount(0);

    await governGroup.click();
    await expect(sidebar.getByRole('link', { name: 'Safety Shield', exact: true })).toBeVisible();
  }
);
