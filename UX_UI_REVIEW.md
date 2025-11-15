# AdapterOS UX/UI Review

**Date:** 2025-01-15  
**Reviewer:** AI Assistant  
**Focus:** Service Failure Visibility & Overall UX Patterns

---

## Executive Summary

The AdapterOS UI is a well-structured React application using modern patterns (shadcn/ui, Tailwind CSS, React Router). However, **service launch failures are not prominently displayed** in the web UI, which creates a gap compared to the menu bar implementation we just completed.

---

## Current UI Architecture

### ✅ **Strengths**

1. **Component Organization**
   - Clean separation: `components/`, `pages/`, `hooks/`, `services/`
   - Reusable UI components in `components/ui/`
   - Role-based dashboard layouts (`Dashboard.tsx`)

2. **State Management**
   - Standardized polling hook (`usePolling.ts`)
   - Context providers for auth, tenant, density
   - Proper error handling with retry logic

3. **Accessibility**
   - ARIA labels and live regions
   - Keyboard shortcuts support
   - Screen reader announcements

4. **Design System**
   - Consistent use of shadcn/ui components
   - Tailwind CSS for styling
   - Responsive grid layouts

### ⚠️ **Gaps Identified**

1. **Service Failure Visibility**
   - Service status exists in `ServicePanel.tsx` but not integrated into main dashboard
   - No prominent alerts for failed services
   - Service failures don't appear in `ActiveAlertsWidget`

2. **Status Information Flow**
   - Dashboard widgets don't consume service status from `/var/run/adapteros_status.json`
   - `SystemHealthWidget` shows metrics but not service failures
   - Missing connection between supervisor API and dashboard

---

## Current Service Status Implementation

### **ServicePanel Component** (`ui/src/components/ServicePanel.tsx`)

**Location:** Standalone component (not integrated into dashboard)

**Features:**
- ✅ Fetches services from `/api/services`
- ✅ Shows service status badges (running/stopped/error)
- ✅ Calculates global status (healthy/warning/error)
- ✅ Supports start/stop/restart actions
- ✅ Polls every 3 seconds

**Limitations:**
- ❌ Not visible on main dashboard
- ❌ No alert/notification when services fail
- ❌ Error messages aren't prominently displayed
- ❌ Doesn't use `/var/run/adapteros_status.json` data

### **Dashboard Widgets**

**Current Widgets:**
1. `SystemHealthWidget` - Memory, sessions, latency
2. `ActiveAlertsWidget` - Policy violations, system alerts
3. `MultiModelStatusWidget` - Model loading status
4. `BaseModelWidget` - Base model status
5. `ComplianceScoreWidget` - Policy compliance
6. `AdapterStatusWidget` - Adapter registry status

**Missing:** Service Status Widget

---

## Recommendations

### 🎯 **Priority 1: Integrate Service Status into Dashboard**

#### **Option A: Add Service Status Widget to Dashboard**

Create `ServiceStatusWidget.tsx` similar to other dashboard widgets:

```typescript
// ui/src/components/dashboard/ServiceStatusWidget.tsx
export function ServiceStatusWidget() {
  const { data: status } = usePolling(
    () => apiClient.getStatus(), // Reads from /var/run/adapteros_status.json
    'fast',
    { showLoadingIndicator: false }
  );

  const failedServices = status?.services?.filter(s => s.state === 'failed') || [];
  const hasFailures = failedServices.length > 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          {hasFailures ? (
            <AlertTriangle className="h-5 w-5 text-destructive" />
          ) : (
            <Server className="h-5 w-5" />
          )}
          Services
        </CardTitle>
      </CardHeader>
      <CardContent>
        {hasFailures ? (
          <div className="space-y-2">
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Service Launch Failures</AlertTitle>
              <AlertDescription>
                {failedServices.length} service{failedServices.length > 1 ? 's' : ''} failed to launch
              </AlertDescription>
            </Alert>
            <div className="space-y-1">
              {failedServices.map(service => (
                <div key={service.id} className="text-sm">
                  <span className="font-medium">{service.name}</span>
                  {service.last_error && (
                    <span className="text-muted-foreground ml-2">
                      - {service.last_error}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="text-sm text-muted-foreground">
            All services running ({status?.services?.length || 0} total)
          </div>
        )}
      </CardContent>
    </Card>
  );
}
```

**Add to Dashboard Layouts:**
- **Admin:** Priority 1 (highest)
- **SRE:** Priority 2
- **Operator:** Priority 3

#### **Option B: Enhance ActiveAlertsWidget**

Include service failures in the alerts system:

```typescript
// In ActiveAlertsWidget.tsx
const serviceAlerts = useMemo(() => {
  if (!status?.hasServiceFailures) return [];
  
  return status.failedServices.map(service => ({
    id: `service-${service.id}`,
    severity: 'critical' as const,
    title: `Service Failed: ${service.name}`,
    message: service.last_error || 'Service failed to launch',
    timestamp: new Date(),
    acknowledged: false,
    source: 'service-supervisor'
  }));
}, [status]);
```

### 🎯 **Priority 2: API Integration**

#### **Add Status Endpoint to API Client**

```typescript
// ui/src/api/client.ts
async getStatus(): Promise<AdapterOSStatus> {
  return this.get<AdapterOSStatus>('/v1/status');
}
```

