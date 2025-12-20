const express = require('express');
const { spawn, exec } = require('child_process');
const path = require('path');
const fs = require('fs');
const cors = require('cors');

const app = express();
// Port configuration - respects environment variables for multi-developer setups
const PORT = parseInt(process.env.AOS_PANEL_PORT || '3301', 10);
const BACKEND_PORT = process.env.AOS_SERVER_PORT || '8080';
const UI_PORT = process.env.AOS_UI_PORT || '3200';
const API_PROXY_TARGET = process.env.API_PROXY_TARGET || `http://localhost:${BACKEND_PORT}`;

// Compute project root as parent of ui/ directory
// This allows the server to work from any location where the project is cloned
const UI_DIR = __dirname;
const PROJECT_ROOT = path.resolve(__dirname, '..');

// Simple shared secret authentication for localhost communication
const SHARED_SECRET = process.env.SERVICE_PANEL_SECRET || 'adapteros-local-dev';
const AUTH_TOKEN = Buffer.from(`service-panel:${SHARED_SECRET}`).toString('base64');

// Middleware
app.use(cors());
app.use(
  express.json({
    limit: '10mb',
    verify: (req, _res, buf) => {
      req.rawBody = buf ? Buffer.from(buf) : Buffer.alloc(0);
    }
  })
);
app.use(
  express.urlencoded({
    extended: true,
    limit: '10mb',
    verify: (req, _res, buf) => {
      req.rawBody = buf ? Buffer.from(buf) : Buffer.alloc(0);
    }
  })
);

// Authentication middleware for service management endpoints
app.use('/api/services', (req, res, next) => {
  const authHeader = req.headers.authorization;

  if (!authHeader || !authHeader.startsWith('Basic ')) {
    return res.status(401).json({
      error: 'Authentication required',
      message: 'Basic authentication required for service management'
    });
  }

  const token = authHeader.substring(6); // Remove 'Basic '
  if (token !== AUTH_TOKEN) {
    return res.status(403).json({
      error: 'Authentication failed',
      message: 'Invalid authentication token'
    });
  }

  next();
});

app.use(express.static(path.join(__dirname, 'dist-service-panel')));

// Store running processes
const runningProcesses = new Map();

// Service configurations - what actually works
// Uses PROJECT_ROOT and UI_DIR for portable paths that work from any clone location
const serviceConfigs = {
  'service-panel': {
    name: 'Service Panel',
    startCommand: `cd "${UI_DIR}" && SERVICE_PANEL_SECRET=adapteros-local-dev pnpm service-panel`,
    stopCommand: 'pkill -f "node server.js"',
    statusCommand: 'pgrep -f "node server.js" >/dev/null && echo "running" || echo "stopped"',
    healthCommand: `curl -f http://localhost:${PORT}/health >/dev/null 2>&1 && echo "healthy" || echo "unhealthy"`,
    port: PORT,
    category: 'management',
    essential: true,
    dependencies: [],
    startupOrder: 0
  },
  'backend-server': {
    name: 'Backend Server',
    startCommand: `cd "${PROJECT_ROOT}" && cargo run -p adapteros-server-api --release 2>&1 | head -100`,
    stopCommand: 'pkill -f "adapteros-server-api"',
    statusCommand: 'pgrep -f "adapteros-server-api" >/dev/null && echo "running" || echo "stopped"',
    healthCommand: `curl -f http://localhost:${BACKEND_PORT}/healthz >/dev/null 2>&1 && echo "healthy" || echo "unhealthy"`,
    port: parseInt(BACKEND_PORT, 10),
    category: 'core',
    essential: true,
    dependencies: [],
    startupOrder: 1
  },
  'ui-frontend': {
    name: 'UI Frontend',
    startCommand: `cd "${UI_DIR}" && pnpm dev -- --host 0.0.0.0 --port ${UI_PORT}`,
    stopCommand: `pkill -f "vite.*${UI_PORT}"`,
    statusCommand: `pgrep -f "vite.*${UI_PORT}" >/dev/null && echo "running" || echo "stopped"`,
    healthCommand: `curl -f http://localhost:${UI_PORT} >/dev/null 2>&1 && echo "healthy" || echo "unhealthy"`,
    port: parseInt(UI_PORT, 10),
    category: 'core',
    essential: false,
    dependencies: [],
    startupOrder: 2
  },
  'supervisor': {
    name: 'Supervisor',
    startCommand: `cd "${PROJECT_ROOT}" && aos-supervisor --config scripts/supervisor.toml`,
    stopCommand: 'pkill -f "aos-supervisor"',
    statusCommand: 'pgrep -f "aos-supervisor" >/dev/null && echo "running" || echo "stopped"',
    healthCommand: 'test -S /var/run/aos/supervisor.sock >/dev/null 2>&1 && echo "healthy" || echo "unhealthy"',
    category: 'core',
    essential: false,
    dependencies: ['backend-server'],
    startupOrder: 3
  }
};

