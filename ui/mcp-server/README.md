# AdapterOS UI MCP Server

An MCP (Model Context Protocol) server that enables Claude to interact with and test the AdapterOS UI using Playwright.

## Installation

```bash
cd ui/mcp-server
npm install
npm run build
npx playwright install chromium
```

## Configuration

The MCP server is configured in `.mcp.json` at the project root:

```json
{
  "mcpServers": {
    "adapteros-ui": {
      "command": "node",
      "args": ["/Users/mln-dev/Dev/adapter-os/ui/mcp-server/dist/index.js"],
      "env": {
        "AOS_UI_URL": "http://localhost:3200"
      }
    }
  }
}
```

## Prerequisites

Before using the MCP server, start the UI dev server:

```bash
cd ui
npm run dev
```

This starts the Vite dev server on port 3200 with proxy to the backend.

## Available Tools

| Tool | Description |
|------|-------------|
| `ui_launch` | Launch browser and open AdapterOS UI |
| `ui_close` | Close browser and cleanup |
| `ui_navigate` | Navigate to a specific path |
| `ui_screenshot` | Take a screenshot (returns base64 PNG) |
| `ui_click` | Click an element by selector or text |
| `ui_fill` | Fill an input field |
| `ui_get_text` | Get text content from elements |
| `ui_get_page_content` | Get structured page content |
| `ui_wait` | Wait for selector/timeout/network idle |
| `ui_eval` | Execute JavaScript in page context |
| `ui_list_elements` | List interactive elements |
| `ui_login` | Log into the UI with credentials |

## Usage Examples

### Launch and navigate
```
ui_launch → Opens browser
ui_navigate path="/owner/home" → Navigate to Owner Home
ui_screenshot → Capture current state
```

### Interact with elements
```
ui_list_elements type="buttons" → List all buttons
ui_click text="Start Training" → Click a button
ui_fill selector="input[name='email']" value="test@test.com"
```

### Get page information
```
ui_get_page_content → Get structured content
ui_get_text selector="h1" → Get heading text
ui_eval script="document.title" → Run JavaScript
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_UI_URL` | `http://localhost:3200` | UI base URL |

## Development

```bash
npm run dev  # Run with tsx (TypeScript directly)
npm run build  # Compile TypeScript
npm start  # Run compiled version
```
