#!/usr/bin/env node
/**
 * E2E Test Runner with Dynamic Port Configuration
 *
 * This wrapper script constructs health check URLs dynamically from environment
 * variables, allowing developers with port offsets to run E2E tests without
 * modifying package.json.
 *
 * Environment Variables:
 *   AOS_SERVER_HOST - Backend host (default: 127.0.0.1)
 *   AOS_SERVER_PORT - Backend port (default: 8080)
 *   AOS_UI_PORT     - Frontend port (default: 3200)
 *   WAIT_ON_TIMEOUT - Health check timeout in ms (default: 180000)
 *   WAIT_ON_INTERVAL - Health check interval in ms (default: 1000)
 *   AOS_PID_FILE    - PID file path (default: var/aos-cp-e2e.pid)
 */

import { spawn } from 'node:child_process';
import process from 'node:process';

// Read port configuration from environment
const backendHost = process.env.AOS_SERVER_HOST || '127.0.0.1';
const backendPort = process.env.AOS_SERVER_PORT || '8080';
const uiPort = process.env.AOS_UI_PORT || '3200';
const waitTimeout = process.env.WAIT_ON_TIMEOUT || '180000';
const waitInterval = process.env.WAIT_ON_INTERVAL || '1000';
const pidFile = process.env.AOS_PID_FILE || 'var/aos-cp-e2e.pid';

// Construct health check URLs
const backendHealthUrl = `http-get://${backendHost}:${backendPort}/api/readyz`;
const uiHealthUrl = `http://localhost:${uiPort}`;
const healthUrls = `${backendHealthUrl},${uiHealthUrl}`;

// Get Cypress spec if provided as argument
const cypressArgs = process.argv.slice(2);
const cypressCommand = cypressArgs.length > 0 ? cypressArgs.join(' ') : 'cypress:run';

console.log('E2E Test Configuration:');
console.log(`  Backend health: ${backendHealthUrl}`);
console.log(`  UI health:      ${uiHealthUrl}`);
console.log(`  Timeout:        ${waitTimeout}ms`);
console.log(`  Interval:       ${waitInterval}ms`);
console.log(`  PID file:       ${pidFile}`);
console.log(`  Cypress cmd:    ${cypressCommand}`);
console.log();

// Spawn start-server-and-test with dynamic URLs
const child = spawn(
  'pnpm',
  [
    'exec',
    'start-server-and-test',
    'dev',
    healthUrls,
    cypressCommand,
  ],
  {
    stdio: 'inherit',
    env: {
      ...process.env,
      AOS_PID_FILE: pidFile,
      WAIT_ON_TIMEOUT: waitTimeout,
      WAIT_ON_INTERVAL: waitInterval,
    },
  }
);

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});

// Forward signals to child process
['SIGINT', 'SIGTERM', 'SIGQUIT', 'SIGHUP'].forEach((signal) => {
  process.on(signal, () => {
    if (!child.killed) {
      child.kill(signal === 'SIGINT' ? 'SIGTERM' : signal);
    }
  });
});
