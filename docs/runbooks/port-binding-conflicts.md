# Port Binding Conflicts

Port 8080 in use, PID lock issues, and socket binding failures.

## Symptoms

- "Address already in use" errors
- "Another aos-cp process is running" errors
- Server fails to bind to port 8080
- Socket permission denied errors
- Stale PID lock files

## Common Failure Modes

### 1. Port 8080 Already in Use

**Symptoms:**
```
[ERROR] Failed to bind to 127.0.0.1:8080
[ERROR] Address already in use (os error 48)
```

**Root Cause:**
- Another AdapterOS instance running
- Different service using port 8080
- Zombie process holding port

**Diagnostic Commands:**
```bash
# Find what's using port 8080
lsof -i :8080

# Alternative: netstat
netstat -an | grep 8080

# Check for aos-cp processes
ps aux | grep aos-cp

# Check PID lock file
cat var/aos-cp.pid 2>/dev/null
ps -p $(cat var/aos-cp.pid 2>/dev/null) 2>/dev/null
```

**Fix Procedure:**

**Option A: Stop Existing Process**
```bash
# Step 1: Find the process ID
PID=$(lsof -t -i :8080)

# Step 2: Check what it is
ps -p $PID

# Step 3: Stop gracefully
kill -SIGTERM $PID

# Step 4: Wait 5 seconds
sleep 5

# Step 5: Force kill if still running
kill -9 $PID 2>/dev/null

# Step 6: Verify port is free
lsof -i :8080
# Should return nothing

# Step 7: Remove stale PID lock
rm -f var/aos-cp.pid
```

**Option B: Change Port**
```bash
# Edit configuration
vim configs/cp.toml

# Change port number
[server]
port = 8081  # or any available port

# Restart server
cargo run --bin aos-cp -- --config configs/cp.toml

# Update health checks
curl http://localhost:8081/healthz/all
```

**Option C: Stop Other Service**
```bash
# If another service is using 8080
# Example: stopping a development server

# Find the service
lsof -i :8080

# Stop it (depends on service)
# Examples:
pkill -f "python.*8080"
pkill -f "node.*8080"
launchctl stop com.example.service
```

**Prevention:**
- Use PID lock file (enabled by default)
- Always stop server gracefully with SIGTERM
- Check port availability before starting
- Document which services use which ports

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:947-961` - Port binding
- `/Users/star/Dev/aos/configs/cp.toml` - Port configuration

### 2. Stale PID Lock File

**Symptoms:**
```
[ERROR] Another aos-cp process is running (PID: 12345). Stop it first or use --no-single-writer.
```

**Root Cause:**
- Server crashed without cleanup
- Server killed with SIGKILL (-9)
- PID file not removed on shutdown
- System restart without cleanup

**Diagnostic Commands:**
```bash
# Check if PID file exists
ls -la var/aos-cp.pid /var/run/aos/cp.pid 2>/dev/null

# Read PID from file
cat var/aos-cp.pid 2>/dev/null

# Check if process with that PID exists
PID=$(cat var/aos-cp.pid 2>/dev/null)
ps -p $PID 2>/dev/null

# Alternative: check process by name
ps aux | grep aos-cp
```

**Fix Procedure:**

**Step 1: Verify Process is Not Running**
```bash
# Get PID from lock file
PID=$(cat var/aos-cp.pid 2>/dev/null)

# Check if it's running
if ps -p $PID > /dev/null 2>&1; then
  echo "Process $PID is running"
  # Follow "Option A: Stop Existing Process" above
else
  echo "Process $PID is not running (stale lock)"
fi
```

**Step 2: Remove Stale Lock**
```bash
# Remove local lock file
rm -f var/aos-cp.pid

# Remove system lock file (if exists)
sudo rm -f /var/run/aos/cp.pid

# Verify removed
ls -la var/aos-cp.pid /var/run/aos/cp.pid 2>/dev/null
```

**Step 3: Restart Server**
```bash
cargo run --bin aos-cp -- --config configs/cp.toml

# Or use script
./scripts/start_server.sh
```

**Prevention:**
- Always stop with SIGTERM, never SIGKILL (-9)
- Use proper shutdown handlers
- Implement cleanup in systemd/launchd units
- Monitor for zombie processes

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:38-83` - PID lock implementation
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:79-83` - Lock cleanup on drop

### 3. Permission Denied on Socket Creation

**Symptoms:**
```
[ERROR] Failed to create socket: Permission denied
[ERROR] Cannot write to /var/run/aos/aos.sock
```

**Root Cause:**
- Insufficient permissions for socket directory
- SELinux/AppArmor policy blocking
- Directory doesn't exist
- Ownership mismatch

**Diagnostic Commands:**
```bash
# Check directory permissions
ls -la /var/run/aos/

