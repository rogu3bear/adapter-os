# Vite Configuration Consolidation

## Overview

The UI build system has been consolidated from 3 separate Vite config files into a single unified configuration that handles all build modes.

## Migration Summary

### Before (3 separate configs)
- `vite.config.ts` - Main application build
- `vite.config.minimal.ts` - Minimal build variant
- `vite.config.service-panel.ts` - Service panel build

### After (1 unified config)
- `vite.config.ts` - Handles all build modes via environment variable
- `vite.aliases.json` - Shared dependency aliases (Radix UI, etc.)
- `vite.config.minimal.ts` - **DEPRECATED** (kept for backwards compatibility)
- `vite.config.service-panel.ts` - **DEPRECATED** (kept for backwards compatibility)

## Build Modes

The unified config supports three build modes controlled by the `VITE_BUILD_MODE` environment variable:

### 1. Default Mode (Main Application)
**When to use**: Standard production builds and development

```bash
# Development
pnpm dev

# Build
pnpm build
```

**Output**: `../crates/adapteros-server/static`

**Features**:
- Full Tailwind CSS support
- Optimized chunk splitting (React, Radix UI, Charts, Icons, etc.)
- Terser minification with console/debugger removal
- WebSocket proxy for streaming API
- Comprehensive CSP headers
- Auto-opens browser on dev server start

### 2. Minimal Mode
**When to use**: Lightweight builds with minimal dependencies

```bash
# Development
pnpm dev:minimal

# Build
pnpm build:minimal
```

**Output**: `../crates/adapteros-server/static-minimal`

**Features**:
- React only (no Tailwind CSS)
- Uses `index-minimal.html` entry point
- Minimal proxy configuration (/api and /v1)
- Smaller bundle size

### 3. Service Panel Mode
**When to use**: Service management panel builds

```bash
# Development
pnpm service-panel:dev

# Build
pnpm build:service-panel
```

**Output**: `dist-service-panel`

**Features**:
- Includes react-mermaid shim
- Special proxy for `/api/services`
- Relaxed CSP for service panel UI
- Uses `service-panel.html` entry point
- Listens on port 3300 (configurable via `AOS_PANEL_PORT`)

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `VITE_BUILD_MODE` | `default` | Build mode selector: `default`, `minimal`, or `service-panel` |
| `AOS_UI_PORT` | `3200` | Dev server port (default/minimal modes) |
| `AOS_PANEL_PORT` | `3300` | Dev server port (service-panel mode) |
| `AOS_SERVER_PORT` | `8080` | Backend API server port |

## Shared Aliases

All Radix UI and common dependency aliases are now centralized in `vite.aliases.json`:

```json
{
  "@radix-ui/react-tooltip@1.1.8": "@radix-ui/react-tooltip",
  "@radix-ui/react-dialog@1.1.6": "@radix-ui/react-dialog",
  ...
}
```

This eliminates duplication and ensures version consistency across build modes.

## Backwards Compatibility

The old config files (`vite.config.minimal.ts` and `vite.config.service-panel.ts`) are marked as **DEPRECATED** but still functional. They will be removed in a future release.

### Old Command → New Command

```bash
# Minimal builds
OLD: vite build --config vite.config.minimal.ts
NEW: VITE_BUILD_MODE=minimal vite build
NEW: pnpm build:minimal

# Service panel builds
OLD: vite --config vite.config.service-panel.ts
NEW: VITE_BUILD_MODE=service-panel vite
NEW: pnpm service-panel:dev
```

## Implementation Details

### Mode Detection

The config detects build mode in the following order:

1. `VITE_BUILD_MODE` environment variable
2. Command-line flags: `--mode=minimal`, `--mode=service-panel`
3. Legacy `--config` flag detection (backwards compatibility)
4. Falls back to `default` mode

### Configuration Functions

The unified config uses pure functions to build mode-specific configurations:

- `getBuildMode()` - Detects and returns current build mode
- `buildAliases()` - Constructs alias map with mode-specific overrides
- `getBuildConfig()` - Returns build options for current mode
- `getServerConfig()` - Returns dev server options for current mode
- `getPlugins()` - Returns Vite plugins for current mode
- `getOptimizeDeps()` - Returns optimization hints for current mode

### Type Safety

All configuration functions return properly typed Vite configuration objects. The config file includes proper TypeScript type hints for IDE support.

## Benefits

1. **Single source of truth**: All build configurations in one place
2. **DRY principle**: Shared aliases eliminate 40+ lines of duplication
3. **Easier maintenance**: Update dependencies/aliases in one location
4. **Better discoverability**: All build modes visible in package.json
5. **Consistent behavior**: Same plugins and optimizations across modes
6. **Environment-driven**: Easy to integrate with CI/CD pipelines

## Testing

To verify the consolidation works correctly:

```bash
# Test default mode
pnpm build
ls -la ../crates/adapteros-server/static

# Test minimal mode
pnpm build:minimal
ls -la ../crates/adapteros-server/static-minimal

# Test service panel mode
pnpm build:service-panel
ls -la dist-service-panel

# Test dev servers
pnpm dev              # Port 3200
pnpm dev:minimal      # Port 3200
pnpm service-panel:dev # Port 3300
```

## Future Work

- [ ] Remove deprecated config files after migration period
- [ ] Add validation for unknown build modes
- [ ] Consider adding `build:all` script to build all modes
- [ ] Add mode-specific environment validation
- [ ] Generate TypeScript types for vite.aliases.json

## Questions?

See the unified config at `ui/vite.config.ts` or the shared aliases at `ui/vite.aliases.json`.
