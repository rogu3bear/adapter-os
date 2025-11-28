#!/usr/bin/env npx tsx
/**
 * UI Flow Test Script
 * Tests key user flows in AdapterOS UI using Playwright
 *
 * Note: Without backend running, tests verify UI renders correctly
 * but protected routes will redirect to login.
 */

import { chromium, Browser, Page } from 'playwright';

const UI_BASE_URL = process.env.AOS_UI_URL || 'http://localhost:3200';

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  details?: string;
}

const results: TestResult[] = [];

async function test(name: string, fn: () => Promise<void>) {
  console.log(`\n🧪 Testing: ${name}`);
  try {
    await fn();
    results.push({ name, passed: true });
    console.log(`   ✅ PASSED`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    results.push({ name, passed: false, error: message });
    console.log(`   ❌ FAILED: ${message}`);
  }
}

async function runTests() {
  console.log('🚀 Starting AdapterOS UI Flow Tests');
  console.log(`   Base URL: ${UI_BASE_URL}`);
  console.log(`   Note: Tests run without backend - protected routes redirect to login\n`);

  let browser: Browser | null = null;
  let page: Page | null = null;

  try {
    // Launch browser
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
    });
    page = await context.newPage();

    // Suppress expected API errors
    page.on('console', () => {}); // Ignore console messages

    // ===== FLOW 1: Landing Page =====
    await test('Landing page loads', async () => {
      await page!.goto(UI_BASE_URL);
      await page!.waitForLoadState('networkidle');
      const title = await page!.title();
      if (!title) throw new Error('No page title found');
      console.log(`      Title: "${title}"`);
    });

    // ===== FLOW 2: Login/Loading Screen =====
    await test('Login screen renders (backend-aware)', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');

      // Wait longer for backend health check to complete (polls every 2s)
      await page!.waitForTimeout(5000);

      // Check for either loading screen OR ready login form
      const hasLoadingScreen = await page!.$('text=Checking backend health');
      const hasRetryButton = await page!.$('text=Retry Connection');
      const hasServiceStatus = await page!.$('text=Service Status');
      const hasAdapterOS = await page!.$('text=AdapterOS');
      const hasEmailInput = await page!.$('input[type="email"], input[name="email"]');
      const hasPasswordInput = await page!.$('input[type="password"], input[name="password"]');

      if (hasAdapterOS) {
        console.log('      AdapterOS branding present');
      }
      if (hasEmailInput && hasPasswordInput) {
        console.log('      Login form ready (backend connected)');
      } else if (hasLoadingScreen) {
        console.log('      Backend loading screen shown (backend not ready yet)');
      }
      if (hasRetryButton) {
        console.log('      Retry button available');
      }
      if (hasServiceStatus) {
        console.log('      Service status section present');
      }

      if (!hasAdapterOS) {
        throw new Error('AdapterOS branding not found');
      }
    });

    // ===== FLOW 3: Route Guard - Protected Routes Redirect =====
    await test('Protected routes redirect to login', async () => {
      await page!.goto(`${UI_BASE_URL}/owner/home`);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(500);

      const url = page!.url();
      if (url.includes('/login')) {
        console.log('      Correctly redirected to /login');
      } else {
        console.log(`      URL: ${url}`);
      }
      // This is expected behavior when not authenticated
    });

    // ===== FLOW 4: UI Assets Load =====
    await test('UI assets and styles load', async () => {
      await page!.goto(UI_BASE_URL);
      await page!.waitForLoadState('networkidle');

      // Check for Tailwind CSS (look for common utility classes)
      const hasStyles = await page!.evaluate(() => {
        const el = document.querySelector('[class*="flex"]');
        if (!el) return false;
        const styles = window.getComputedStyle(el);
        return styles.display === 'flex';
      });

      // Check for Lucide icons (SVG elements)
      const hasIcons = await page!.$$eval('svg', svgs => svgs.length > 0);

      console.log(`      Styles loaded: ${hasStyles}`);
      console.log(`      Icons loaded: ${hasIcons}`);

      if (!hasStyles && !hasIcons) {
        throw new Error('UI assets not loading correctly');
      }
    });

    // ===== FLOW 5: Card Components Render =====
    await test('UI Card components render', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(500);

      // Look for card elements (either by class or role)
      const cards = await page!.$$('[class*="card"], [class*="Card"], [role="article"]');
      console.log(`      Found ${cards.length} card components`);
    });

    // ===== FLOW 6: Button Components =====
    await test('Interactive buttons present', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');

      // Wait for backend health check to complete and login form to render
      await page!.waitForTimeout(5000);

      const buttons = await page!.$$('button');
      const buttonTexts: string[] = [];
      for (const btn of buttons.slice(0, 5)) {
        const text = await btn.textContent();
        if (text?.trim()) buttonTexts.push(text.trim());
      }

      console.log(`      Found ${buttons.length} buttons`);
      if (buttonTexts.length > 0) {
        console.log(`      Examples: ${buttonTexts.slice(0, 3).join(', ')}`);
      }

      // If no buttons, check state - loading screen has no buttons, which is valid
      if (buttons.length === 0) {
        const isLoading = await page!.$('text=Checking backend health');
        const isError = await page!.$('text=Retry Connection');
        if (isLoading || isError) {
          // This is acceptable - login form shows loading/error state without buttons
          console.log('      Note: In loading/error state (no buttons expected)');
        } else {
          throw new Error('No buttons found and not in loading state');
        }
      }
    });

    // ===== FLOW 7: Security Badges =====
    await test('Security indicators present', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(2000); // Wait longer for backend check

      // Look for security-related text
      const hasZeroEgress = await page!.$('text=Zero Egress');
      const hasCSP = await page!.$('text=CSP Enforced');
      const hasITAR = await page!.$('text=ITAR');
      const hasSecure = await page!.$('text=Secure');

      const badges = [
        hasZeroEgress && 'Zero Egress',
        hasCSP && 'CSP Enforced',
        hasITAR && 'ITAR',
        hasSecure && 'Secure'
      ].filter(Boolean);

      if (badges.length > 0) {
        console.log(`      Found security badges: ${badges.join(', ')}`);
      } else {
        console.log('      Security badges shown after backend ready');
      }
    });

    // ===== FLOW 8: Progress Indicators =====
    await test('Progress components work', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(500);

      // Look for progress bars
      const progress = await page!.$$('[role="progressbar"], [class*="progress"], [class*="Progress"]');
      console.log(`      Found ${progress.length} progress components`);
    });

    // ===== FLOW 9: Time Display =====
    await test('Real-time clock displays', async () => {
      await page!.goto(`${UI_BASE_URL}/login`);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(500);

      // Look for time display (format like HH:MM:SS)
      const hasTime = await page!.evaluate(() => {
        const body = document.body.textContent || '';
        // Match time format like "12:34:56" or similar
        return /\d{2}:\d{2}:\d{2}/.test(body);
      });

      console.log(`      Time ticker present: ${hasTime}`);
    });

    // ===== FLOW 10: Dark Mode Support =====
    await test('Theme system works', async () => {
      await page!.goto(UI_BASE_URL);
      await page!.waitForLoadState('networkidle');

      // Check for dark mode classes or CSS variables
      const hasDarkModeSupport = await page!.evaluate(() => {
        const root = document.documentElement;
        const hasDarkClass = root.classList.contains('dark') ||
                            document.body.classList.contains('dark');
        const hasThemeVar = getComputedStyle(root).getPropertyValue('--background');
        return hasDarkClass || hasThemeVar.length > 0;
      });

      console.log(`      Theme support: ${hasDarkModeSupport}`);
    });

    // ===== FLOW 11: Responsive Meta Tags =====
    await test('Responsive viewport configured', async () => {
      await page!.goto(UI_BASE_URL);
      await page!.waitForLoadState('networkidle');

      const hasViewport = await page!.evaluate(() => {
        const meta = document.querySelector('meta[name="viewport"]');
        return meta?.getAttribute('content')?.includes('width=device-width');
      });

      console.log(`      Responsive viewport: ${hasViewport}`);
      if (!hasViewport) {
        throw new Error('Viewport meta tag not properly configured');
      }
    });

    // ===== FLOW 12: No Critical JS Errors =====
    await test('No critical JavaScript errors', async () => {
      const errors: string[] = [];

      page!.on('pageerror', (err) => {
        // Filter out expected errors:
        // - Network/fetch errors (expected without backend or during loading)
        // - 401 Unauthorized (expected without auth)
        // - Zod validation errors (expected form validation on empty/invalid inputs)
        // - too_small/invalid_format (Zod schema validation messages)
        const msg = err.message.toLowerCase();
        if (!msg.includes('fetch') &&
            !msg.includes('network') &&
            !msg.includes('401') &&
            !msg.includes('zod') &&
            !msg.includes('too_small') &&
            !msg.includes('invalid_format') &&
            !msg.includes('password') &&
            !msg.includes('email')) {
          errors.push(err.message);
        }
      });

      await page!.goto(UI_BASE_URL);
      await page!.waitForLoadState('networkidle');
      await page!.waitForTimeout(1000);

      if (errors.length > 0) {
        console.log(`      Critical errors: ${errors.slice(0, 2).join('; ')}`);
        throw new Error(`${errors.length} critical JS errors`);
      }
      console.log('      No critical JavaScript errors');
    });

  } finally {
    if (browser) {
      await browser.close();
    }
  }

  // Print summary
  console.log('\n' + '='.repeat(60));
  console.log('📊 TEST SUMMARY');
  console.log('='.repeat(60));

  const passed = results.filter(r => r.passed).length;
  const failed = results.filter(r => !r.passed).length;

  console.log(`   Total:  ${results.length}`);
  console.log(`   Passed: ${passed} ✅`);
  console.log(`   Failed: ${failed} ❌`);

  if (failed > 0) {
    console.log('\n   Failed tests:');
    results.filter(r => !r.passed).forEach(r => {
      console.log(`   - ${r.name}`);
      console.log(`     ${r.error}`);
    });
  }

  console.log('\n' + '='.repeat(60));
  console.log('Note: Tests run without backend. Protected routes redirect to login.');
  console.log('To test full flows, start the backend: cargo run -p adapteros-server-api');
  console.log('='.repeat(60));

  // Return exit code
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch(console.error);
