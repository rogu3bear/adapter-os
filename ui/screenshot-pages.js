const { chromium } = require('playwright');

async function captureScreenshots() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 }
  });
  const page = await context.newPage();
  
  // Set the API URL environment variable
  await page.addInitScript(() => {
    window.env = { VITE_API_URL: 'http://127.0.0.1:8080/api' };
  });

  try {
    // Navigate to the app
    await page.goto('http://localhost:3000');
    await page.waitForLoadState('networkidle');

    // Wait a bit for the app to load
    await page.waitForTimeout(2000);

    // Capture login page
    await page.screenshot({ path: '/Users/star/Desktop/adapteros-login.png', fullPage: true });
    console.log('Captured login page');

    // Try to login (assuming default credentials or mock login)
    // Since we don't have real credentials, we'll simulate the login flow
    try {
      // Look for login form elements
      const emailInput = await page.locator('input[type="email"], input[name="email"], input[placeholder*="email" i]').first();
      const passwordInput = await page.locator('input[type="password"], input[name="password"]').first();
      const loginButton = await page.locator('button[type="submit"], button:has-text("Login"), button:has-text("Sign In"), button:has-text("Log in")').first();

      if (await emailInput.isVisible()) {
        await emailInput.fill('admin@aos.local');
        await passwordInput.fill('password');
        await loginButton.click();
        
        // Wait for login to complete
        await page.waitForTimeout(5000);
        await page.waitForLoadState('networkidle');
        
        // Wait for navigation to appear
        await page.waitForSelector('nav button', { timeout: 10000 });
        
        // Check if we're still on login page (login failed)
        const currentUrl = page.url();
        if (currentUrl.includes('login') || await page.locator('input[type="email"]').isVisible()) {
          console.log('Login failed, trying to bypass authentication for demo');
          // Try to bypass auth by setting localStorage or sessionStorage
          await page.evaluate(() => {
            localStorage.setItem('auth_token', 'demo-token');
            localStorage.setItem('user', JSON.stringify({
              id: 'demo-user',
              email: 'admin@aos.local',
              role: 'admin',
              tenant_id: 'default'
            }));
          });
          await page.reload();
          await page.waitForTimeout(3000);
        }
      }
    } catch (error) {
      console.log('Login form not found or login failed, trying to bypass auth:', error.message);
      // Try to bypass auth by setting localStorage
      await page.evaluate(() => {
        localStorage.setItem('auth_token', 'demo-token');
        localStorage.setItem('user', JSON.stringify({
          id: 'demo-user',
          email: 'admin@aos.local',
          role: 'admin',
          tenant_id: 'default'
        }));
      });
      await page.reload();
      await page.waitForTimeout(3000);
    }

    // Capture dashboard first
    await page.screenshot({ 
      path: `/Users/star/Desktop/adapteros-dashboard.png`, 
      fullPage: true 
    });
    console.log('Captured Dashboard page');

    // List of pages to capture based on the sidebar navigation
    const pages = [
      { id: 'tenants', label: 'Tenants' },
      { id: 'nodes', label: 'Nodes' },
      { id: 'adapters', label: 'Adapters' },
      { id: 'plans', label: 'Plans' },
      { id: 'promotion', label: 'Promotion' },
      { id: 'telemetry', label: 'Telemetry' },
      { id: 'policies', label: 'Policies' },
      { id: 'code', label: 'Code Intelligence' }
    ];

    // Try to navigate to each page and capture screenshots
    for (const pageInfo of pages) {
      try {
        // Wait for navigation to be available
        await page.waitForSelector('nav button', { timeout: 5000 });
        
        // Look for the sidebar navigation button
        const navButton = await page.locator(`nav button:has-text("${pageInfo.label}")`).first();
        
        if (await navButton.isVisible()) {
          await navButton.click();
          await page.waitForTimeout(3000);
          await page.waitForLoadState('networkidle');
          
          // Capture screenshot
          await page.screenshot({ 
            path: `/Users/star/Desktop/adapteros-${pageInfo.id}.png`, 
            fullPage: true 
          });
          console.log(`Captured ${pageInfo.id} page`);
        } else {
          console.log(`Navigation button "${pageInfo.label}" not found`);
        }
      } catch (error) {
        console.log(`Error capturing ${pageInfo.id}:`, error.message);
      }
    }

  } catch (error) {
    console.error('Error:', error);
  } finally {
    await browser.close();
  }
}

captureScreenshots().catch(console.error);
