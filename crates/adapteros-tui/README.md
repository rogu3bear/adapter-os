# adapterOS TUI Control System

A comprehensive Terminal User Interface for controlling and monitoring adapterOS services.

## Features

- **Live Status Bar**: Real-time display of model status, memory usage, and system mode
- **Service Management**: Start/stop/restart individual services or all at once
- **Metrics Dashboard**: Monitor performance metrics, resource usage, and component health
- **Log Viewer**: Filter and search through system logs with color-coded severity levels
- **Configuration Editor**: Modify system settings with live validation
- **adapterOS ASCII Branding**: Beautiful terminal interface with proper "adapterOS" branding

## Running the TUI

```bash
# From the project root
cargo run -p adapteros-tui

# Or from within the TUI crate directory
cd crates/adapteros-tui
cargo run
```

## Keyboard Controls

### Navigation
- `↑/↓` - Navigate menu items
- `←/→` - Switch between screens
- `Tab` - Next screen
- `Shift+Tab` - Previous screen
- `Enter` - Select/activate
- `Esc` - Go back/cancel

### Quick Keys
- `b` - Boot all services
- `s` - Services screen
- `l` - Logs screen
- `m` - Metrics screen
- `c` - Config screen
- `d` - Dashboard
- `p` - Toggle production mode
- `h` - Toggle help
- `q` - Quit
- `Ctrl+C` - Force quit

## Screens

1. **Dashboard**: Main welcome screen with ASCII art and system overview
2. **Services**: Service control panel with status and actions
3. **Logs**: Real-time log viewer with filtering
4. **Metrics**: Performance metrics and resource monitoring
5. **Config**: System configuration editor
6. **Help**: Keyboard shortcuts and usage information

## Visual Design

- **Green** `[OK]` - Running/Success/Healthy
- **Yellow** `[!!]` - Warning/Starting/Degraded
- **Red** `[XX]` - Error/Stopped/Failed
- **Gray** `[--]` - Disabled/Inactive

Progress bars use box-drawing characters without emojis:
```
████████████░░░░░░░░ 60%
```

## Integration

The TUI integrates with adapterOS services via:
- REST API on port 3300 (when server is running)
- Metrics endpoint on port 9090
- Direct database access (when configured)

## Development

To test the TUI without running the full adapterOS server:
```bash
cargo run -p adapteros-tui
```

The TUI will run with mock data when services are unavailable.