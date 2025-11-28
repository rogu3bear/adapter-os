#!/usr/bin/env npx tsx
/**
 * Login Flow Test Script
 * Tests the actual login authentication flow in AdapterOS UI
 */

import { chromium, Browser, Page } from 'playwright';

const UI_BASE_URL = process.env.AOS_UI_URL || 'http://localhost:3200';

async function testLoginFlow() {
  console.log('🚀 Starting AdapterOS Login Flow Test');
  console.log(`   Base URL: ${UI_BASE_URL}\n`);

  let browser: Browser | null = null;
  let page: Page | null = null;

  try {
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
    });
    page = await context.newPage();

    // Log all console messages for debugging
    page.on('console', (msg) => {
      const type = msg.type();
      const text = msg.text();
      // Log LoginForm logs and errors/warnings
      if (text.includes('[LoginForm]') || type === 'error' || type === 'warn') {
        console.log(`   [${type.toUpperCase()}] ${text}`);
      }
    });

    // Log network requests for debugging
    page.on('response', async (response) => {
      const url = response.url();
      if (url.includes('healthz') || url.includes('status') || url.includes('login') || url.includes('auth')) {
        console.log(`   [NETWORK] ${response.request().method()} ${url} -> ${response.status()}`);
        try {
          const body = await response.text();
          if (body.length < 500) {
            console.log(`   [RESPONSE] ${body}`);
          }
        } catch {}
      }
    });

    console.log('🧪 Step 1: Navigate to login page');
    await page.goto(`${UI_BASE_URL}/login`);
    await page.waitForLoadState('networkidle');

    console.log('\n🧪 Step 2: Wait for backend health check (up to 15s)');
    let backendReady = false;
    for (let i = 0; i < 15; i++) {
      await page.waitForTimeout(1000);

      // Check if login form is visible (backend is ready)
      // Look for the submit button which only appears in the ready state
      const hasSecureLoginButton = await page.$('button:has-text("Secure Login")');
      const hasEmailInput = await page.$('input[type="email"], input[name="email"]');
      const hasPasswordInput = await page.$('input[type="password"], input[name="password"]');

      if (hasSecureLoginButton || (hasEmailInput && hasPasswordInput)) {
        console.log(`   ✅ Login form appeared after ${i + 1}s`);
        backendReady = true;
        break;
      }

      const loadingText = await page.$('text=Checking backend health');
      const initializingText = await page.$('text=Initializing Services');
      if (loadingText || initializingText) {
        console.log(`   ... Still loading (attempt ${i + 1})`);
      }
    }

    if (!backendReady) {
      // Take screenshot to debug
      await page.screenshot({ path: '/tmp/login-debug.png' });
      console.log('   ❌ Backend never became ready - screenshot saved to /tmp/login-debug.png');

      // Get page content for debugging
      const pageContent = await page.textContent('body');
      console.log(`   Page content: ${pageContent?.slice(0, 500)}`);
      return false;
    }

    console.log('\n🧪 Step 3: Fill login form');
    await page.fill('input[type="email"], input[name="email"]', 'admin@aos.local');
    await page.fill('input[type="password"], input[name="password"]', 'password');
    console.log('   ✅ Credentials entered');

    console.log('\n🧪 Step 4: Submit login form');
    const submitButton = await page.$('button[type="submit"]');
    if (!submitButton) {
      console.log('   ❌ Submit button not found');
      return false;
    }

    await submitButton.click();
    console.log('   ✅ Form submitted');

    console.log('\n🧪 Step 5: Wait for redirect after login');
    await page.waitForTimeout(3000);

    const currentUrl = page.url();
    console.log(`   Current URL: ${currentUrl}`);

    if (currentUrl.includes('/owner') || currentUrl.includes('/dashboard') || !currentUrl.includes('/login')) {
      console.log('   ✅ Successfully redirected after login');
      return true;
    } else {
      // Check for error message
      const errorAlert = await page.$('[role="alert"]');
      if (errorAlert) {
        const errorText = await errorAlert.textContent();
        console.log(`   ❌ Login error: ${errorText}`);
      } else {
        console.log('   ❌ Still on login page but no error shown');
      }
      return false;
    }

  } catch (error) {
    console.error(`\n❌ Test failed with error: ${error instanceof Error ? error.message : String(error)}`);
    return false;
  } finally {
    if (browser) {
      await browser.close();
    }
  }
}

testLoginFlow().then((success) => {
  console.log('\n' + '='.repeat(60));
  console.log(success ? '✅ LOGIN FLOW TEST PASSED' : '❌ LOGIN FLOW TEST FAILED');
  console.log('='.repeat(60));
  process.exit(success ? 0 : 1);
}).catch(console.error);
