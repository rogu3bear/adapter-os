#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import process from 'node:process';

const DEFAULT_PORT = 3200;
const modeArg = process.argv.find((arg) => arg.startsWith('--mode='));
const mode = modeArg ? modeArg.split('=')[1] : (process.argv[2] ?? 'dev');
const port = Number.parseInt(process.env.AOS_DEV_PORT ?? `${DEFAULT_PORT}`, 10);
const testingMarker = process.env.AOS_PORT_TESTING_TAG ?? 'AOS_PORT_3200_TAG=testing';
const killTimeoutMs = Number.parseInt(process.env.AOS_PORT_KILL_TIMEOUT ?? '5000', 10);

function getPortProcesses() {
  const result = spawnSync('lsof', ['-nP', '-i', `:${port}`, '-sTCP:LISTEN', '-Fp'], { encoding: 'utf8' });
  if (result.error) {
    if (result.error.code === 'ENOENT') {
      console.warn('[ensure-port] `lsof` not available; skipping port enforcement.');
      return [];
    }
    throw result.error;
  }

  if (result.status !== 0 && !result.stdout.trim()) {
    return [];
  }

  return result.stdout
    .split('\n')
    .filter((line) => line.startsWith('p'))
    .map((line) => line.slice(1))
    .filter(Boolean);
}

function describePid(pid) {
  const result = spawnSync('ps', ['-p', pid, '-o', 'command='], { encoding: 'utf8' });
  return (result.stdout ?? '').trim();
}

async function waitForExit(targetPids) {
  const deadline = Date.now() + killTimeoutMs;
  while (Date.now() < deadline) {
    const remaining = getPortProcesses();
    if (!remaining.some((pid) => targetPids.includes(pid))) {
      return true;
    }
    await sleep(150);
  }
  return false;
}

async function makeSurePortAvailable() {
  const initialPids = getPortProcesses();
  if (initialPids.length === 0) {
    return 0;
  }

  const processes = initialPids.map((pid) => {
    const command = describePid(pid);
    const isTesting = command.includes(testingMarker) || /--testing\b/.test(command);
    return { pid, command, isTesting };
  });

  const killTargets = processes.filter((proc) => mode === 'build' || !proc.isTesting);
  const skipped = processes.filter((proc) => !killTargets.includes(proc));

  if (killTargets.length) {
    for (const proc of killTargets) {
      console.log(`[ensure-port] Terminating PID ${proc.pid} on port ${port}${proc.isTesting ? ' (testing override ignored)' : ''}`);
      try {
        process.kill(Number.parseInt(proc.pid, 10), 'SIGTERM');
      } catch (error) {
        if (error.code !== 'ESRCH') {
          console.warn(`[ensure-port] Failed to signal PID ${proc.pid}: ${error.message}`);
        }
      }
    }

    const cleanExit = await waitForExit(killTargets.map((proc) => proc.pid));
    if (!cleanExit) {
      for (const proc of killTargets) {
        try {
          process.kill(Number.parseInt(proc.pid, 10), 'SIGKILL');
        } catch (error) {
          if (error.code !== 'ESRCH') {
            console.warn(`[ensure-port] Failed to force kill PID ${proc.pid}: ${error.message}`);
          }
        }
      }
      await waitForExit(killTargets.map((proc) => proc.pid));
    }
  }

  if (mode === 'dev' && skipped.length) {
    console.warn(`[ensure-port] Port ${port} reserved for testing by:`);
    skipped.forEach((proc) => console.warn(`  PID ${proc.pid}: ${proc.command}`));
    console.warn('[ensure-port] Dev server will not start until testing process releases the port.');
    return 2;
  }

  const remaining = getPortProcesses();
  if (remaining.length > 0) {
    console.warn(`[ensure-port] Port ${port} still in use by PIDs: ${remaining.join(', ')}`);
    return 1;
  }

  return 0;
}

makeSurePortAvailable()
  .then((code) => {
    process.exit(code);
  })
  .catch((error) => {
    console.error(`[ensure-port] ${error.message}`);
    process.exit(1);
  });
