# Service Management in AdapterOS Menu Bar

This document describes the service management functionality added to the AdapterOS menu bar application.

## Overview

The menu bar now includes service management capabilities that allow users to start and stop AdapterOS services directly from the menu bar, without needing to open the web interface.

## Features

### Service Control
- **Start/Stop Individual Services**: Control backend-server, ui-frontend, and other services
- **Bulk Operations**: Start or stop all essential services at once
- **Smart Dashboard Launch**: "Open Dashboard" automatically starts the UI if needed
- **Real-time Status**: See current service state with visual indicators
- **Operation Feedback**: Progress indicators and success/error messages

### Security
- **Authentication**: Uses shared secret authentication for localhost communication
- **Localhost Only**: All communication restricted to localhost
- **Minimal Permissions**: Only required entitlements enabled

### User Experience
- **Model Loading Status**: Shows loaded model name, memory usage, and loading status
- **Visual Status Indicators**: Color-coded icons and text for service states
- **Individual Service Control**: Each service has its own start/stop buttons and direct access links
- **Auto Dashboard Start**: Dashboard service automatically starts when menu bar app launches
- **Smart Dashboard Launch**: "Open Dashboard" button ensures UI service is running before opening
- **Service Direct Access**: Click service icons to open the correct service URLs
- **Progress Feedback**: Shows operation progress for long-running tasks
- **Error Recovery**: Graceful handling of connection failures with retry options
- **User-Friendly Alerts**: Clear error messages when service operations fail

## Architecture

### Components

#### 1. ServicePanelClient (`Services/ServicePanelClient.swift`)
- HTTP client for communicating with the service panel API
- Handles authentication, retries, and error recovery
- Provides async/await API for all service operations

#### 2. StatusViewModel Updates (`StatusViewModel.swift`)
- Added service state management
- Background polling of service status (15-second intervals)
- Operation state tracking and coordination
- Service control methods with proper error handling

#### 3. UI Components (`Views/StatusMenuView.swift`)
- Service management section in menu dropdown
- Individual service control rows
- Operation status indicators
- Error state displays

#### 4. Service Panel Updates (`../../ui/server.js`)
- Basic authentication middleware for service endpoints
- Shared secret validation
- Proper error responses

### Data Flow

```
Menu Bar UI → StatusViewModel → ServicePanelClient → Service Panel API → Services
    ↑                                                                       ↓
    └───────────────── Status Updates ←──────────────────────────────────────┘
```

## Configuration

### Environment Variables

#### Service Panel
```bash
# Shared secret for authentication (defaults to "adapteros-local-dev")
export SERVICE_PANEL_SECRET="your-secret-here"
```

#### Menu Bar App
```bash
# Use same secret as service panel
export SERVICE_PANEL_SECRET="your-secret-here"
```

### Ports
- **Service Panel**: `http://localhost:3301`
- **Backend Server**: `http://localhost:3300`
- **UI Frontend**: `http://localhost:3200`

## API Endpoints

### Health Check
```http
GET /api/health
```

### Service Management (Requires Authentication)
```http
POST /api/services/start
POST /api/services/stop
GET  /api/services
POST /api/services/status
POST /api/services/essential/start
POST /api/services/essential/stop
GET  /api/services/essential
```

### Authentication
```http
Authorization: Basic <base64-encoded-credentials>
# Where credentials are: service-panel:<shared-secret>
```

## Usage

### Starting the System

1. **Start Service Panel**:
   ```bash
   cd ui && SERVICE_PANEL_SECRET="your-secret" pnpm service-panel
   ```

2. **Start Menu Bar App**:
   ```bash
   cd menu-bar-app && SERVICE_PANEL_SECRET="your-secret" swift run
   ```

3. **Menu Bar Integration**:
   - Look for the menu bar icon (⚡)
   - **Model Status**: View loaded model name and memory usage at the top
   - **Auto Dashboard Start**: Dashboard service automatically starts when menu bar app launches
   - **Smart Dashboard Launch**: Click "Open Dashboard" - ensures UI service is running and opens browser
   - **Individual Service Control**: Each service shows status, port, and has start/stop buttons
   - **Direct Service Access**: Click the arrow icon next to running services to open them
   - Monitor individual service status and control operations

### Service States