// Utility function to execute commands
function executeCommand(command, cwd = process.cwd()) {
  return new Promise((resolve, reject) => {
    exec(command, { cwd, maxBuffer: 1024 * 1024 * 10 }, (error, stdout, stderr) => {
      if (error) {
        reject({ error: error.message, stdout, stderr });
      } else {
        resolve({ stdout, stderr });
      }
    });
  });
}

// Start service endpoint
app.post('/api/services/start', async (req, res) => {
  try {
    const { serviceId } = req.body;

    if (!serviceId) {
      return res.status(400).json({ error: 'serviceId is required' });
    }

    const config = serviceConfigs[serviceId];
    if (!config) {
      return res.status(404).json({ error: `Service ${serviceId} not configured` });
    }

    // Check if service is already running
    if (runningProcesses.has(serviceId)) {
      return res.status(409).json({ error: 'Service is already running' });
    }

    console.log(`Starting service ${serviceId}`);

    // Start the service process
    const child = spawn(config.startCommand, [], {
      shell: true,
      cwd: process.cwd(),
      detached: true,
      stdio: ['ignore', 'pipe', 'pipe']
    });

    const processInfo = {
      process: child,
      startTime: new Date(),
      config,
      logs: [`[${new Date().toISOString()}] Service ${serviceId} starting...`]
    };

    runningProcesses.set(serviceId, processInfo);

    // Collect stdout and stderr
    child.stdout.on('data', (data) => {
      const output = data.toString().trim();
      if (output) {
        const logEntry = `[${new Date().toISOString()}] ${output}`;
        processInfo.logs.push(logEntry);
        console.log(`[${serviceId}] ${output}`);
      }
    });

    child.stderr.on('data', (data) => {
      const output = data.toString().trim();
      if (output) {
        const logEntry = `[${new Date().toISOString()}] ERROR: ${output}`;
        processInfo.logs.push(logEntry);
        console.error(`[${serviceId}] ${output}`);
      }
    });

    child.on('close', (code) => {
      const endTime = new Date();
      processInfo.endTime = endTime;
      processInfo.exitCode = code;
      const logEntry = `[${endTime.toISOString()}] Process exited with code ${code}`;
      processInfo.logs.push(logEntry);
      console.log(`Service ${serviceId} exited with code ${code}`);
    });

    child.on('error', (error) => {
      const logEntry = `[${new Date().toISOString()}] Process error: ${error.message}`;
      processInfo.logs.push(logEntry);
      console.error(`Service ${serviceId} process error:`, error);
    });

    // Unref to allow parent to exit independently
    child.unref();

    res.json({
      success: true,
      pid: child.pid,
      message: `Service ${serviceId} started`
    });

  } catch (error) {
    console.error('Error starting service:', error);
    res.status(500).json({
      error: error.message || 'Failed to start service'
    });
  }
});

