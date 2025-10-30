# UI Troubleshooting Guide

## Current Status

✅ **UI Dev Server**: Running on http://localhost:3200  
✅ **Backend API Server**: Running on http://localhost:8080  
✅ **TypeScript**: Configured to exclude test files  
✅ **Auth**: Login with `admin@example.com` / `password`

## If You See a White Page

### 1. Check Browser Console

Open Developer Tools (F12 or Cmd+Option+I) and check the Console tab for JavaScript errors.

Common errors and fixes:

**Error: "Cannot read property of undefined"**
- The API might not be responding
- Check if backend is running: `ps aux | grep adapteros-server`
- Test API: `curl http://localhost:8080/api/v1/adapters`

**Error: "Failed to fetch" or "Network Error"**
- API proxy configuration issue
- Backend not running on expected port (8080)
- CORS configuration problem

**Error: Module not found**
- Missing dependency: `cd ui && pnpm install`
- Stale node_modules: `cd ui && rm -rf node_modules && pnpm install`

### 2. Force Refresh

1. Clear browser cache: Cmd+Shift+R (Mac) or Ctrl+Shift+R (Windows/Linux)
2. Hard reload without cache
3. Try incognito/private mode

### 3. Check Network Tab

In Developer Tools, go to Network tab:
- Look for failed requests (red status codes)
- Check if `/api/*` requests are being proxied correctly
- Verify responses return data (not 404/500 errors)

### 4. Restart Services

```bash
# Stop everything
pkill -f adapteros-server
pkill -f vite

# Start backend
cd /Users/star/Dev/adapter-os
./target/debug/adapteros-server --skip-pf-check --config configs/cp.toml &

# Start frontend
cd ui
pnpm dev
```

### 5. Check Logs

**Vite Dev Server Output:**
- Should show "VITE ready in X ms"
- Should list local URL
- Look for compilation errors

**Backend Server Output:**
- Should show "Starting control plane on 127.0.0.1:8080"
- Should show "UI available at..."
- Check for database connection errors

## Development Mode Features

- **Login Required**: Use `admin@example.com` / `password` to access the system
- **Hot reload**: UI changes auto-refresh
- **API proxy**: `/api/*` requests automatically forwarded to backend

## Quick Health Check

Run this to verify everything is working:

```bash
# Check UI server
curl -I http://localhost:3200

# Check backend API
curl http://localhost:8080/api/v1/adapters

# Check if database exists
ls -lh var/aos-cp.sqlite3
```

## Still Having Issues?

1. Check terminal output for error messages
2. Verify ports 3200 and 8080 are not blocked by firewall
3. Make sure you're in the correct directory when running commands
4. Try building the UI: `cd ui && pnpm build` to see if there are TypeScript errors

## Configuration Files

- **UI Config**: `ui/vite.config.ts` - Dev server and proxy settings
- **Backend Config**: `configs/cp.toml` - Server port and database path  
- **TypeScript**: `ui/tsconfig.json` - Excludes test files from compilation
- **Package**: `ui/package.json` - Dependencies and scripts

## Useful Commands

```bash
# See what's running on port 3200
lsof -i :3200

# See what's running on port 8080
lsof -i :8080

# View real-time logs
tail -f server.log

# Check Node/pnpm versions
node --version
pnpm --version
```

## Known Issues

1. **Test files causing TypeScript errors**: Fixed by excluding `src/__tests__` in tsconfig
2. **Port 3200 in use**: Dev tooling now enforces port 3200. `pnpm dev` will terminate any lingering Vite instances on that port before starting. If you intentionally reserve the port for temporary test automation, launch that process with `AOS_PORT_3200_TAG=testing` set; the dev script will wait for it to exit instead of killing it. All other commands (including `pnpm build`) will forcibly reclaim the port, so testing processes must release it promptly.
3. **Single writer lock**: If backend won't start, use `--no-single-writer` flag

## Contact

If you continue to experience issues, check:
- GitHub issues
- Documentation in `/docs`
- Implementation plans in project root
