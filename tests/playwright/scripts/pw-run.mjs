import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';

function nowMs() {
  return Date.now();
}

function sanitizeRunId(value) {
  const cleaned = String(value ?? '')
    .trim()
    .replace(/[^A-Za-z0-9._-]/g, '_');
  return cleaned || 'default';
}

function generateRunId() {
  const ts = new Date().toISOString().replace(/[-:.TZ]/g, '');
  const rand = randomBytes(3).toString('hex');
  return `run-${ts}-${process.pid}-${rand}`;
}

function resolveRunId() {
  const configured = (process.env.PW_RUN_ID ?? '').trim();
  if (configured) return sanitizeRunId(configured);
  return generateRunId();
}

function parseExplicitPort(value) {
  const raw = (value ?? '').trim();
  if (!raw) return null;
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
    return null;
  }
  return parsed;
}

async function canListenOnPort(port) {
  return await new Promise((resolve) => {
    const server = net.createServer();
    server.unref();
    server.once('error', () => resolve(false));
    server.listen(port, '127.0.0.1', () => {
      server.close(() => resolve(true));
    });
  });
}

async function resolveServerPort(runId, explicitPort) {
  if (Number.isFinite(explicitPort)) return explicitPort;

  const minPort = 18080;
  const span = 10_000;
  const hash = createHash('sha256').update(runId).digest();
  const baseOffset = hash.readUInt16BE(0) % span;

  for (let i = 0; i < span; i += 1) {
    const candidate = minPort + ((baseOffset + i) % span);
    // Best effort: avoid a known-in-use port before launching Playwright webServer.
    // A race can still exist, but this dramatically reduces collisions in concurrent runs.
    if (await canListenOnPort(candidate)) return candidate;
  }

  return 8080;
}

function parseArgs(argv) {
  const delim = argv.indexOf('--');
  const runnerArgs = delim === -1 ? argv : argv.slice(0, delim);
  const pwArgs = delim === -1 ? [] : argv.slice(delim + 1);

  let configPath = null;
  let idleMs = null;
  let hardMs = null;

  for (let i = 0; i < runnerArgs.length; i += 1) {
    const arg = runnerArgs[i];
    if (arg === '-c' || arg === '--config') {
      configPath = runnerArgs[i + 1];
      i += 1;
      continue;
    }
    if (arg === '--idle-ms') {
      idleMs = Number(runnerArgs[i + 1]);
      i += 1;
      continue;
    }
    if (arg === '--hard-ms') {
      hardMs = Number(runnerArgs[i + 1]);
      i += 1;
      continue;
    }
  }

  return { configPath, idleMs, hardMs, pwArgs };
}

function resolvePlaywrightBin() {
  // Prefer local install (works on CI and dev)
  const base = process.platform === 'win32' ? 'playwright.cmd' : 'playwright';
  return path.resolve(process.cwd(), 'node_modules', '.bin', base);
}

function safeReadStatMs(filePath) {
  try {
    const st = fs.statSync(filePath);
    return st.mtimeMs;
  } catch {
    return null;
  }
}

function readPidFile(pidPath) {
  try {
    const raw = fs.readFileSync(pidPath, 'utf8').trim();
    const pid = Number.parseInt(raw, 10);
    return Number.isFinite(pid) ? pid : null;
  } catch {
    return null;
  }
}

function killPid(pid, signal) {
  if (!pid) return;
  try {
    process.kill(pid, signal);
  } catch {
    // ignore
  }
}

