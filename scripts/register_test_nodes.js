#!/usr/bin/env node

// Script to register test nodes for development/testing
const axios = require('axios');

// Port configuration - respects canonical pane defaults for multi-developer setups
const rawPaneBase = Number.parseInt(process.env.AOS_PORT_PANE_BASE || '18080', 10);
const paneBase = Number.isInteger(rawPaneBase) && rawPaneBase > 0 && rawPaneBase <= 65523
  ? rawPaneBase
  : 18080;
const BACKEND_PORT = process.env.AOS_SERVER_PORT || String(paneBase);
const API_BASE = `http://localhost:${BACKEND_PORT}`;

async function registerNode(hostname, metalFamily, memoryGb, agentEndpoint) {
  try {
    const response = await axios.post(`${API_BASE}/v1/nodes/register`, {
      hostname,
      metal_family: metalFamily,
      memory_gb: memoryGb,
      agent_endpoint: agentEndpoint
    });

    console.log(`✓ Registered node: ${hostname} (ID: ${response.data.id})`);
    return response.data;
  } catch (error) {
    console.error(`✗ Failed to register ${hostname}:`, error.response?.data || error.message);
    return null;
  }
}

async function main() {
  console.log('Registering test nodes...\n');

  // Register some test nodes
  const nodes = [
    { hostname: 'worker-node-01.local', metalFamily: 'apple_silicon', memoryGb: 16, agentEndpoint: 'http://worker-node-01.local:8081' },
    { hostname: 'worker-node-02.local', metalFamily: 'apple_silicon', memoryGb: 32, agentEndpoint: 'http://worker-node-02.local:8081' },
    { hostname: 'compute-node-01.local', metalFamily: 'apple_silicon', memoryGb: 64, agentEndpoint: 'http://compute-node-01.local:8081' },
  ];

  for (const node of nodes) {
    await registerNode(node.hostname, node.metalFamily, node.memoryGb, node.agentEndpoint);
    // Small delay between registrations
    await new Promise(resolve => setTimeout(resolve, 500));
  }

  console.log('\nTest node registration complete!');
  console.log('You can now test the CommandPalette node search functionality.');
}

if (require.main === module) {
  main().catch(console.error);
}

module.exports = { registerNode };
