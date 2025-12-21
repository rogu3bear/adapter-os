const { chromium } = require('playwright');

// Port configuration - respects environment variables for multi-developer setups
const BACKEND_PORT = process.env.AOS_SERVER_PORT || '8080';
const UI_PORT = process.env.AOS_UI_PORT || '3200';

async function capturePage(pageId, pageLabel) {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 }
  });
  const page = await context.newPage();

  // Set the API URL environment variable
  await page.addInitScript((port) => {
    window.env = { VITE_API_URL: `http://127.0.0.1:${port}/api` };
  }, BACKEND_PORT);

  try {
    // Navigate to the app
    await page.goto(`http://localhost:${UI_PORT}`);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);

    // Login
    const emailInput = await page.locator('input[type="email"]').first();
    const passwordInput = await page.locator('input[type="password"]').first();
    const loginButton = await page.locator('button:has-text("Secure Login")').first();

    await emailInput.fill('admin@aos.local');
    await passwordInput.fill('password');
    await loginButton.click();
    
    // Wait for login to complete and navigation to appear
    await page.waitForTimeout(5000);
    await page.waitForSelector('nav button', { timeout: 10000 });

    // Click the specific page
    const navButton = await page.locator(`nav button:has-text("${pageLabel}")`).first();
    
    if (await navButton.isVisible()) {
      await navButton.click();
      await page.waitForTimeout(3000);
      await page.waitForLoadState('networkidle');
      
      // Capture screenshot
      await page.screenshot({ 
        path: `/Users/star/Desktop/adapteros-${pageId}.png`, 
        fullPage: true 
      });
      console.log(`Captured ${pageId} page`);
    } else {
      console.log(`Navigation button "${pageLabel}" not found`);
    }

  } catch (error) {
    console.error(`Error capturing ${pageId}:`, error);
  } finally {
    await browser.close();
  }
}

// Capture each page individually
const pages = [
  { id: 'nodes', label: 'Nodes' },
  { id: 'adapters', label: 'Adapters' },
  { id: 'plans', label: 'Plans' },
  { id: 'promotion', label: 'Promotion' },
  { id: 'telemetry', label: 'Telemetry' },
  { id: 'policies', label: 'Policies' },
  { id: 'code', label: 'Code Intelligence' }
];

async function captureAll() {
  for (const pageInfo of pages) {
    await capturePage(pageInfo.id, pageInfo.label);
    // Wait a bit between captures
    await new Promise(resolve => setTimeout(resolve, 2000));
  }
}

captureAll().catch(console.error);