#### **Backend Endpoint**

The backend should expose the status file data:

```rust
// crates/adapteros-server-api/src/handlers.rs
#[utoipa::path(
    get,
    path = "/v1/status",
    tag = "status",
    responses(
        (status = 200, description = "Current system status", body = AdapterOSStatus)
    )
)]
pub async fn get_status(State(state): State<AppState>) -> Result<Json<AdapterOSStatus>> {
    let status = adapteros_server::status_writer::get_cached_status()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Status not available".to_string()))?;
    Ok(Json(status))
}
```

### 🎯 **Priority 3: Visual Enhancements**

#### **Badge Colors**
- ✅ Running: Green (`text-green-600`)
- ⚠️ Starting/Stopping: Yellow (`text-yellow-600`)
- ❌ Failed: Red (`text-red-600`)
- ⏸️ Stopped: Gray (`text-gray-600`)

#### **Icons**
- Running: `CheckCircle`
- Failed: `AlertTriangle` or `XCircle`
- Starting: `RefreshCw` (animated)
- Stopped: `Square`

#### **Toast Notifications**

Show toast when services fail:

```typescript
useEffect(() => {
  if (status?.hasServiceFailures) {
    toast.error(
      `${status.failedServices.length} service(s) failed to launch`,
      {
        duration: 5000,
        action: {
          label: 'View Details',
          onClick: () => navigate('/admin/services')
        }
      }
    );
  }
}, [status?.hasServiceFailures]);
```

---

## Comparison: Menu Bar vs Web UI

| Feature | Menu Bar ✅ | Web UI ❌ |
|---------|------------|-----------|
| Service failure detection | ✅ Reads from status file | ❌ Not implemented |
| Visual indicator | ✅ Icon changes | ❌ No indicator |
| Prominent display | ✅ Problems banner | ❌ Not visible |
| Error messages | ✅ Shows in tooltip | ❌ Not shown |
| Service list | ✅ Dedicated section | ⚠️ Only in ServicePanel |

---

## Implementation Plan

### **Phase 1: Quick Win (1-2 hours)**
1. Add `ServiceStatusWidget` to dashboard
2. Read from `/var/run/adapteros_status.json` via API
3. Display failed services prominently

### **Phase 2: Integration (2-3 hours)**
1. Add service failures to `ActiveAlertsWidget`
2. Create toast notifications for failures
3. Add service status to `SystemHealthWidget`

### **Phase 3: Polish (1-2 hours)**
1. Add service details page (`/admin/services`)
2. Implement restart actions from dashboard
3. Add service health history/graphs

---

## Code Examples

### **Status Type Definition**

```typescript
// ui/src/api/types.ts
export interface ServiceStatus {
  id: string;
  name: string;
  state: 'stopped' | 'starting' | 'running' | 'stopping' | 'failed' | 'restarting';
  pid?: number;
  port?: number;
  health_status: 'unknown' | 'healthy' | 'unhealthy' | 'checking';
  restart_count: number;
  last_error?: string;
}

export interface AdapterOSStatus {
  schema_version?: string;
  status: 'ok' | 'degraded' | 'error';
  uptime_secs: number;
  adapters_loaded: number;
  deterministic: boolean;
  kernel_hash: string;
  telemetry_mode: string;
  worker_count: number;
  base_model_loaded?: boolean;
  base_model_id?: string;
  base_model_name?: string;
  base_model_status?: string;
  base_model_memory_mb?: number;
  services?: ServiceStatus[]; // NEW
}
```

### **Dashboard Integration**

```typescript
// ui/src/components/Dashboard.tsx
const dashboardLayouts: Record<UserRole, DashboardLayout> = {
  admin: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 }, // NEW
      { id: 'multi-model-status', component: MultiModelStatusWidget, priority: 2 },
      // ... rest
    ],
    // ...
  },
  sre: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 }, // NEW
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 2 },
      // ... rest
    ],
    // ...
  },
  // ...
};
```

---

## UX Best Practices Observed

### ✅ **Good Patterns**
- Consistent use of Card components
- Loading states with skeletons
- Error boundaries
- Retry logic for failed requests
- Polling for real-time updates
- Accessibility attributes

### ⚠️ **Areas for Improvement**
- Service failures should be more prominent
- Error messages need better visibility
- Service control actions should be accessible from dashboard
- Status file should be primary source of truth

---

## Testing Recommendations

1. **Unit Tests**
   - Test `ServiceStatusWidget` rendering
   - Test failed service detection logic
   - Test alert generation from service failures

2. **Integration Tests**
   - Test API endpoint `/v1/status`
   - Test service failure propagation to dashboard
   - Test toast notifications

3. **E2E Tests**
   - Simulate service failure
   - Verify dashboard shows failure
   - Verify alert appears
   - Verify menu bar also shows failure

---

## Conclusion

The AdapterOS UI has a solid foundation but **lacks integration of service failure visibility**. The menu bar implementation we completed provides a good reference for how service failures should be displayed in the web UI.

**Recommended Next Steps:**
1. Implement `ServiceStatusWidget` (Priority 1)
2. Add service failures to alerts system (Priority 2)
3. Create service management page (Priority 3)

This will ensure users can see service failures immediately when they open the dashboard, matching the visibility we achieved in the menu bar app.

