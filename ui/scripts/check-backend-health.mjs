#!/usr/bin/env node
// Quick helper to verify the control plane health endpoint used by E2E runs.
const backendHost = process.env.AOS_SERVER_HOST ?? '127.0.0.1';
const backendPort = process.env.AOS_SERVER_PORT ?? process.env.AOS_SERVER__PORT ?? '8080';
const healthPath = process.env.AOS_BACKEND_HEALTH_PATH ?? '/readyz'; // Public and unauthenticated
const healthUrl = `http://${backendHost}:${backendPort}${healthPath}`;

async function main() {
  console.log(`Pinging backend health at ${healthUrl} ...`);
  try {
    const res = await fetch(healthUrl, { method: 'GET' });
    const text = await res.text();
    console.log(`Status: ${res.status}`);
    console.log(text ? text.slice(0, 400) : '<empty body>');
    if (!res.ok) {
      throw new Error(`Health check returned HTTP ${res.status}`);
    }
  } catch (err) {
    console.error(
      `Health check failed. Ensure the dev server is running (pnpm dev) or that port ${backendPort} is reachable.`,
    );
    console.error(err);
    process.exit(1);
  }
}

main();
