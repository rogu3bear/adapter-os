#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import process from 'node:process';

const DEFAULT_PORT = 3200;
const modeArg = process.argv.find((arg) => arg.startsWith('--mode='));
const mode = modeArg ? modeArg.split('=')[1] : (process.argv[2] ?? 'dev');
const port = Number.parseInt(process.env.AOS_DEV_PORT ?? `${DEFAULT_PORT}`, 10);
const testingMarkers = (process.env.AOS_PORT_TESTING_MARKERS ?? '--testing,AOS_PORT_3200_TAG=testing')
  .split(',')
  .map((m) => m.trim())
  .filter(Boolean);
const testingGraceMs = Number.parseInt(process.env.AOS_PORT_TESTING_GRACE ?? '4000', 10);
const killTimeoutMs = Number.parseInt(process.env.AOS_PORT_KILL_TIMEOUT ?? '5000', 10);
const manualClearWaitMs = Number.parseInt(process.env.AOS_PORT_MANUAL_WAIT ?? '8000', 10);

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
  const result = spawnSync('ps', ['-p', pid, '-ww', '-o', 'command='], { encoding: 'utf8' });
  return (result.stdout ?? '').trim();
}

async function waitForExit(targetPids, timeoutMs = killTimeoutMs) {
  const deadline = Date.now() + timeoutMs;
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
    const isTesting = testingMarkers.some((marker) => command.includes(marker));
    return { pid, command, isTesting };
  });

  const killTargets = [];
  const testingHolders = [];

  for (const proc of processes) {
    if (proc.isTesting && mode === 'dev') {
      testingHolders.push(proc);
    } else {
      killTargets.push(proc);
    }
  }

  if (testingHolders.length) {
    console.warn(`[ensure-port] Port ${port} held by testing process${testingHolders.length > 1 ? 'es' : ''}. Waiting up to ${testingGraceMs}ms for graceful exit...`);
    testingHolders.forEach((proc) => console.warn(`  PID ${proc.pid}: ${proc.command}`));
    const released = await waitForExit(testingHolders.map((proc) => proc.pid), testingGraceMs);
    if (!released) {
      console.warn('[ensure-port] Testing hold timed out; terminating to enforce port policy.');
      killTargets.push(...testingHolders);
    } else {
      console.log('[ensure-port] Testing process released port.');
    }
  }

  if (killTargets.length) {
    let needsManualClear = false;
    for (const proc of killTargets) {
      console.log(`[ensure-port] Terminating PID ${proc.pid} on port ${port}${proc.isTesting ? ' (testing override ignored)' : ''}`);
      try {
        process.kill(Number.parseInt(proc.pid, 10), 'SIGTERM');
      } catch (error) {
        if (error.code !== 'ESRCH') {
            console.warn(`[ensure-port] Failed to signal PID ${proc.pid}: ${error.message}`);
            needsManualClear = needsManualClear || error.code === 'EPERM';
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
            needsManualClear = needsManualClear || error.code === 'EPERM';
          }
        }
      }
      await waitForExit(killTargets.map((proc) => proc.pid));

      // If we couldn't signal due to permissions, give the operator a grace window to clear it manually
      if (needsManualClear) {
        console.warn(`[ensure-port] Waiting up to ${manualClearWaitMs}ms for manual port clearance (permission denied when signaling).`);
        await sleep(manualClearWaitMs);
      }
    }
  }

  const remaining = getPortProcesses();
  if (remaining.length > 0) {
    console.warn(`[ensure-port] Port ${port} still in use by PIDs: ${remaining.join(', ')}`);
    console.warn('[ensure-port] Build cannot proceed while the port is occupied. Please stop the listed process(es) or set AOS_DEV_PORT to a free port, then retry.');
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
