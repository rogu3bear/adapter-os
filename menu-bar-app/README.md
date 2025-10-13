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

## Code Signing

For distribution, sign with your Developer ID:

```bash
codesign --sign "Developer ID Application: Your Name" .build/release/AdapterOSMenu
```

## License

Dual-licensed under Apache 2.0 or MIT.




