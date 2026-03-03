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

  await expect(page.getByRole('heading', { name: 'Home', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Quick Start', exact: true })).toBeVisible();
  await expect(page.getByText('Create Adapter', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('View Evidence', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Start Chat', { exact: true }).first()).toBeVisible();

  await sidebar.getByRole('link', { name: 'Build', exact: true }).click();
  await expect(page).toHaveURL(/\/training(\/|$|\?)/);
  await expect(page.getByRole('button', { name: 'Create Adapter', exact: true })).toBeVisible();
  await expect(page.getByText('Filters', { exact: true })).toBeVisible();

  await sidebar.getByRole('link', { name: 'Chat', exact: true }).click();
  await expect(page).toHaveURL(/\/chat(\/|$|\?)/);
  const chatEntryState = await resolveChatEntryState(page);
  if (chatEntryState === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
  } else {
    await expect(page.getByTestId('chat-header')).toBeVisible();
    await expect(page.getByTestId('chat-advanced-adapter-controls')).toBeVisible();
  }

  await sidebar.getByRole('link', { name: 'Execution Records', exact: true }).click();
  await expect(page).toHaveURL(/\/runs(\/|$|\?)/);
  await expect(
    page.getByRole('heading', { name: 'Execution Records', exact: true }).first()
  ).toBeVisible();

  await sidebar.getByRole('link', { name: 'Versions', exact: true }).click();
  await expect(page).toHaveURL(/\/update-center(\/|$|\?)/);
  await expect(
    page.getByRole('heading', { name: /^(Update Center|Versions)$/ }).first()
  ).toBeVisible();
});

test(
  'primary profile defaults to guided groups and full keeps govern/org collapsed',
  { tag: ['@smoke'] },
  async ({ page }) => {
    test.setTimeout(90_000);
    await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
    await ensureSidebarExpanded(page);

    const sidebar = page.getByRole('navigation', { name: 'Main navigation' });
    await expect(sidebar.getByRole('link', { name: 'Build', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Chat', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Execution Records', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Versions', exact: true })).toBeVisible();
    await expect(sidebar.getByRole('button', { name: /^Govern/ })).toHaveCount(0);
    await expect(sidebar.getByRole('button', { name: /^Org/ })).toHaveCount(0);

    const switchToFull = page.locator('button[title*="switch to Full"]').first();
    if (!(await switchToFull.isVisible().catch(() => false))) {
      return;
    }

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