async function main() {
  const { configPath, idleMs, hardMs, pwArgs } = parseArgs(process.argv.slice(2));

  if (!configPath) {
    console.error('pw-run: missing -c/--config <path>');
    process.exit(2);
  }

  const repoRoot = path.resolve(process.cwd(), '..', '..');
  const runId = resolveRunId();
  const explicitPort = parseExplicitPort(process.env.PW_SERVER_PORT);
  const serverPort = await resolveServerPort(runId, explicitPort);
  const runRoot = path.resolve(repoRoot, 'var/playwright/runs', runId);
  const heartbeatPath = path.resolve(runRoot, 'heartbeat.json');
  const serverPidPath = path.resolve(runRoot, 'run/aos-server.pid');

  const hardTimeoutMs = Number.isFinite(hardMs) ? hardMs : 15 * 60_000;
  // Default idle timeout is intentionally generous: initial Rust+WASM builds can be quiet for
  // several minutes on a cold cache, and we don't want spurious watchdog kills.
  const idleTimeoutMs = Number.isFinite(idleMs) ? idleMs : 10 * 60_000;

  fs.mkdirSync(path.dirname(serverPidPath), { recursive: true });

  // Clean heartbeat from prior runs to make stall detection deterministic.
  try {
    fs.rmSync(heartbeatPath, { force: true });
  } catch {
    // ignore
  }
  try {
    fs.rmSync(serverPidPath, { force: true });
  } catch {
    // ignore
  }

  const playwrightBin = resolvePlaywrightBin();
  const args = ['test', '-c', configPath, ...pwArgs];

  const env = {
    ...process.env,
    // Tell configs to avoid reusing external servers; orphan prevention depends on this.
    PW_REUSE_EXISTING_SERVER: '0',
    // Enable progress + heartbeat reporters.
    PW_WATCHDOG: '1',
    PW_RUN_ID: runId,
    PW_SERVER_PORT: String(serverPort),
  };

  const startedAt = nowMs();
  let lastActivityAt = startedAt;
  let lastHeartbeatAt = safeReadStatMs(heartbeatPath) ?? 0;

  const child = spawn(playwrightBin, args, {
    env,
    cwd: process.cwd(),
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: process.platform !== 'win32',
  });

  const ring = [];
  const RING_MAX = 200;
  const onChunk = (chunk) => {
    lastActivityAt = nowMs();
    const text = chunk.toString('utf8');
    // Keep last N lines for watchdog output.
    for (const line of text.split(/\r?\n/)) {
      if (!line) continue;
      ring.push(line);
      if (ring.length > RING_MAX) ring.shift();
    }
    process.stdout.write(text);
  };
  const onErrChunk = (chunk) => {
    lastActivityAt = nowMs();
    const text = chunk.toString('utf8');
    for (const line of text.split(/\r?\n/)) {
      if (!line) continue;
      ring.push(line);
      if (ring.length > RING_MAX) ring.shift();
    }
    process.stderr.write(text);
  };
  child.stdout.on('data', onChunk);
  child.stderr.on('data', onErrChunk);

  let stopping = false;
  const killTree = async (reason) => {
    if (stopping) return;
    stopping = true;
    try {
      process.stderr.write(`\n[pw-run] stopping (${reason})\n`);
      if (ring.length) {
        process.stderr.write('[pw-run] last output:\n');
        process.stderr.write(ring.map((l) => `  ${l}`).join('\n') + '\n');
      }
    } catch {
      // ignore
    }

    try {
      if (process.platform === 'win32') {
        // Best-effort: kill the whole tree.
        spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], {
          stdio: 'ignore',
        });
      } else {
        process.kill(-child.pid, 'SIGTERM');
      }
    } catch {
      // ignore
    }

    // Best-effort: kill the server started by Playwright (pid file is written by backendCommand).
    killPid(readPidFile(serverPidPath), 'SIGTERM');

    // Grace then hard kill.
    await new Promise((r) => setTimeout(r, 5000));
    try {
      if (process.platform === 'win32') {
        // no-op; taskkill already forced
      } else {
        process.kill(-child.pid, 'SIGKILL');
      }
    } catch {
      // ignore
    }

    killPid(readPidFile(serverPidPath), 'SIGKILL');
  };

  const onSignal = (sig) => {
    void killTree(`signal ${sig}`);
  };
  process.on('SIGINT', onSignal);
  process.on('SIGTERM', onSignal);

  const watchdog = setInterval(() => {
    const t = nowMs();
    const hb = safeReadStatMs(heartbeatPath);
    if (hb) lastHeartbeatAt = hb;

    if (t - startedAt > hardTimeoutMs) {
      void killTree(`hard timeout ${hardTimeoutMs}ms exceeded`);
      clearInterval(watchdog);
      return;
    }

    // Prefer whichever signal is freshest so log output still counts as activity.
    const last = Math.max(lastActivityAt, lastHeartbeatAt);
    if (t - last > idleTimeoutMs) {
      void killTree(`idle timeout ${idleTimeoutMs}ms exceeded`);
      clearInterval(watchdog);
    }
  }, 1000);

  const exitCode = await new Promise((resolve) => {
    child.on('exit', (code) => resolve(code ?? 1));
  });

  clearInterval(watchdog);
  process.off('SIGINT', onSignal);
  process.off('SIGTERM', onSignal);

  // Best-effort cleanup: Playwright should stop the web server, but it can get orphaned.
  killPid(readPidFile(serverPidPath), 'SIGTERM');
  await new Promise((r) => setTimeout(r, 1000));
  killPid(readPidFile(serverPidPath), 'SIGKILL');

  process.exit(exitCode);
}

main().catch((err) => {
  console.error(`[pw-run] fatal: ${err?.stack ?? String(err)}`);
  process.exit(1);
});
