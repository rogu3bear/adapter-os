#!/usr/bin/env node
/**
 * Test UI Routes Script
 * Tests navigation to additional AdapterOS UI pages
 */

import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname } from 'path';

const UI_BASE_URL = process.env.AOS_UI_URL || 'http://localhost:3200';
const SCREENSHOT_DIR = '/Users/mln-dev/Dev/adapter-os/ui-test-screenshots';

// Ensure screenshot directory exists
try {
  mkdirSync(SCREENSHOT_DIR, { recursive: true });
} catch (e) {
  // Directory might already exist
}

const routesToTest = [
  { path: '/base-models', name: 'Base Models', requiresAuth: true },
  { path: '/code-intelligence', name: 'Code Intelligence', requiresAuth: true },
  { path: '/metrics/advanced', name: 'Advanced Metrics', requiresAuth: true },
  { path: '/help', name: 'Help', requiresAuth: false },
  { path: '/router-config', name: 'Router Config', requiresAuth: true, requiresAdmin: true },
  { path: '/federation', name: 'Federation', requiresAuth: true, requiresAdmin: true },
];

async function testRoutes() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  const results = [];

  try {
    // Navigate to login page
    console.log(`Navigating to ${UI_BASE_URL}/login...`);
    await page.goto(`${UI_BASE_URL}/login`, { waitUntil: 'networkidle' });

    // Login with admin credentials
    console.log('Logging in...');

    // Wait for login form to be fully loaded
    await page.waitForSelector('input[type="email"]', { timeout: 5000 });
    await page.waitForSelector('input[type="password"]', { timeout: 5000 });

    await page.fill('input[type="email"]', 'admin@adapteros.ai');
    await page.fill('input[type="password"]', 'admin123');

    // Wait for the submit button to be enabled
    await page.waitForSelector('button:has-text("Secure Login"):not([disabled])', { timeout: 5000 });
    await page.click('button:has-text("Secure Login")');

    // Wait for navigation after login (either to dashboard or error)
    await page.waitForTimeout(3000);
    const currentUrl = page.url();
    console.log(`After login, current URL: ${currentUrl}`);

    // Test each route
    for (const route of routesToTest) {
      console.log(`\nTesting route: ${route.path} (${route.name})`);

      try {
        const fullUrl = `${UI_BASE_URL}${route.path}`;
        await page.goto(fullUrl, { waitUntil: 'networkidle', timeout: 10000 });

        // Wait a bit for React to render
        await page.waitForTimeout(1500);

        // Check for errors
        const pageTitle = await page.title();
        const pageUrl = page.url();
        const pageContent = await page.content();

        // Check for common error indicators
        const hasReactError = pageContent.includes('Error:') ||
                             pageContent.includes('Something went wrong') ||
                             pageContent.includes('404');

        // Check if we got redirected (e.g., to login or 404)
        const wasRedirected = !pageUrl.includes(route.path);

        // Take screenshot
        const screenshotPath = `${SCREENSHOT_DIR}/${route.path.replace(/\//g, '_')}.png`;
        await page.screenshot({ path: screenshotPath, fullPage: true });

        const status = hasReactError ? 'ERROR' :
                      wasRedirected ? 'REDIRECTED' :
                      'SUCCESS';

        results.push({
          path: route.path,
          name: route.name,
          status,
          finalUrl: pageUrl,
          screenshot: screenshotPath,
          title: pageTitle
        });

        console.log(`  Status: ${status}`);
        console.log(`  Final URL: ${pageUrl}`);
        console.log(`  Screenshot: ${screenshotPath}`);

      } catch (error) {
        console.error(`  Error testing ${route.path}:`, error.message);
        results.push({
          path: route.path,
          name: route.name,
          status: 'FAILED',
          error: error.message
        });
      }
    }

  } catch (error) {
    console.error('Test failed:', error);
  } finally {
    await browser.close();
  }

  // Print summary
  console.log('\n' + '='.repeat(80));
  console.log('SUMMARY');
  console.log('='.repeat(80));

  for (const result of results) {
    const statusEmoji = result.status === 'SUCCESS' ? '✅' :
                       result.status === 'REDIRECTED' ? '🔄' :
                       result.status === 'ERROR' ? '❌' : '⚠️';
    console.log(`${statusEmoji} ${result.path.padEnd(25)} ${result.status.padEnd(12)} ${result.name}`);
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
  }

  // Write results to JSON
  const resultsPath = `${SCREENSHOT_DIR}/test-results.json`;
  writeFileSync(resultsPath, JSON.stringify(results, null, 2));
  console.log(`\nResults saved to: ${resultsPath}`);

  return results;
}

// Run the tests
testRoutes().catch(console.error);
