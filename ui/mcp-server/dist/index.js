#!/usr/bin/env node
/**
 * AdapterOS UI Testing MCP Server
 *
 * Provides tools for Claude to interact with and test the AdapterOS UI:
 * - Launch/close browser
 * - Navigate to pages
 * - Take screenshots
 * - Click elements
 * - Fill forms
 * - Read page content
 * - Execute JavaScript
 */
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema, } from '@modelcontextprotocol/sdk/types.js';
import { chromium } from 'playwright';
const UI_BASE_URL = process.env.AOS_UI_URL || 'http://localhost:3200';
// Global state
let browser = null;
let context = null;
let page = null;
// Tool definitions
const tools = [
    {
        name: 'ui_launch',
        description: 'Launch the browser and open the AdapterOS UI. Must be called before other UI tools.',
        inputSchema: {
            type: 'object',
            properties: {
                headless: {
                    type: 'boolean',
                    description: 'Run browser in headless mode (default: true)',
                    default: true,
                },
            },
        },
    },
    {
        name: 'ui_close',
        description: 'Close the browser and clean up resources.',
        inputSchema: {
            type: 'object',
            properties: {},
        },
    },
    {
        name: 'ui_navigate',
        description: 'Navigate to a specific page in the UI by path (e.g., "/owner/home", "/adapters", "/training").',
        inputSchema: {
            type: 'object',
            properties: {
                path: {
                    type: 'string',
                    description: 'The path to navigate to (e.g., "/owner/home")',
                },
                waitFor: {
                    type: 'string',
                    description: 'Optional selector to wait for after navigation',
                },
            },
            required: ['path'],
        },
    },
    {
        name: 'ui_screenshot',
        description: 'Take a screenshot of the current page. Returns base64 encoded PNG.',
        inputSchema: {
            type: 'object',
            properties: {
                fullPage: {
                    type: 'boolean',
                    description: 'Capture full scrollable page (default: false)',
                    default: false,
                },
                selector: {
                    type: 'string',
                    description: 'Optional CSS selector to screenshot a specific element',
                },
            },
        },
    },
    {
        name: 'ui_click',
        description: 'Click on an element identified by CSS selector or text content.',
        inputSchema: {
            type: 'object',
            properties: {
                selector: {
                    type: 'string',
                    description: 'CSS selector for the element to click',
                },
                text: {
                    type: 'string',
                    description: 'Alternative: click element containing this text',
                },
            },
        },
    },
    {
        name: 'ui_fill',
        description: 'Fill an input field with text.',
        inputSchema: {
            type: 'object',
            properties: {
                selector: {
                    type: 'string',
                    description: 'CSS selector for the input field',
                },
                value: {
                    type: 'string',
                    description: 'Text value to fill',
                },
            },
            required: ['selector', 'value'],
        },
    },
    {
        name: 'ui_get_text',
        description: 'Get text content from elements matching a selector.',
        inputSchema: {
            type: 'object',
            properties: {
                selector: {
                    type: 'string',
                    description: 'CSS selector for elements to read',
                },
            },
            required: ['selector'],
        },
    },
    {
        name: 'ui_get_page_content',
        description: 'Get structured content from the current page including title, URL, headings, and main text.',
        inputSchema: {
            type: 'object',
            properties: {
                includeHtml: {
                    type: 'boolean',
                    description: 'Include raw HTML (default: false)',
                    default: false,
                },
            },
        },
    },
    {
        name: 'ui_wait',
        description: 'Wait for a condition: selector to appear, timeout, or network idle.',
        inputSchema: {
            type: 'object',
            properties: {
                selector: {
                    type: 'string',
                    description: 'Wait for this selector to appear',
                },
                timeout: {
                    type: 'number',
                    description: 'Wait for specified milliseconds',
                },
                networkIdle: {
                    type: 'boolean',
                    description: 'Wait for network to be idle',
                },
            },
        },
    },
    {
        name: 'ui_eval',
        description: 'Execute JavaScript in the page context and return the result.',
        inputSchema: {
            type: 'object',
            properties: {
                script: {
                    type: 'string',
                    description: 'JavaScript code to execute',
                },
            },
            required: ['script'],
        },
    },
    {
        name: 'ui_list_elements',
        description: 'List interactive elements on the page (buttons, links, inputs).',
        inputSchema: {
            type: 'object',
            properties: {
                type: {
                    type: 'string',
                    enum: ['buttons', 'links', 'inputs', 'all'],
                    description: 'Type of elements to list (default: all)',
                    default: 'all',
                },
            },
        },
    },
    {
        name: 'ui_login',
        description: 'Log into the AdapterOS UI with credentials.',
        inputSchema: {
            type: 'object',
            properties: {
                email: {
                    type: 'string',
                    description: 'User email address',
                },
                password: {
                    type: 'string',
                    description: 'User password',
                },
            },
            required: ['email', 'password'],
        },
    },
];
// Tool implementations
async function launchBrowser(headless = true) {
    if (browser) {
        return 'Browser already running. Use ui_close first to restart.';
    }
    browser = await chromium.launch({ headless });
    context = await browser.newContext({
        viewport: { width: 1280, height: 720 },
        deviceScaleFactor: 1, // Force 1x pixel density to reduce screenshot size
    });
    page = await context.newPage();
    await page.goto(UI_BASE_URL);
    await page.waitForLoadState('networkidle');
    return `Browser launched. Navigated to ${UI_BASE_URL}. Current URL: ${page.url()}`;
}
async function closeBrowser() {
    if (browser) {
        await browser.close();
        browser = null;
        context = null;
        page = null;
        return 'Browser closed successfully.';
    }
    return 'No browser to close.';
}
async function navigate(path, waitFor) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    const url = `${UI_BASE_URL}${path}`;
    await page.goto(url);
    await page.waitForLoadState('networkidle');
    if (waitFor) {
        await page.waitForSelector(waitFor, { timeout: 10000 });
    }
    return `Navigated to ${url}. Title: "${await page.title()}"`;
}
async function takeScreenshot(fullPage = false, selector) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    let buffer;
    if (selector) {
        const element = await page.$(selector);
        if (!element)
            throw new Error(`Element not found: ${selector}`);
        buffer = await element.screenshot();
    }
    else {
        buffer = await page.screenshot({ fullPage });
    }
    return buffer.toString('base64');
}
async function click(selector, text) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    if (text) {
        await page.getByText(text, { exact: false }).first().click();
        return `Clicked element containing text: "${text}"`;
    }
    else if (selector) {
        await page.click(selector);
        return `Clicked element: ${selector}`;
    }
    throw new Error('Either selector or text must be provided');
}
async function fill(selector, value) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    await page.fill(selector, value);
    return `Filled ${selector} with value`;
}
async function getText(selector) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    const elements = await page.$$(selector);
    const texts = await Promise.all(elements.map(el => el.textContent()));
    return JSON.stringify(texts.filter(Boolean), null, 2);
}
async function getPageContent(includeHtml = false) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    const content = await page.evaluate((includeHtml) => {
        const result = {
            title: document.title,
            url: window.location.href,
            headings: Array.from(document.querySelectorAll('h1, h2, h3')).map(h => ({
                level: h.tagName,
                text: h.textContent?.trim(),
            })),
            buttons: Array.from(document.querySelectorAll('button')).map(b => b.textContent?.trim()).filter(Boolean),
            links: Array.from(document.querySelectorAll('a[href]')).map(a => ({
                text: a.textContent?.trim(),
                href: a.getAttribute('href'),
            })).filter(l => l.text),
            inputs: Array.from(document.querySelectorAll('input, textarea, select')).map(i => ({
                type: i.getAttribute('type') || i.tagName.toLowerCase(),
                name: i.getAttribute('name'),
                placeholder: i.getAttribute('placeholder'),
                id: i.id,
            })),
            mainText: document.querySelector('main')?.textContent?.trim().slice(0, 2000) || '',
        };
        if (includeHtml) {
            result.html = document.documentElement.outerHTML;
        }
        return result;
    }, includeHtml);
    return JSON.stringify(content, null, 2);
}
async function wait(selector, timeout, networkIdle) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    if (selector) {
        await page.waitForSelector(selector, { timeout: 10000 });
        return `Element appeared: ${selector}`;
    }
    else if (timeout) {
        await page.waitForTimeout(timeout);
        return `Waited ${timeout}ms`;
    }
    else if (networkIdle) {
        await page.waitForLoadState('networkidle');
        return 'Network is idle';
    }
    return 'No wait condition specified';
}
async function evaluate(script) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    const result = await page.evaluate(script);
    return JSON.stringify(result, null, 2);
}
async function listElements(type = 'all') {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    const elements = await page.evaluate((type) => {
        const result = {};
        if (type === 'buttons' || type === 'all') {
            result.buttons = Array.from(document.querySelectorAll('button, [role="button"]')).map((el, i) => ({
                index: i,
                text: el.textContent?.trim().slice(0, 50),
                disabled: el.disabled,
                ariaLabel: el.getAttribute('aria-label'),
            }));
        }
        if (type === 'links' || type === 'all') {
            result.links = Array.from(document.querySelectorAll('a[href]')).map((el, i) => ({
                index: i,
                text: el.textContent?.trim().slice(0, 50),
                href: el.getAttribute('href'),
            }));
        }
        if (type === 'inputs' || type === 'all') {
            result.inputs = Array.from(document.querySelectorAll('input, textarea, select')).map((el, i) => ({
                index: i,
                type: el.getAttribute('type') || el.tagName.toLowerCase(),
                name: el.getAttribute('name'),
                id: el.id,
                placeholder: el.getAttribute('placeholder'),
                value: el.value?.slice(0, 50),
            }));
        }
        return result;
    }, type);
    return JSON.stringify(elements, null, 2);
}
async function login(email, password) {
    if (!page)
        throw new Error('Browser not launched. Call ui_launch first.');
    // Navigate to login if not already there
    if (!page.url().includes('/login')) {
        await page.goto(`${UI_BASE_URL}/login`);
        await page.waitForLoadState('networkidle');
    }
    // Fill login form
    await page.fill('input[type="email"], input[name="email"]', email);
    await page.fill('input[type="password"], input[name="password"]', password);
    // Click login button
    await page.click('button[type="submit"]');
    await page.waitForLoadState('networkidle');
    // Wait for navigation away from login page
    await page.waitForURL(url => !url.toString().includes('/login'), { timeout: 10000 }).catch(() => { });
    return `Login attempted. Current URL: ${page.url()}`;
}
// Create and run server
const server = new Server({
    name: 'adapteros-ui-mcp',
    version: '0.1.0',
}, {
    capabilities: {
        tools: {},
    },
});
// Handle tool listing
server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools,
}));
// Handle tool execution
server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
        let result;
        switch (name) {
            case 'ui_launch':
                result = await launchBrowser(args?.headless);
                break;
            case 'ui_close':
                result = await closeBrowser();
                break;
            case 'ui_navigate':
                result = await navigate(args?.path, args?.waitFor);
                break;
            case 'ui_screenshot':
                result = await takeScreenshot(args?.fullPage, args?.selector);
                break;
            case 'ui_click':
                result = await click(args?.selector, args?.text);
                break;
            case 'ui_fill':
                result = await fill(args?.selector, args?.value);
                break;
            case 'ui_get_text':
                result = await getText(args?.selector);
                break;
            case 'ui_get_page_content':
                result = await getPageContent(args?.includeHtml);
                break;
            case 'ui_wait':
                result = await wait(args?.selector, args?.timeout, args?.networkIdle);
                break;
            case 'ui_eval':
                result = await evaluate(args?.script);
                break;
            case 'ui_list_elements':
                result = await listElements(args?.type);
                break;
            case 'ui_login':
                result = await login(args?.email, args?.password);
                break;
            default:
                throw new Error(`Unknown tool: ${name}`);
        }
        return {
            content: [
                {
                    type: 'text',
                    text: result,
                },
            ],
        };
    }
    catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        return {
            content: [
                {
                    type: 'text',
                    text: `Error: ${message}`,
                },
            ],
            isError: true,
        };
    }
});
// Run server
async function main() {
    const transport = new StdioServerTransport();
    await server.connect(transport);
    console.error('AdapterOS UI MCP Server running on stdio');
}
main().catch(console.error);