# Check if directory exists
test -d /var/run/aos && echo "exists" || echo "missing"

# Check ownership
stat /var/run/aos/

# Check current user
id
```

**Fix Procedure:**

**Step 1: Create Directory with Correct Permissions**
```bash
# Create directory
sudo mkdir -p /var/run/aos

# Set permissions (775 = rwxrwxr-x)
sudo chmod 775 /var/run/aos

# Set ownership
sudo chown $(whoami):staff /var/run/aos

# Verify
ls -la /var/run/aos/
```

**Step 2: Alternative - Use Local Socket**
```bash
# Create local socket directory
mkdir -p var/run

# Start server with local socket
aosctl serve --socket var/run/aos.sock \
  --tenant default --plan cp_abc123
```

**Step 3: Fix Existing Socket Permissions**
```bash
# Remove stale socket
rm -f /var/run/aos/aos.sock

# Set correct permissions
chmod 660 /var/run/aos/aos.sock
```

**Prevention:**
- Create directories during installation
- Set correct ownership and permissions
- Use local paths in development
- Document permission requirements

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-cli/src/main.rs:721` - Socket path configuration

### 4. Multiple Control Plane Instances

**Symptoms:**
```
[WARN] Multiple aos-cp processes detected
[ERROR] Database locked by another instance
[ERROR] PID lock acquisition failed
```

**Root Cause:**
- Started server multiple times accidentally
- Automated restart scripts misbehaving
- PID lock disabled with --no-single-writer
- Different instances on different ports

**Diagnostic Commands:**
```bash
# Find all aos-cp processes
ps aux | grep aos-cp | grep -v grep

# Count instances
ps aux | grep aos-cp | grep -v grep | wc -l

# Show with details
ps aux | grep aos-cp | grep -v grep | awk '{print $2, $11, $12, $13}'

# Check what ports they're using
lsof -c aos-cp | grep LISTEN
```

**Fix Procedure:**

**Step 1: Identify All Instances**
```bash
# List all with PIDs
ps aux | grep aos-cp | grep -v grep | awk '{print "PID:", $2, "CMD:", $11, $12, $13}'

# Check configs/ports
lsof -c aos-cp | grep LISTEN
```

**Step 2: Stop All Instances**
```bash
# Graceful stop
pkill -SIGTERM aos-cp

# Wait 10 seconds
sleep 10

# Force kill any remaining
pkill -9 aos-cp

# Verify all stopped
ps aux | grep aos-cp | grep -v grep
# Should return nothing
```

**Step 3: Clean Up Lock Files**
```bash
# Remove all PID locks
rm -f var/aos-cp.pid
sudo rm -f /var/run/aos/cp.pid

# Remove sockets
rm -f var/run/*.sock
sudo rm -f /var/run/aos/*.sock
```

**Step 4: Restart Single Instance**
```bash
# Start with single-writer enabled (default)
cargo run --bin aos-cp -- --config configs/cp.toml

# Verify only one instance
ps aux | grep aos-cp | grep -v grep | wc -l
# Should output: 1
```

**Prevention:**
- Always use single-writer mode (default)
- Never use --no-single-writer in production
- Implement proper process supervision
- Use systemd/launchd for management

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:198-201` - single-writer flag
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:244-249` - PID lock acquisition

## Port Configuration Reference

**Default Ports:**
```
8080  - Control Plane HTTP API/UI
8081  - Alternative port (if 8080 in use)
```

**Socket Paths:**
```
/var/run/aos/aos.sock      - Production UDS socket
var/run/aos.sock           - Development UDS socket
```

**Configuration File:**
```toml
# configs/cp.toml
[server]
port = 8080
host = "127.0.0.1"
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:948-953` - Port binding code
- `/Users/star/Dev/aos/scripts/start_server.sh:100-111` - Startup script port handling

## Quick Commands

```bash
# Check what's using port 8080
lsof -i :8080

# Stop all aos-cp processes
pkill -SIGTERM aos-cp

# Remove all lock files
rm -f var/aos-cp.pid /var/run/aos/cp.pid

# Verify port is free
lsof -i :8080 | wc -l
# Should output: 0

# Start server
./scripts/start_server.sh
```

## Related Runbooks

- [Startup Failures](./startup-failures.md)
- [Startup Procedures](./startup-procedures.md)
- [Cleanup Procedures](./cleanup-procedures.md)

## Escalation Criteria

Escalate if:
- Port conflicts persist after following procedures
- Permission issues cannot be resolved
- Multiple mysterious instances appearing
- System-level port binding failures
- See [Escalation Guide](./escalation.md)
