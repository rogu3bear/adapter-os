import { spawn } from 'node:child_process';

const suiteFiles = ['ui/auth.spec.ts', 'ui/routes.core.smoke.spec.ts'];
const runs = [
  {
    label: 'concurrency-a',
    project: 'chromium',
    runId: 'ci-concurrency-a',
    port: '4190',
  },
  {
    label: 'concurrency-b',
    project: 'webkit',
    runId: 'ci-concurrency-b',
    port: '4191',
  },
];

const children = new Set();

function pipeWithPrefix(prefix, stream, target) {
  let tail = '';
  stream.on('data', (chunk) => {
    tail += chunk.toString('utf8');
    const lines = tail.split(/\r?\n/);
    tail = lines.pop() ?? '';
    for (const line of lines) {
      if (line.length === 0) continue;
      target.write(`[${prefix}] ${line}\n`);
    }
  });
  stream.on('end', () => {
    if (tail.length > 0) {
      target.write(`[${prefix}] ${tail}\n`);
    }
  });
}

function terminateChildren(signal) {
  for (const child of children) {
    if (!child.killed) {
      child.kill(signal);
    }
  }
}

function runConcurrencySlice(run) {
  return new Promise((resolve) => {
    const child = spawn(
      process.execPath,
      [
        'scripts/pw-run.mjs',
        '-c',
        'playwright.ui.config.ts',
        '--',
        `--project=${run.project}`,
        ...suiteFiles,
      ],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          PW_RUN_ID: run.runId,
          PW_SERVER_PORT: run.port,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      }
    );

    children.add(child);
    pipeWithPrefix(run.label, child.stdout, process.stdout);
    pipeWithPrefix(run.label, child.stderr, process.stderr);

    child.on('error', (error) => {
      children.delete(child);
      resolve({
        label: run.label,
        code: 1,
        signal: null,
        error: String(error),
      });
    });

    child.on('exit', (code, signal) => {
      children.delete(child);
      resolve({
        label: run.label,
        code: code ?? 1,
        signal: signal ?? null,
      });
    });
  });
}

async function main() {
  process.on('SIGINT', () => terminateChildren('SIGINT'));
  process.on('SIGTERM', () => terminateChildren('SIGTERM'));

  const results = await Promise.all(runs.map(runConcurrencySlice));
  const failed = results.filter((result) => result.code !== 0);

  for (const result of results) {
    const status = result.code === 0 ? 'PASS' : 'FAIL';
    const signalText = result.signal ? `, signal=${result.signal}` : '';
    const errorText = result.error ? `, error=${result.error}` : '';
    process.stdout.write(
      `[concurrency-smoke] ${result.label}: ${status} (code=${result.code}${signalText}${errorText})\n`
    );
  }

  if (failed.length > 0) {
    process.exit(1);
  }
}

main().catch((error) => {
  process.stderr.write(`[concurrency-smoke] fatal: ${error?.stack ?? String(error)}\n`);
  terminateChildren('SIGTERM');
  process.exit(1);
});
