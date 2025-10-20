#!/usr/bin/env node
import { spawn, spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const extraArgs = process.argv.slice(2);
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const ensureScript = path.resolve(__dirname, 'ensure-port.mjs');
const ensure = spawnSync('node', [ensureScript, '--mode=dev'], { stdio: 'inherit' });

if (ensure.status !== 0) {
  process.exit(ensure.status ?? 1);
}

const viteBin = path.resolve(__dirname, '../node_modules/vite/bin/vite.js');
const child = spawn('node', [viteBin, ...extraArgs], {
  stdio: 'inherit',
  env: {
    ...process.env,
    AOS_PORT_3200_TAG: process.env.AOS_PORT_3200_TAG ?? 'dev',
  },
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
