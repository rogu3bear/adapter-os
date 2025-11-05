# AdapterOS Service Management Panel

A straightforward service management interface for AdapterOS that provides basic start/stop control and monitoring of core services. Runs on port 3300 with real-time status updates and terminal output display.

## ✅ **What Actually Works**

### **Core Functionality**
- **Service Start/Stop**: Working start and stop commands for configured services
- **Real-time Status**: Live status polling every 3 seconds
- **Terminal Output**: Scrolling log display for selected services
- **Service Grouping**: Core and monitoring services organized in columns
- **Session Tracking**: Basic start time and PID display
- **Error Handling**: Basic error display and API error handling

### **Supported Services**
| Service | Status | Port | Description |
|---------|--------|------|-------------|
| **Backend Server** | ✅ Working | 8080 | AdapterOS API server |
| **UI Frontend** | ✅ Working | 3200 | React dashboard |
| **Supervisor** | ✅ Working | - | Service orchestration |

### **API Endpoints**
```http
GET  /api/services          # List all services with status
POST /api/services/start    # Start a service
POST /api/services/stop     # Stop a service
GET  /api/services/{id}/logs # Get service logs
GET  /api/health            # System health check
```

## 🚀 **Usage & Operation**

### **Starting the Service Panel**

#### **Development Mode**
```bash
cd ui
pnpm service-panel:dev
```
Starts Vite dev server on port 3300.

#### **Production Mode**
```bash
cd ui
pnpm build:service-panel
pnpm service-panel
```
Builds and starts the production server on port 3300.

### **Interface Overview**

#### **Service Groups**
- **Core Services**: Backend server, UI frontend, supervisor
- **Monitoring Services**: Telemetry and metrics services
- **Status Indicators**: Running/stopped status with real-time updates

#### **Service Controls**
- **Start Button**: Green play button to start services
- **Stop Button**: Red square button to stop services
- **Terminal Output**: Click any service to view its logs
- **Refresh Button**: Manual status refresh

#### **Status Display**
- **Global Status**: Overall system health indicator
- **Service Counters**: Running/total services count
- **Session Info**: Start time and process ID for running services

## 🔧 **Implementation Details**

### **Backend Architecture**
- **Express Server**: Simple REST API on port 3300
- **Process Management**: Direct child_process spawning
- **Service Configuration**: Hardcoded service definitions
- **Log Collection**: Stdout/stderr capture with timestamps

### **Frontend Architecture**
- **React Components**: ServicePanel, ServiceCard, TerminalOutput
- **Real-time Updates**: Polling-based status updates
- **AdapterOS Styling**: Consistent with main UI design system
- **Simple State**: Local component state management

## 🛠️ **Extending the System**

### **Adding New Services**
1. Add service config to `server.js` serviceConfigs object
2. Define start/stop/status commands
3. Set appropriate category and port
4. Restart the service panel

### **Customization**
- **Commands**: Modify start/stop/status commands in serviceConfigs
- **Styling**: Update ServiceCard and ServicePanel components
- **Polling**: Change update intervals in ServicePanel useEffect
- **Categories**: Add new service categories with appropriate icons

## 🚨 **Troubleshooting**

### **Service Won't Start**
- Check if the service command exists and is executable
- Verify port availability if service uses a port
- Check service logs for error messages
- Ensure dependencies are running (if applicable)

### **Status Not Updating**
- Click the Refresh button to force update
- Check browser console for API errors
- Verify the service panel server is running on port 3300
- Check network connectivity to localhost:3300

### **Terminal Output Empty**
- Service must be selected (clicked) to show logs
- Logs only appear after service start/stop actions
- Check if the service process is actually running
- Some services may not produce immediate output

## 📝 **Development Notes**

### **Current Limitations**
- No automatic service restart on failure
- No dependency management between services
- No health checking beyond basic status
- No authentication or access control
- No persistent state across restarts
- No metrics or advanced monitoring

### **Future Enhancements**
- Service dependency resolution
- Health check integration
- Auto-restart functionality
- Better error handling and recovery
- Configuration file support
- User authentication

## 🎯 **Success Criteria Met**

✅ **Service Control**: Start/stop services with working commands
✅ **Status Display**: Real-time status updates and indicators
✅ **Terminal Output**: Scrolling log display for selected services
✅ **Service Grouping**: Logical organization of core/monitoring services
✅ **Session Tracking**: Start time and PID display
✅ **Error Handling**: Basic error display and API error handling
✅ **AdapterOS Styling**: Consistent with main UI design system
✅ **Port 3300**: Runs on requested port
✅ **Simple Operation**: Easy to use interface for basic service management

---

**Ready for use at `http://localhost:3300`** with working start/stop controls and terminal output display! 🎉

