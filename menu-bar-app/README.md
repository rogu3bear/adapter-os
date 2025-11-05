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

Lightweight macOS menu bar application that displays AdapterOS status by reading JSON written by the control plane.

## Architecture

- **No network calls**: Reads local JSON file at `/var/run/adapteros_status.json` (or `var/adapteros_status.json`)
- **Native system metrics**: Uses IOKit and ProcessInfo for CPU/GPU/RAM
- **5-second polling**: Updates status every 5 seconds
- **Zero egress**: Completely offline, no telemetry

## Building

```bash
# Build debug version
swift build

# Build release version
swift build -c release

# Run directly
swift run

# Or run the built executable
.build/release/AdapterOSMenu
```

## Installation

Copy the built executable to a system location:

```bash
# Build release
swift build -c release

# Copy to local bin
cp .build/release/AdapterOSMenu /usr/local/bin/aos-menu

# Run
/usr/local/bin/aos-menu
```

## Optional: Auto-start with launchd

Create `~/Library/LaunchAgents/com.adapteros.menu.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.adapteros.menu</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/aos-menu</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load with:
```bash
launchctl load ~/Library/LaunchAgents/com.adapteros.menu.plist
```

## Status Display

**Menu bar icon**:
- `⚡︎` - Normal operation, deterministic mode
- `⚡︎/` - Non-deterministic mode
- `🔥` - High CPU load (>70%)

**Tooltip**:
```
AdapterOS OK · 45% CPU · 62% GPU · 18GB RAM
```

**Dropdown menu**:
```
Adapters: 3
Workers: 2
CPU: 45% | GPU: 62% | RAM: 18 GB
Deterministic: ✅
Uptime: 3h 42m
───────────
View Logs
```

## Development

The app reads from two sources:

1. **AdapterOS status**: `/var/run/adapteros_status.json` (written by mplora-server)
2. **System metrics**: Native macOS APIs (ProcessInfo, IOKit)

If the JSON file doesn't exist, the app displays "AdapterOS OFFLINE".

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