// Stop service endpoint
app.post('/api/services/stop', async (req, res) => {
  try {
    const { serviceId } = req.body;

    if (!serviceId) {
      return res.status(400).json({ error: 'serviceId is required' });
    }

    const config = serviceConfigs[serviceId];
    if (!config) {
      return res.status(404).json({ error: `Service ${serviceId} not configured` });
    }

    const processInfo = runningProcesses.get(serviceId);
    if (!processInfo) {
      // Try to stop the service even if we don't have it in memory
      try {
        await executeCommand(config.stopCommand);
        return res.json({
          success: true,
          message: `Service ${serviceId} stop command executed`
        });
      } catch (error) {
        return res.status(404).json({ error: 'Service not running' });
      }
    }

    console.log(`Stopping service ${serviceId}`);

    // Execute stop command
    try {
      await executeCommand(config.stopCommand);
    } catch (error) {
      console.warn(`Stop command failed, trying process kill: ${error.message}`);
      // Fallback: try to kill the process directly
      if (processInfo.process && processInfo.process.pid) {
        try {
          process.kill(processInfo.process.pid, 'SIGTERM');
        } catch (killError) {
          console.warn(`Process kill failed: ${killError.message}`);
        }
      }
    }

    // Update process info
    processInfo.endTime = new Date();
    processInfo.logs.push(`[${new Date().toISOString()}] Service ${serviceId} stopping...`);

    // Remove from running processes after a short delay
    setTimeout(() => {
      runningProcesses.delete(serviceId);
    }, 2000);

    res.json({
      success: true,
      message: `Service ${serviceId} stopped`
    });

  } catch (error) {
    console.error('Error stopping service:', error);
    res.status(500).json({
      error: error.message || 'Failed to stop service'
    });
  }
});

// Check service status endpoint
app.post('/api/services/status', async (req, res) => {
  try {
    const { serviceId } = req.body;

    if (!serviceId) {
      return res.status(400).json({ error: 'serviceId is required' });
    }

    const config = serviceConfigs[serviceId];
    if (!config) {
      return res.status(404).json({ error: `Service ${serviceId} not configured` });
    }

    // Check if we have it in memory first
    const processInfo = runningProcesses.get(serviceId);
    if (processInfo) {
      return res.json({
        running: true,
        pid: processInfo.process?.pid,
        startTime: processInfo.startTime,
        logs: processInfo.logs.slice(-50) // Last 50 log entries
      });
    }

    // Check system status using the configured command
    try {
      const result = await executeCommand(config.statusCommand);
      const isRunning = result.stdout.trim() === 'running';

      res.json({
        running: isRunning,
        pid: null, // We don't have PID from status command
        logs: []
      });
    } catch (error) {
      res.json({
        running: false,
        error: 'Status check failed',
        logs: []
      });
    }

  } catch (error) {
    console.error('Error checking service status:', error);
    res.status(500).json({
      error: error.message || 'Failed to check service status'
    });
  }
});

// Get service logs endpoint
app.get('/api/services/:serviceId/logs', (req, res) => {
  const { serviceId } = req.params;

  const processInfo = runningProcesses.get(serviceId);
  if (processInfo && processInfo.logs) {
    res.json({ logs: processInfo.logs });
  } else {
    res.json({ logs: [] });
  }
});

// Get all services status
app.get('/api/services', (req, res) => {
  const services = Object.keys(serviceConfigs).map(serviceId => {
    const processInfo = runningProcesses.get(serviceId);
    const config = serviceConfigs[serviceId];

    return {
      id: serviceId,
      name: config.name,
      status: processInfo ? 'running' : 'stopped',
      port: config.port,
      pid: processInfo?.process?.pid,
      startTime: processInfo?.startTime,
      category: config.category,
      essential: config.essential,
      dependencies: config.dependencies,
      startupOrder: config.startupOrder,
      logs: processInfo?.logs || []
    };
  });

  res.json({ services });
});

// Get essential services
app.get('/api/services/essential', (req, res) => {
  const essentialServices = Object.keys(serviceConfigs)
    .filter(serviceId => serviceConfigs[serviceId].essential)
    .map(serviceId => {
      const processInfo = runningProcesses.get(serviceId);
      const config = serviceConfigs[serviceId];

      return {
        id: serviceId,
        name: config.name,
        status: processInfo ? 'running' : 'stopped',
        port: config.port,
        pid: processInfo?.process?.pid,
        dependencies: config.dependencies,
        startupOrder: config.startupOrder
      };
    })
    .sort((a, b) => a.startupOrder - b.startupOrder);

  res.json({ essentialServices });
});

