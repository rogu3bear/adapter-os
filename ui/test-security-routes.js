/**
 * Simple test script to verify Security group navigation routes
 * Tests: /security/policies, /security/audit, /security/compliance
 */

const puppeteer = require('puppeteer');
const fs = require('fs');
const path = require('path');

const BASE_URL = 'http://localhost:3200';
const ROUTES = [
  { path: '/security/policies', name: 'Policies' },
  { path: '/security/audit', name: 'Audit' },
  { path: '/security/compliance', name: 'Compliance' }
];

async function testRoute(browser, route) {
  const page = await browser.newPage();
  const errors = [];

  // Listen for console errors
  page.on('console', msg => {
    if (msg.type() === 'error') {
      errors.push(`Console error: ${msg.text()}`);
    }
  });

  // Listen for page errors
  page.on('pageerror', error => {
    errors.push(`Page error: ${error.message}`);
  });

  try {
    console.log(`\nTesting route: ${route.path}`);

    // Navigate to the route
    const response = await page.goto(`${BASE_URL}${route.path}`, {
      waitUntil: 'networkidle0',
      timeout: 30000
    });

    // Check response status
    const status = response.status();
    console.log(`  HTTP Status: ${status}`);

    if (status !== 200) {
      errors.push(`HTTP Status ${status} instead of 200`);
    }

    // Wait for React to render
    await page.waitForTimeout(2000);

    // Check if the page title contains expected text
    const title = await page.title();
    console.log(`  Page Title: ${title}`);

    // Take a screenshot
    const screenshotDir = path.join(__dirname, 'screenshots');
    if (!fs.existsSync(screenshotDir)) {
      fs.mkdirSync(screenshotDir, { recursive: true });
    }

    const screenshotPath = path.join(screenshotDir, `${route.name.toLowerCase()}.png`);
    await page.screenshot({ path: screenshotPath, fullPage: true });
    console.log(`  Screenshot saved: ${screenshotPath}`);

    // Check for specific text on the page
    const bodyText = await page.evaluate(() => document.body.innerText);
    const hasExpectedContent = bodyText.includes(route.name) || bodyText.includes('AdapterOS');
    console.log(`  Contains expected content: ${hasExpectedContent}`);

    if (!hasExpectedContent) {
      errors.push(`Page does not contain expected content: "${route.name}"`);
    }

    // Check for React error boundaries or error messages
    const hasErrorBoundary = await page.evaluate(() => {
      return document.body.innerHTML.includes('Something went wrong') ||
             document.body.innerHTML.includes('Error:') ||
             document.body.innerHTML.includes('Unable to');
    });

    if (hasErrorBoundary) {
      errors.push('Page contains error boundary or error message');
    }

    return {
      route: route.path,
      status,
      errors,
      success: errors.length === 0
    };

  } catch (error) {
    errors.push(`Exception: ${error.message}`);
    return {
      route: route.path,
      status: 'ERROR',
      errors,
      success: false
    };
  } finally {
    await page.close();
  }
}

async function main() {
  console.log('Starting Security Routes Navigation Test...\n');
  console.log(`Base URL: ${BASE_URL}`);
  console.log(`Routes to test: ${ROUTES.length}\n`);

  const browser = await puppeteer.launch({
    headless: 'new',
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });

  const results = [];

  for (const route of ROUTES) {
    const result = await testRoute(browser, route);
    results.push(result);
  }

  await browser.close();

  // Print summary
  console.log('\n' + '='.repeat(60));
  console.log('TEST SUMMARY');
  console.log('='.repeat(60));

  for (const result of results) {
    console.log(`\n${result.route}:`);
    console.log(`  Status: ${result.success ? '✓ PASS' : '✗ FAIL'}`);
    if (result.errors.length > 0) {
      console.log(`  Errors:`);
      result.errors.forEach(err => console.log(`    - ${err}`));
    }
  }

  const passCount = results.filter(r => r.success).length;
  const failCount = results.filter(r => !r.success).length;

  console.log('\n' + '='.repeat(60));
  console.log(`Total: ${results.length} | Pass: ${passCount} | Fail: ${failCount}`);
  console.log('='.repeat(60) + '\n');

  process.exit(failCount > 0 ? 1 : 0);
}

main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
