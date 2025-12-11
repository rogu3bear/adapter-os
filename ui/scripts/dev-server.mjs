#!/usr/bin/env node
import { spawn, spawnSync, exec } from 'node:child_process';
import { createWriteStream, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { setInterval, clearInterval, setTimeout } from 'node:timers';

const extraArgs = process.argv.slice(2);
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const projectRoot = path.resolve(__dirname, '..', '..');
const pidFile = path.resolve(projectRoot, process.env.AOS_PID_FILE ?? 'var/aos-cp-e2e.pid');
const dbPath = path.resolve(projectRoot, 'var', 'aos-cp.sqlite3');

// Ensure ports for dev
const ensureScript = path.resolve(__dirname, 'ensure-port.mjs');
const ensure = spawnSync('node', [ensureScript, '--mode=dev'], { stdio: 'inherit' });

if (ensure.status !== 0) {
  process.exit(ensure.status ?? 1);
}

// Ensure backend port (control plane default: 8080)
process.env.AOS_DEV_PORT = process.env.AOS_DEV_PORT ?? '8080';
const ensureBackend = spawnSync('node', [ensureScript, '--mode=dev'], {
  stdio: 'inherit',
  env: process.env,
});

if (ensureBackend.status !== 0) {
  console.warn('Backend port ensure failed, continuing');
}

try {
  rmSync(pidFile);
} catch {
  // ignore missing pid file
}

// For e2e runs, reset the control-plane DB to avoid drift against modified migrations.
if ((process.env.AOS_E2E_RESET_DB ?? '1') !== '0') {
  try {
    rmSync(dbPath);
  } catch {
    // ignore missing db
  }
  try {
    writeFileSync(dbPath, '');
  } catch {
    // ignore write errors; sqlx will attempt to create if possible
  }

  const migrateEnv = {
    ...process.env,
    DATABASE_URL:
      process.env.DATABASE_URL ??
      `sqlite://${path.resolve(projectRoot, 'var', 'aos-cp.sqlite3')}`,
    AOS_SKIP_MIGRATION_SIGNATURES: process.env.AOS_SKIP_MIGRATION_SIGNATURES ?? '1',
  };

  const migrate = spawnSync('cargo', ['sqlx', 'migrate', 'run'], {
    cwd: projectRoot,
    stdio: 'inherit',
    env: migrateEnv,
  });
  if (migrate.status !== 0) {
    console.error('Failed to run migrations for dev server');
    process.exit(migrate.status ?? 1);
  }
}

// Start backend server
const backendLog = path.resolve(projectRoot, 'server-dev.log');
const backendHost = process.env.AOS_SERVER_HOST ?? '127.0.0.1';
const backendPort = process.env.AOS_SERVER_PORT ?? process.env.AOS_SERVER__PORT ?? '8080';
const healthPath = process.env.AOS_BACKEND_HEALTH_PATH ?? '/api/readyz'; // Public, no auth required; see routes.rs
const readinessHost = backendHost === '0.0.0.0' ? '127.0.0.1' : backendHost;
const healthUrl = `http://${readinessHost}:${backendPort}${healthPath}`;
const backendWaitMs = Number(process.env.AOS_BACKEND_WAIT_MS ?? '180000');
const backendWaitIntervalMs = Number(process.env.AOS_BACKEND_WAIT_INTERVAL_MS ?? '1000');
const backendWaitInitialMs = Number(process.env.AOS_BACKEND_WAIT_INITIAL_MS ?? '500');
const backendArgs = [
  'run',
  '--features',
  'dev-bypass',
  '-p',
  'adapteros-server',
  '--bin',
  'adapteros-server',
  '--',
  '--config',
  'configs/cp.toml',
  '--pid-file',
  pidFile,
  '--single-writer',
];
const backendEnv = {
  ...process.env,
  // Force control plane to bind on 8080 in dev; fail fast if occupied
  AOS_SERVER_PORT: process.env.AOS_SERVER_PORT ?? '8080',
  AOS_SERVER__PORT: process.env.AOS_SERVER__PORT ?? '8080',
  // Enable dev no-auth in debug/dev
  AOS_DEV_NO_AUTH: process.env.AOS_DEV_NO_AUTH ?? '1',
  // Allow local/e2e runs even if migration signatures differ (tests only)
  AOS_SKIP_MIGRATION_SIGNATURES: process.env.AOS_SKIP_MIGRATION_SIGNATURES ?? '1',
  AOS_PID_FILE: pidFile,
  // Skip SQLx statement validation during e2e bootstrap
  SQLX_DISABLE_STATEMENT_CHECKS: process.env.SQLX_DISABLE_STATEMENT_CHECKS ?? '1',
  AOS_DATABASE_URL:
    process.env.AOS_DATABASE_URL ??
    process.env.DATABASE_URL ??
    `sqlite://${path.resolve(projectRoot, 'var', 'aos-cp.sqlite3')}`,
  DATABASE_URL:
    process.env.DATABASE_URL ??
    `sqlite://${path.resolve(projectRoot, 'var', 'aos-cp.sqlite3')}`,
};
const backendEnvSummary = {
  AOS_SERVER_HOST: backendHost,
  AOS_SERVER_PORT: backendEnv.AOS_SERVER_PORT,
  AOS_SERVER__PORT: backendEnv.AOS_SERVER__PORT,
  AOS_DEV_NO_AUTH: backendEnv.AOS_DEV_NO_AUTH,
  AOS_SKIP_MIGRATION_SIGNATURES: backendEnv.AOS_SKIP_MIGRATION_SIGNATURES,
  SQLX_DISABLE_STATEMENT_CHECKS: backendEnv.SQLX_DISABLE_STATEMENT_CHECKS,
  AOS_DATABASE_URL: backendEnv.AOS_DATABASE_URL,
  DATABASE_URL: backendEnv.DATABASE_URL,
  AOS_PID_FILE: backendEnv.AOS_PID_FILE,
};
const backendCommandString = `cargo ${backendArgs.join(' ')}`;
const backendLogStream = createWriteStream(backendLog, { flags: 'a' });
backendLogStream.write(
  `\n[dev-server] adapteros-server launch\n[dev-server] cwd: ${projectRoot}\n[dev-server] cmd: ${backendCommandString}\n[dev-server] env: ${JSON.stringify(
    backendEnvSummary,
  )}\n`,
);
console.log(
  `Starting adapteros-server with:\n  cmd: ${backendCommandString}\n  cwd: ${projectRoot}\n  env: ${JSON.stringify(
    backendEnvSummary,
  )}\n  log: ${backendLog}`,
);

const backend = spawn('cargo', backendArgs, {
  cwd: projectRoot,
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: true,
  env: backendEnv,
});

let backendReady = false;
let backendExitInfo = null;
let shuttingDown = false;
let readinessInterval;
let readinessTimeout;
let readinessInitialTimer;
let lastHealthError = null;
let lastLoggedHealthError = null;
let readinessAttemptCount = 0;

const pipeOutput = (stream, target) => {
  stream.on('data', (data) => {
    backendLogStream.write(data);
    target.write(data);
  });
};

pipeOutput(backend.stdout, process.stdout);
pipeOutput(backend.stderr, process.stderr);

const tailBackendLog = () => {
  try {
    const tail = readFileSync(backendLog, 'utf8').split('\n').slice(-120).join('\n');
    console.error(tail);
    return tail;
  } catch (err) {
    console.error('Unable to read backend log for diagnostics', err);
    return '';
  }
};

const clearReadinessTimers = () => {
  if (readinessInterval) clearInterval(readinessInterval);
  if (readinessTimeout) clearTimeout(readinessTimeout);
  if (readinessInitialTimer) clearTimeout(readinessInitialTimer);
};

const failFast = (message) => {
  const prefix = `[dev-server] ${message}`;
  backendLogStream.write(`${prefix}\n`);
  console.error(prefix);
  tailBackendLog();
  cleanup();
  process.exit(1);
};

const startReadinessCheck = () => {
  console.log(
    `Waiting for backend readiness at ${healthUrl} (timeout ${backendWaitMs}ms, interval ${backendWaitIntervalMs}ms)`,
  );
  const logHealthError = (errorMessage) => {
    if (!errorMessage) return;
    const shouldLog = lastLoggedHealthError !== errorMessage || readinessAttemptCount % 30 === 0;
    if (shouldLog) {
      const msg = `[dev-server] readiness probe error: ${errorMessage}`;
      backendLogStream.write(`${msg}\n`);
      console.warn(msg);
      lastLoggedHealthError = errorMessage;
    }
  };

  const probeHealth = async () => {
    try {
      const res = await fetch(healthUrl, { method: 'GET' });
      if (res.ok) {
        backendReady = true;
        clearReadinessTimers();
        const readyMsg = `Backend readiness confirmed at ${healthUrl}`;
        backendLogStream.write(`[dev-server] ${readyMsg}\n`);
        console.log(readyMsg);
        return;
      }
      lastHealthError = `HTTP ${res.status}`;
      logHealthError(lastHealthError);
    } catch (err) {
      const cause = err?.cause;
      const causeInfo =
        cause && (cause.code || cause.errno) ? ` (cause: ${cause.code ?? cause.errno})` : '';
      lastHealthError = `${err?.message ?? 'unknown error'}${causeInfo}`;
      logHealthError(lastHealthError);
    }
  };

  void probeHealth();

  readinessInterval = setInterval(() => {
    readinessAttemptCount += 1;
    if (backendExitInfo && !backendReady && !shuttingDown) {
      clearReadinessTimers();
      failFast(
        `Backend exited before readiness (code=${backendExitInfo.code}, signal=${backendExitInfo.signal})`,
      );
      return;
    }
    void probeHealth();
  }, backendWaitIntervalMs);

  readinessTimeout = setTimeout(() => {
    if (!backendReady) {
      clearReadinessTimers();
      const healthHint = lastHealthError ? ` (last error: ${lastHealthError})` : '';
      failFast(`Backend not ready after ${backendWaitMs}ms at ${healthUrl}${healthHint}`);
    }
  }, backendWaitMs);
};

backend.once('spawn', () => {
  console.log(`Backend spawned with PID ${backend.pid}, logs in ${backendLog}`);
  readinessInitialTimer = setTimeout(startReadinessCheck, backendWaitInitialMs);
});

backend.on('exit', (code, signal) => {
  backendExitInfo = { code, signal };
  const exitMsg = `Backend exited with code=${code} signal=${signal}`;
  backendLogStream.write(`[dev-server] ${exitMsg}\n`);
  if (!shuttingDown && !backendReady) {
    clearReadinessTimers();
    failFast(`${exitMsg} before readiness completed`);
  }
  if (!shuttingDown && code !== 0) {
    clearReadinessTimers();
    failFast(`${exitMsg} (non-zero)`);
  }
});

backend.on('error', (err) => {
  backendLogStream.write(`[dev-server] Backend spawn error: ${err.message}\n`);
  failFast(`Backend error: ${err.message}`);
});

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
  shuttingDown = true;

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
