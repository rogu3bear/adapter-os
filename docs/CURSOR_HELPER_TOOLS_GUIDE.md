# Cursor Helper Tools Prevention Guide

**Issue:** Cursor IDE keeps creating new helper tools/scripts automatically.

**Root Cause:** Cursor's Background Agents, Plan Mode, and MCP integrations can automatically generate helper scripts to assist with tasks.

## Quick Fix: Disable Background Agents

### Method 1: Via Cursor Settings UI

1. Open Cursor Settings: `Cmd+,` (macOS) or `Ctrl+,` (Windows/Linux)
2. Search for: `background agents` or `plan mode`
3. Disable:
   - `cursor.backgroundAgents.enabled` → Set to `false`
   - `cursor.planMode.enabled` → Set to `false`
   - `cursor.autoGenerateTools` → Set to `false` (if available)

### Method 2: Via settings.json

Edit `~/Library/Application Support/Cursor/User/settings.json` (macOS) or equivalent:

```json
{
  "cursor.backgroundAgents.enabled": false,
  "cursor.planMode.enabled": false,
  "cursor.autoGenerateTools": false,
  "cursor.agentSecurity.requireManualApproval": false,
  "cursor.agentSecurity.fingerprintRequired": false
}
```

**Note:** Disabling agent security reduces protection but stops fingerprint prompts.

## Check for Created Helper Tools

### Common Locations

```bash
# Check project root for helper scripts
find . -maxdepth 1 -name "*helper*" -o -name "*temp*" -o -name "*cursor*"

# Check scripts directory
ls -la scripts/ | grep -E "(helper|temp|cursor|tool)"

# Check for recently created scripts
find . -type f -mtime -7 -name "*.sh" -o -name "*.py" | grep -v node_modules | grep -v target
```

### Clean Up Existing Tools

```bash
# Review and remove helper tools (be careful!)
# List first to see what would be deleted
find . -name "*helper*" -o -name "*cursor-tool*" | grep -v node_modules

# If safe, remove them
find . -name "*helper*" -o -name "*cursor-tool*" | grep -v node_modules | xargs rm -f
```

## MCP Server Configuration

If you have MCP servers configured (like in `.mcp.json`), they may also create helper tools:

```json
{
  "mcpServers": {
    "adapteros-ui": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

**To disable MCP tool creation:**
- Remove or comment out MCP server entries in `.mcp.json`
- Or configure MCP servers to not auto-generate tools

## Cursor Features That Create Tools

| Feature | Creates Tools? | How to Disable |
|---------|----------------|----------------|
| **Background Agents** | ✅ Yes | `cursor.backgroundAgents.enabled: false` |
| **Plan Mode** | ✅ Yes | `cursor.planMode.enabled: false` |
| **MCP Tools** | ✅ Yes | Remove from `.mcp.json` |
| **Bugbot** | ⚠️ Maybe | Disable in GitHub integration settings |
| **Multi-Agent Support** | ✅ Yes | Disable background agents |

## Prevention Checklist

- [ ] Disable Background Agents in settings
- [ ] Disable Plan Mode in settings
- [ ] Configure Agent Security (disable fingerprint requirement if not needed)
- [ ] Review `.mcp.json` and remove unnecessary MCP servers
- [ ] Add helper tool patterns to `.gitignore`:
  ```
  *helper*.sh
  *cursor-tool*
  *temp-*.py
  ```
- [ ] Review Cursor workspace settings for auto-tool generation
- [ ] Check installed extensions for authentication requirements

## Verify Settings

After making changes, verify they're applied:

```bash
# Check Cursor settings file
cat ~/Library/Application\ Support/Cursor/User/settings.json | grep -i "background\|plan\|tool"
```

## Alternative: Use .cursorignore

Add patterns to `.cursorignore` to prevent Cursor from accessing certain directories:

```
# Prevent Cursor from creating tools in these locations
scripts/temp/
scripts/helpers/
*.temp.*
```

## Authentication Fingerprint Request

**Issue:** Cursor requests your "true authentication fingerprint" at user level.

**What This Is:**
- Part of Cursor's **Agent Security** system
- Requires manual approval for sensitive actions
- Used to verify user identity for security-sensitive operations
- May be triggered by Background Agents or other features

**How to Handle:**

### Option 1: Disable Agent Security (Less Secure)
```json
{
  "cursor.agentSecurity.requireManualApproval": false,
  "cursor.agentSecurity.enabled": false
}
```

### Option 2: Configure Agent Security (Recommended)
```json
{
  "cursor.agentSecurity.requireManualApproval": true,
  "cursor.agentSecurity.fingerprintRequired": false
}
```

### Option 3: Review What's Triggering It
1. Check Cursor's Agent Security settings: `Cmd+,` → Search "agent security"
2. Review which features require authentication
3. Disable specific features that trigger fingerprint requests

**Note:** The fingerprint request is a security feature. Disabling it reduces security but may stop the prompts.

## Related Cursor Features

- **Background Agents**: Run multiple AI agents concurrently (creates isolated workspaces, may trigger fingerprint)
- **Plan Mode**: AI drafts step-by-step plans before executing (may create helper scripts)
- **MCP Tools**: Model Context Protocol integrations (can auto-generate tools)
- **Bugbot**: Automatic PR review (may create patches/scripts)
- **Agent Security**: Manual approval system (triggers fingerprint requests)

## References

- Cursor Settings: `Cmd+,` → Search for feature names
- Cursor Documentation: https://cursor.sh/docs
- MCP Protocol: https://modelcontextprotocol.io

---

## Safety Assessment

**✅ SAFE TO DISABLE** - These features are Cursor IDE conveniences, not required for your codebase:

### What's Safe to Disable

1. **Background Agents** ✅ Safe
   - Creates temporary helper scripts in `.cursor/.agent-tools/`
   - Not part of your AdapterOS codebase
   - Only affects Cursor IDE behavior, not your code

2. **Plan Mode** ✅ Safe
   - Creates planning documents/scripts
   - Optional workflow enhancement
   - Your codebase doesn't depend on it

3. **MCP Server** ⚠️ Optional
   - Your `.mcp.json` configures `adapteros-ui` MCP server
   - **Purpose**: UI testing with Playwright (development tool)
   - **Impact**: Only affects Cursor's ability to test UI via browser automation
   - **Safe to disable** if you don't use Cursor for UI testing

### What Won't Break

- ✅ Your codebase compilation
- ✅ Your build system (Cargo, Makefile)
- ✅ Your CI/CD pipelines
- ✅ Your application runtime
- ✅ Git operations
- ✅ Your scripts and tools

### What Might Change

- ⚠️ Cursor won't auto-generate helper scripts
- ⚠️ Background agents won't run concurrently
- ⚠️ Plan Mode won't create step-by-step plans
- ⚠️ MCP UI testing tools won't be available (if you disable MCP)

### Current State

- `.cursor/.agent-tools/` directory exists but is **empty** (20KB total)
- No active helper tools detected
- MCP server configured but optional

### Recommendation

**Start conservative:**
1. ✅ **Disable Background Agents** - Safe, prevents tool creation
2. ✅ **Disable Plan Mode** - Safe, optional feature
3. ⚠️ **Keep MCP server** - Only disable if you don't use UI testing

**If issues arise**, you can re-enable features one at a time.

**Note:** Some helper tools may be necessary for Cursor's functionality. Only disable features you don't need. If you're unsure, disable one feature at a time and observe behavior.

MLNavigator Inc 2025-12-02.