// Start all essential services
app.post('/api/services/essential/start', async (req, res) => {
  try {
    const essentialServices = Object.keys(serviceConfigs)
      .filter(serviceId => serviceConfigs[serviceId].essential)
      .sort((a, b) => serviceConfigs[a].startupOrder - serviceConfigs[b].startupOrder);

    const results = [];

    for (const serviceId of essentialServices) {
      const config = serviceConfigs[serviceId];

      // Check dependencies
      for (const dep of config.dependencies) {
        if (!runningProcesses.has(dep)) {
          results.push({
            serviceId,
            status: 'skipped',
            reason: `Dependency ${dep} not running`
          });
          continue;
        }
      }

      // Check if already running
      if (runningProcesses.has(serviceId)) {
        results.push({
          serviceId,
          status: 'already_running'
        });
        continue;
      }

      try {
        console.log(`Starting essential service ${serviceId}`);

        const child = spawn(config.startCommand, [], {
          shell: true,
          cwd: process.cwd(),
          detached: true,
          stdio: ['ignore', 'pipe', 'pipe']
        });

        const processInfo = {
          process: child,
          startTime: new Date(),
          config,
          logs: [`[${new Date().toISOString()}] Service ${serviceId} starting...`]
        };

        runningProcesses.set(serviceId, processInfo);

        // Collect logs
        child.stdout.on('data', (data) => {
          const output = data.toString().trim();
          if (output) {
            const logEntry = `[${new Date().toISOString()}] ${output}`;
            processInfo.logs.push(logEntry);
          }
        });

        child.stderr.on('data', (data) => {
          const output = data.toString().trim();
          if (output) {
            const logEntry = `[${new Date().toISOString()}] ERROR: ${output}`;
            processInfo.logs.push(logEntry);
          }
        });

        child.on('close', (code) => {
          const endTime = new Date();
          processInfo.endTime = endTime;
          processInfo.exitCode = code;
          const logEntry = `[${endTime.toISOString()}] Process exited with code ${code}`;
          processInfo.logs.push(logEntry);
        });

        child.unref();

        results.push({
          serviceId,
          status: 'started',
          pid: child.pid
        });

      } catch (error) {
        results.push({
          serviceId,
          status: 'error',
          error: error.message
        });
      }
    }

    res.json({
      success: true,
      message: `Started ${results.filter(r => r.status === 'started').length} essential services`,
      results
    });

  } catch (error) {
    console.error('Error starting essential services:', error);
    res.status(500).json({
      error: error.message || 'Failed to start essential services'
    });
  }
});

// Stop all essential services
app.post('/api/services/essential/stop', async (req, res) => {
  try {
    const essentialServices = Object.keys(serviceConfigs)
      .filter(serviceId => serviceConfigs[serviceId].essential)
      .reverse(); // Stop in reverse order

    const results = [];

    for (const serviceId of essentialServices) {
      const config = serviceConfigs[serviceId];
      const processInfo = runningProcesses.get(serviceId);

      if (!processInfo) {
        results.push({
          serviceId,
          status: 'not_running'
        });
        continue;
      }

      try {
        console.log(`Stopping essential service ${serviceId}`);

        await executeCommand(config.stopCommand);

        processInfo.endTime = new Date();
        processInfo.logs.push(`[${new Date().toISOString()}] Service ${serviceId} stopping...`);

        setTimeout(() => {
          runningProcesses.delete(serviceId);
        }, 2000);

        results.push({
          serviceId,
          status: 'stopped'
        });

      } catch (error) {
        results.push({
          serviceId,
          status: 'error',
          error: error.message
        });
      }
    }

    res.json({
      success: true,
      message: `Stopped ${results.filter(r => r.status === 'stopped').length} essential services`,
      results
    });

  } catch (error) {
    console.error('Error stopping essential services:', error);
    res.status(500).json({
      error: error.message || 'Failed to stop essential services'
    });
  }
});

