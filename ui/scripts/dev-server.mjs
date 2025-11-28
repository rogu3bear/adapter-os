#!/usr/bin/env node
import { spawn, spawnSync, exec } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { setInterval, clearInterval, setTimeout } from 'node:timers';

const extraArgs = process.argv.slice(2);
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const projectRoot = path.resolve(__dirname, '..', '..');

// Ensure ports for dev
const ensureScript = path.resolve(__dirname, 'ensure-port.mjs');
const ensure = spawnSync('node', [ensureScript, '--mode=dev'], { stdio: 'inherit' });

if (ensure.status !== 0) {
  process.exit(ensure.status ?? 1);
}

// Ensure backend port
process.env.AOS_DEV_PORT = '3300';
const ensureBackend = spawnSync('node', [ensureScript, '--mode=dev'], { stdio: 'inherit', env: process.env });

if (ensureBackend.status !== 0) {
  console.warn('Backend port ensure failed, continuing');
}

// Start backend server
const backendLog = path.resolve(projectRoot, 'server-dev.log');
const backend = spawn('cargo', [
  'run',
  '-p', 'adapteros-server',
  '--bin', 'adapteros-server',
  '--',
  '--config', 'configs/cp.toml',
  '--skip-pf-check',
  '--single-writer'
], {
  cwd: projectRoot,
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: true
});

backend.stdout.pipe(createWriteStream(backendLog, { flags: 'a' }));
backend.stderr.pipe(createWriteStream(backendLog, { flags: 'a' }));

console.log(`Backend started with PID ${backend.pid}, logs in ${backendLog}`);

// Wait for backend to be ready
let backendReady = false;
const checkBackend = setInterval(() => {
  exec('curl -f http://localhost:8080/api/healthz >/dev/null 2>&1', (err) => {
    if (!err) {
      backendReady = true;
      clearInterval(checkBackend);
      console.log('Backend is ready');
    }
  });
}, 1000);

setTimeout(() => {
  if (!backendReady) {
    console.warn('Backend not ready after 30s, continuing anyway');
    clearInterval(checkBackend);
  }
}, 30000);

// Start Vite after backend
const viteBin = path.resolve(__dirname, '../node_modules/vite/bin/vite.js');
const child = spawn('node', [viteBin, ...extraArgs], {
  stdio: 'inherit',
  env: {
    ...process.env,
    AOS_PORT_3200_TAG: process.env.AOS_PORT_3200_TAG ?? 'dev',
  },
});

let cleanupRan = false;

function cleanup() {
  if (cleanupRan) return;
  cleanupRan = true;

  // Stop backend gracefully
  if (!backend.killed) {
    console.log('Stopping backend gracefully...');
    backend.kill('SIGTERM');
    // Wait up to 30s
    const timeout = setTimeout(() => {
      if (!backend.killed) {
        console.log('Force killing backend');
        backend.kill('SIGKILL');
      }
    }, 30000);
    backend.once('exit', () => clearTimeout(timeout));
  }

  // Ensure ports cleanup for backend
  process.env.AOS_DEV_PORT = '3300';
  spawnSync('node', [ensureScript, '--mode=build'], { stdio: 'inherit', env: process.env });

  // UI port cleanup
  delete process.env.AOS_DEV_PORT;
  spawnSync('node', [ensureScript, '--mode=build'], { stdio: 'inherit', env: process.env });
}

const forwardExit = (signal) => {
  if (!child.killed) {
    child.kill(signal === 'SIGINT' ? 'SIGTERM' : signal);
  }
  if (!backend.killed) {
    backend.kill(signal === 'SIGINT' ? 'SIGTERM' : signal);
  }
};

['SIGINT', 'SIGTERM', 'SIGQUIT', 'SIGHUP'].forEach((signal) => {
  process.on(signal, () => {
    forwardExit(signal);
  });
});

process.on('exit', () => {
  cleanup();
});

process.on('uncaughtException', (error) => {
  cleanup();
  throw error;
});

child.on('exit', (code, signal) => {
  cleanup();
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});

backend.on('error', (err) => {
  console.error('Backend error:', err);
  process.exit(1);
});