| State | Icon | Description | Actions Available |
|-------|------|-------------|-------------------|
| Running | 🟢 | Service is active | Stop |
| Stopped | ⚪ | Service is not running | Start |
| Starting | 🟡 ▶️ | Service is starting up | None (wait) |
| Stopping | 🟡 ⏸️ | Service is shutting down | None (wait) |
| Error | 🔴 ⚠️ | Service failed | Start (retry) |

## Error Handling

### Connection Issues
- Service panel unreachable → Shows "Service Panel Offline"
- Retry button to attempt reconnection
- Automatic retry on next polling cycle

### Authentication Failures
- Wrong shared secret → Shows authentication error
- Check environment variables match between components

### Operation Failures
- Service start/stop failures → Shows error message with details
- Automatic cleanup of failed operation states
- Retry capability for failed operations

## Development

### Building
```bash
# Build menu bar app with service management
cd menu-bar-app
swift build -c release

# Build service panel with authentication
cd ../ui
SERVICE_PANEL_SECRET="dev-secret" pnpm build:service-panel
```

### Testing
```bash
# Run unit tests
cd menu-bar-app
swift test

# Integration testing
# 1. Start service panel
# 2. Start menu bar app
# 3. Test service operations via UI
```

### Debugging
```bash
# Enable verbose logging
export RUST_LOG=debug
export SERVICE_PANEL_LOG_LEVEL=debug

# Check service panel logs
tail -f ui/service-panel.log

# Check menu bar logs (if implemented)
log stream --predicate 'process == "aos-menu"' --style compact
```

## Security Considerations

### Threat Model
- **Localhost Only**: No remote access to service management
- **Shared Secret**: Simple authentication for local processes
- **No Persistent Storage**: Authentication tokens not stored
- **Minimal Permissions**: Only required macOS entitlements

### Best Practices
1. **Use strong shared secrets** in production
2. **Rotate secrets** periodically
3. **Monitor authentication failures** for security events
4. **Keep components updated** with latest security patches

## Future Enhancements

### Planned Features
- **Service Health Monitoring**: Detailed health checks per service
- **Service Logs**: View recent logs directly in menu bar
- **Service Configuration**: Basic config editing
- **Bulk Service Management**: Select multiple services for batch operations
- **Service Dependencies**: Visual dependency graph
- **Performance Metrics**: Per-service resource usage

### Technical Improvements
- **WebSocket Updates**: Real-time service status updates
- **Service Auto-restart**: Automatic restart on failure
- **Load Balancing**: Multi-instance service management
- **Remote Management**: Secure remote access (VPN required)
- **Audit Logging**: Comprehensive operation logging

## Troubleshooting

### Common Issues

**"Service Panel Offline"**
- Check if service panel is running on port 3301
- Verify shared secret matches between components
- Check firewall settings for localhost communication

**"Authentication Failed"**
- Verify `SERVICE_PANEL_SECRET` environment variable
- Ensure same secret used by both menu bar and service panel
- Check for special characters in secret

**"Service Operation Failed"**
- Check service panel logs for detailed error messages
- Verify service dependencies are satisfied
- Ensure sufficient system resources for service startup

### Logs and Diagnostics

```bash
# Service panel logs
tail -f ui/service-panel.log

# System logs for authentication issues
log show --predicate 'process == "AdapterOSMenu"' --last 1h

# Network connectivity test
curl -H "Authorization: Basic $(echo -n 'service-panel:your-secret' | base64)" \
     http://localhost:3301/api/health
```

## Migration Guide

### From Menu Bar v1.0 to v1.1

1. **Update Environment Variables**:
   ```bash
   export SERVICE_PANEL_SECRET="your-production-secret"
   ```

2. **Update Launch Agents**:
   ```xml
   <!-- Add to your launchd plist -->
   <key>EnvironmentVariables</key>
   <dict>
       <key>SERVICE_PANEL_SECRET</key>
       <string>your-production-secret</string>
   </dict>
   ```

3. **Update Service Panel Startup**:
   ```bash
   # Add to service panel startup script
   export SERVICE_PANEL_SECRET="your-production-secret"
   pnpm service-panel
   ```

4. **Verify Integration**:
   - Start both components
   - Check menu bar shows service management section
   - Test start/stop operations
   - Verify no authentication errors

This completes the service management implementation for the AdapterOS menu bar. The feature provides essential service control capabilities while maintaining security and usability standards.
