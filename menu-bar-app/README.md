## AdapterOS Menu Bar App

SwiftUI `MenuBarExtra` companion for local AdapterOS status.

### Build & Run

1. Open in Xcode 15+ or build via SwiftPM:
   - `swift build -c release`
2. Launch the `AdapterOSMenu` target.

Requirements:
- macOS 13+
- Read access to `/var/run/adapteros_status.json`

### Features
- VNODE watcher for instant updates + 5s polling fallback
- Lightweight metrics: CPU%, memory used/total (10s sampling)
- Robust error states (missing, decode, permission)
- Actions: Open Dashboard, Reload, Copy Status JSON

### LaunchAgent (optional)
Use `Config/LaunchAgent.plist` as a template to run on login. Do not install by default; edit the `ProgramArguments` path to your app bundle.

### Screenshots
Add light/dark screenshots here.

# AdapterOS Menu Bar App

## Overview
The AdapterOS Menu Bar App provides real-time monitoring and management of AdapterOS services from the macOS menu bar. It reads system status from `/var/run/adapteros_status.json` and interacts with the service supervisor API at `http://localhost:8081`.

## Features
- Real-time status display (uptime, adapters, base model, services)
- Service management (start/stop essential services, unload models)
- Trust verification and notifications for failures
- Copy utilities for kernel hash, status JSON, and reports
- Accessibility support (VoiceOver, reduced motion)

## Prerequisites
- AdapterOS server and service supervisor running
- Set `SERVICE_PANEL_SECRET` environment variable for auth (shared secret with supervisor)

## Installation
1. Build the app:
   ```
   make menu-bar
   ```
2. Install to `/usr/local/bin`:
   ```
   make menu-bar-install
   ```
3. Run:
   ```
   aos-menu
   ```

## Usage
- Click the menu bar icon (bolt) to view status sections: Header (health/uptime), Tenants, Services (with health/PID), Operations, Management (Open Dashboard, Reload, Copy JSON).
- Problems banner shows errors (e.g., service failures) with retry/logs actions.
- Toasts confirm actions (e.g., "Model unloaded").
- Debug mode (development builds): Performance overlay and sample status loaders.

## Integration Notes
- **Status Polling**: Watches `/var/run/adapteros_status.json` for changes (VNODE events, 5s fallback poll).
- **API Calls**: Manages services via supervisor endpoints (`/api/services/start`, `/api/services/essential/start`, etc.). Models via main server `/v1/models/:id/unload`.
- **Auth**: Basic auth with Keychain-cached tokens (1h TTL). Requires shared secret.
- **Error Handling**: Circuit breaker (5 failures → 60s cooldown), retries (3x exponential backoff), JSON validation.
- **Testing**: `make test-menu-bar-integration` creates sample JSON and verifies build/parsing.

## Troubleshooting
- If offline: Check server status, file permissions on `/var/run/adapteros_status.json`.
- Auth errors: Verify `SERVICE_PANEL_SECRET` env.
- Build issues: Ensure Swift 5.9+, Xcode 15+.

## Development
- Run in debug: `cd menu-bar-app && swift run`
- Tokens: Edit `Resources/DesignTokens.json` for theming.
- Logs: View in Console.app (subsystem: com.adapteros.menu).

For full AdapterOS docs, see root README.md.

## Recent Fixes (2025-01-15)

### Bug Fixes
- **StatusViewModel Hash Comparison**: Fixed redundant status updates, now only updates when content changes
- **Watcher Concurrency**: Added serialization guard to prevent concurrent watcher setup
- **StatusReader Error Context**: Enhanced error messages with detailed decode/validation context
- **ResponseCache Statistics**: Improved accuracy by tracking actual data sizes instead of estimates
- **ServicePanelClient Cache**: Fixed cache check logic for GET requests without body

### Test Coverage
- Added comprehensive unit tests for all components (25+ tests)
- Added integration tests for end-to-end scenarios (9 tests)
- Added stress tests for concurrent operations and rapid updates

See [TESTING.md](TESTING.md) for testing guide and [ARCHITECTURE.md](ARCHITECTURE.md) for architecture details.

## Recent Enhancements (2025-11-13)

### Bug Fixes and Improvements
- **StatusTypes.swift**: Enhanced type definitions for better service status representation and error handling.
- **ServicePanelClient.swift**: Improved API client with better authentication, retry logic, and error recovery.
- **StatusViewModel.swift**: Updated view model for more efficient state management and real-time updates.
- **StatusMenuView.swift**: Refined menu view with improved layout, accessibility, and interaction feedback.

### Test Coverage
- Expanded unit tests for view models and services (additional 15 tests).
- Integration tests for end-to-end status polling and API interactions.

See [TESTING.md](TESTING.md) for testing guide and [ARCHITECTURE.md](ARCHITECTURE.md) for architecture details.

*Last Updated: November 13, 2025*

## Recent Fixes (2025-01-15)

### Bug Fixes
- **StatusViewModel Hash Comparison**: Fixed redundant status updates, now only updates when content changes
- **Watcher Concurrency**: Added serialization guard to prevent concurrent watcher setup
- **StatusReader Error Context**: Enhanced error messages with detailed decode/validation context
- **ResponseCache Statistics**: Improved accuracy by tracking actual data sizes instead of estimates
- **ServicePanelClient Cache**: Fixed cache check logic for GET requests without body

### Test Coverage
- Added comprehensive unit tests for all components (25+ tests)
- Added integration tests for end-to-end scenarios (9 tests)
- Added stress tests for concurrent operations and rapid updates

See [TESTING.md](TESTING.md) for testing guide and [ARCHITECTURE.md](ARCHITECTURE.md) for architecture details.

## Code Signing

For distribution, sign with your Developer ID:

```bash
codesign --sign "Developer ID Application: Your Name" .build/release/AdapterOSMenu
```

## License

Dual-licensed under Apache 2.0 or MIT.

## App Structure Outline

### Core App
- **AdapterOSMenuApp.swift**: Entry point, theme setup.

### Models (Data Structures)
- **StatusTypes.swift**, **DesignTokensModel.swift**: Typed responses, UI tokens.
  - Distinguish: Status (backend data) vs. Tokens (theming).

### Services (Logic)
- **StatusReader.swift**, **ServicePanelClient.swift**: UDS polling, API calls.
  - Flow: Poll → Cache (ResponseCache.swift) → Notify (NotificationManager.swift).
- **AuthenticationManager.swift**: Token handling.
  - Distinguish: Async ops vs. Caching.

### Views (UI)
- **StatusMenuView.swift** → Rows (StatusRow.swift), Problems (ProblemsView.swift).
  - Distinguish: Menu items vs. Subviews.

### Utils & Tests
- **StatusUtils.swift**: Formatting.
- Tests: StatusViewModelTests.swift (full coverage).

[source: menu-bar-app/Sources/AdapterOSMenu/AdapterOSMenuApp.swift L1-L20]
[source: menu-bar-app/Sources/AdapterOSMenu/Services/StatusReader.swift L1-L50]
[source: menu-bar-app/Sources/AdapterOSMenu/Views/StatusMenuView.swift L1-L100]




