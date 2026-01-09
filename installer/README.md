# AdapterOS Installer

Native macOS graphical installer for AdapterOS using SwiftUI.

## Features

- **Hardware Pre-Checks**: Validates Apple Silicon (M1+), RAM (≥16GB), disk space, and macOS version
- **Installation Modes**: Full (with model download and tenant setup) or Minimal (binaries only)
- **Air-Gapped Support**: Skip all network operations for offline installations
- **Checkpoint Recovery**: Resume interrupted installations from where they left off
- **Progress Streaming**: Real-time log output and progress tracking
- **Determinism Education**: Post-install explainer about cryptographic verification

## Building

### Prerequisites

- Xcode 14.0 or later
- macOS 12.0 (Monterey) or later
- Apple Silicon Mac (for development)

### Build from Xcode

1. Open `AdapterOSInstaller.xcodeproj` in Xcode
2. Select your development team in project settings
3. Build and run (⌘R)

### Build from Command Line

```bash
cd installer
xcodebuild -project AdapterOSInstaller.xcodeproj -scheme AdapterOSInstaller -configuration Release
```

The built app will be in `build/Release/AdapterOS Installer.app`

### Build from Project Root

```bash
cd installer && xcodebuild -project AdapterOSInstaller.xcodeproj -scheme AdapterOSInstaller -configuration Release
```

## Architecture

The installer is a thin SwiftUI wrapper around the existing CLI infrastructure:

```
┌─────────────────────┐
│   SwiftUI Views     │  PreCheckView, InstallView, CompletionView
├─────────────────────┤
│   ProcessRunner     │  Async process execution, log streaming
├─────────────────────┤
│ HardwareChecker     │  System validation (M1+, RAM, disk)
├─────────────────────┤
│ CheckpointManager   │  Resume detection and state management
└─────────────────────┘
           │
           ▼
┌─────────────────────┐
│  bootstrap script   │  scripts/bootstrap_with_checkpoints.sh
└─────────────────────┘
           │
           ▼
┌─────────────────────┐
│   aosctl CLI        │  Rust binaries and build system
└─────────────────────┘
```

## Files

- `AdapterOSInstallerApp.swift` - App entry point
- `ContentView.swift` - Main navigation
- `PreCheckView.swift` - Hardware validation screen
- `InstallView.swift` - Installation progress screen
- `CompletionView.swift` - Success screen with determinism explainer
- `Models.swift` - Data structures (InstallStep, InstallMode, etc.)
- `HardwareChecker.swift` - System requirements validation
- `ProcessRunner.swift` - Process execution and log streaming
- `CheckpointManager.swift` - Resume logic
- `DeterminismExplainer.swift` - Educational content

## Installation Flow

1. **Pre-Check**: Validate hardware and configure options
2. **Install**: Run bootstrap script with checkpoint recovery
3. **Smoke Test**: Verify core functionality (init, serve, inference)
4. **Complete**: Show determinism explainer and next steps

## Checkpoint Recovery

If installation is interrupted, the installer automatically detects the checkpoint file at `./var/adapteros_install.state` and resumes from the last completed step:

- `create_dirs` - Directory creation
- `build_binaries` - Rust compilation (longest step)
- `init_db` - Database initialization
- `build_metal` - Metal kernel compilation
- `download_model` - Model download (skipped in air-gapped mode)
- `create_tenant` - Default tenant setup (full mode only)
- `smoke_test` - Post-install verification tests

## Code Signing

For distribution, sign the app with your Developer ID:

```bash
codesign --deep --force --verify --verbose --sign "Developer ID Application: Your Name" "AdapterOS Installer.app"
```

For notarization (required for distribution outside the App Store):

```bash
xcrun notarytool submit "AdapterOS Installer.app" --apple-id your@email.com --team-id TEAMID --wait
xcrun stapler staple "AdapterOS Installer.app"
```

## Smoke Test

The installer includes a comprehensive post-install smoke test (`installer/smoke_test.sh`) that verifies:

- **Binary Availability**: `aosctl` binary exists and responds to help
- **Tenant Management**: Tenant initialization (if database available)
- **Serve Functionality**: Dry-run serve command validation
- **Inference Examples**: Basic inference example compilation
- **API Client**: Client library availability and compilation
- **Metal Kernels**: Precompiled kernel verification
- **Configuration**: Config files and manifests presence
- **Database**: Migration files availability
- **Policy System**: Policy management functionality

The smoke test runs automatically after installation and provides detailed feedback on system readiness.

## Future Enhancements

- [ ] DMG creation for drag-and-drop installation
- [ ] Automatic update checking
- [ ] Custom model selection
- [ ] Network connectivity detection
- [ ] Installation location customization
- [ ] Uninstaller

## License

MIT OR Apache-2.0 (matches parent project)