// Update essential service configuration
app.put('/api/services/:serviceId/essential', (req, res) => {
  const { serviceId } = req.params;
  const { essential } = req.body;

  if (!serviceConfigs[serviceId]) {
    return res.status(404).json({ error: `Service ${serviceId} not found` });
  }

  if (typeof essential !== 'boolean') {
    return res.status(400).json({ error: 'essential must be a boolean' });
  }

  serviceConfigs[serviceId].essential = essential;

  res.json({
    success: true,
    serviceId,
    essential,
    message: `Service ${serviceId} ${essential ? 'marked as' : 'unmarked from'} essential`
  });
});

// Proxy remaining API traffic to the backend so the UI can reach control plane endpoints
app.use('/api', async (req, res, next) => {
  if (req.path.startsWith('/services') || req.path === '/health') {
    return next();
  }

  const targetUrl = `${API_PROXY_TARGET}${req.originalUrl}`;
  const headers = { ...req.headers };

  delete headers.host;
  delete headers['content-length'];

  const method = req.method.toUpperCase();
  const shouldIncludeBody = !['GET', 'HEAD'].includes(method);
  let body;

  if (shouldIncludeBody) {
    if (req.rawBody && req.rawBody.length) {
      body = req.rawBody;
    } else if (typeof req.body === 'string') {
      body = req.body;
    } else if (req.body && Object.keys(req.body).length > 0) {
      body = JSON.stringify(req.body);
      if (!headers['content-type']) {
        headers['content-type'] = 'application/json';
      }
    }

    if (body !== undefined) {
      const contentLength = Buffer.isBuffer(body)
        ? body.length
        : Buffer.byteLength(body);
      headers['content-length'] = String(contentLength);
    }
  }

  try {
    const response = await fetch(targetUrl, {
      method,
      headers,
      body,
      redirect: 'manual',
    });

    const setCookieHeader =
      typeof response.headers.getSetCookie === 'function'
        ? response.headers.getSetCookie()
        : response.headers.get('set-cookie');

    if (setCookieHeader && (Array.isArray(setCookieHeader) ? setCookieHeader.length > 0 : true)) {
      res.setHeader('set-cookie', setCookieHeader);
    }

    response.headers.forEach((value, key) => {
      if (key.toLowerCase() === 'set-cookie') {
        return;
      }
      res.setHeader(key, value);
    });

    res.status(response.status);

    if (response.status === 204) {
      res.end();
      return;
    }

    const buffer = Buffer.from(await response.arrayBuffer());
    res.send(buffer);
  } catch (error) {
    console.error('API proxy error:', error);
    res.status(502).json({
      error: 'API proxy error',
      details: error?.message || 'Unknown error',
    });
  }
});

// Health check
app.get('/healthz', (req, res) => {
  res.json({
    status: 'healthy',
    timestamp: new Date().toISOString(),
    runningServices: runningProcesses.size,
    totalServices: Object.keys(serviceConfigs).length
  });
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('Shutting down service panel...');

  // Stop all running services
  for (const [serviceId, processInfo] of runningProcesses.entries()) {
    try {
      if (processInfo.process && processInfo.process.pid) {
        process.kill(processInfo.process.pid, 'SIGTERM');
        console.log(`Stopped service ${serviceId}`);
      }
    } catch (error) {
      console.error(`Error stopping ${serviceId}:`, error);
    }
  }

  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log('Received SIGTERM, shutting down...');
  process.emit('SIGINT');
});

// Start server
app.listen(PORT, '0.0.0.0', () => {
  console.log(`🚀 Service Management Panel running on http://0.0.0.0:${PORT}`);
  console.log(`📊 Health check: http://0.0.0.0:${PORT}/healthz`);
  console.log(`🎛️  Available services: ${Object.keys(serviceConfigs).join(', ')}`);
});
