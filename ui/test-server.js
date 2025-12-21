#!/usr/bin/env node

/**
 * Simple test server for AdapterOS UI development
 *
 * This server provides mock endpoints to test the UI without
 * needing the full Rust backend compiled and running.
 */

const express = require('express');
const cors = require('cors');
const path = require('path');
const fs = require('fs');

const app = express();
const PORT = process.env.PORT || 8080;

// Middleware
app.use(cors());
app.use(express.json());

// Log all requests
app.use((req, res, next) => {
  console.log(`${new Date().toISOString()} ${req.method} ${req.path}`);
  next();
});

// Serve static files from the built UI
const staticPath = path.join(__dirname, '../crates/adapteros-server/static-minimal');
if (fs.existsSync(staticPath)) {
  app.use(express.static(staticPath));
  console.log(`Serving static files from: ${staticPath}`);
} else {
  console.warn(`Static directory not found: ${staticPath}`);
  console.warn(`Build the UI first with: pnpm vite build --config vite.config.minimal.ts`);
}

// Mock data
const mockAdapters = [
  {
    id: 'adapter-1',
    name: 'Code Review Assistant',
    current_state: 'Unloaded',
    description: 'Specialized for Python and JavaScript code review',
    memory_mb: 0,
    activation_percent: 0
  },
  {
    id: 'adapter-2',
    name: 'Documentation Writer',
    current_state: 'Unloaded',
    description: 'Technical documentation and API reference generation',
    memory_mb: 0,
    activation_percent: 0
  },
  {
    id: 'adapter-3',
    name: 'Test Generator',
    current_state: 'Unloaded',
    description: 'Unit test and integration test generation',
    memory_mb: 0,
    activation_percent: 0
  }
];

// Track adapter states
const adapterStates = {};
mockAdapters.forEach(a => {
  adapterStates[a.id] = a.current_state;
});

// API Endpoints

// Health check
app.get('/health', (req, res) => {
  res.json({
    status: 'ok',
    timestamp: new Date().toISOString(),
    version: '0.1.0-test'
  });
});

// System info
app.get('/v1/system/info', (req, res) => {
  const totalMemoryGB = 16; // Mock 16GB total
  const usedMemoryGB = 4.2; // Mock usage
  const loadedAdapters = mockAdapters.filter(a =>
    adapterStates[a.id] === 'Hot' ||
    adapterStates[a.id] === 'Warm'
  ).length;

  res.json({
    memory_used_gb: usedMemoryGB,
    memory_total_gb: totalMemoryGB,
    memory_available_gb: totalMemoryGB - usedMemoryGB,
    adapters_loaded: loadedAdapters,
    system: 'mock-server',
    version: '0.1.0'
  });
});

// List adapters
app.get('/v1/adapters', (req, res) => {
  const adapters = mockAdapters.map(a => ({
    ...a,
    current_state: adapterStates[a.id] || 'Unloaded',
    memory_mb: adapterStates[a.id] === 'Hot' ? 200 :
               adapterStates[a.id] === 'Warm' ? 150 : 0
  }));

  res.json({
    adapters: adapters,
    total: adapters.length
  });
});

// Get single adapter
app.get('/v1/adapters/:id', (req, res) => {
  const adapter = mockAdapters.find(a => a.id === req.params.id);

  if (!adapter) {
    return res.status(404).json({
      error: 'Adapter not found',
      adapter_id: req.params.id
    });
  }

  res.json({
    ...adapter,
    current_state: adapterStates[adapter.id] || 'Unloaded'
  });
});

// Load adapter
app.post('/v1/adapters/:id/load', (req, res) => {
  const adapter = mockAdapters.find(a => a.id === req.params.id);

  if (!adapter) {
    return res.status(404).json({
      error: 'Adapter not found',
      adapter_id: req.params.id
    });
  }

  // Simulate loading delay
  setTimeout(() => {
    adapterStates[adapter.id] = 'Hot';
    console.log(`Loaded adapter: ${adapter.id} -> Hot`);
  }, 100);

  res.json({
    success: true,
    message: `Adapter ${adapter.name} loaded successfully`,
    adapter_id: adapter.id,
    new_state: 'Hot'
  });
});

// Unload adapter
app.post('/v1/adapters/:id/unload', (req, res) => {
  const adapter = mockAdapters.find(a => a.id === req.params.id);

  if (!adapter) {
    return res.status(404).json({
      error: 'Adapter not found',
      adapter_id: req.params.id
    });
  }

  adapterStates[adapter.id] = 'Unloaded';
  console.log(`Unloaded adapter: ${adapter.id} -> Unloaded`);

  res.json({
    success: true,
    message: `Adapter ${adapter.name} unloaded`,
    adapter_id: adapter.id,
    new_state: 'Unloaded'
  });
});

// Swap adapter
app.post('/v1/adapters/:id/swap', (req, res) => {
  const adapter = mockAdapters.find(a => a.id === req.params.id);

  if (!adapter) {
    return res.status(404).json({
      error: 'Adapter not found',
      adapter_id: req.params.id
    });
  }

  // Unload all other adapters and load this one
  Object.keys(adapterStates).forEach(id => {
    if (id !== adapter.id) {
      adapterStates[id] = 'Unloaded';
    }
  });
  adapterStates[adapter.id] = 'Hot';

  console.log(`Swapped to adapter: ${adapter.id}`);

  res.json({
    success: true,
    message: `Swapped to adapter ${adapter.name}`,
    adapter_id: adapter.id,
    new_state: 'Hot',
    swap_time_ms: 42  // Mock swap time
  });
});

// Generate text
app.post('/v1/generate', (req, res) => {
  const { prompt, max_tokens = 100, adapter_id } = req.body;

  if (!prompt) {
    return res.status(400).json({
      error: 'Prompt is required'
    });
  }

  // Check if adapter is loaded
  if (adapter_id && adapterStates[adapter_id] !== 'Hot') {
    return res.status(400).json({
      error: 'Adapter is not loaded',
      adapter_id: adapter_id,
      current_state: adapterStates[adapter_id] || 'Unloaded'
    });
  }

  // Generate mock response based on adapter
  let response = '';
  const adapterName = adapter_id ?
    mockAdapters.find(a => a.id === adapter_id)?.name :
    'Base Model';

  if (prompt.toLowerCase().includes('hello')) {
    response = `Hello! I'm the ${adapterName}. How can I help you today?`;
  } else if (prompt.toLowerCase().includes('code')) {
    response = `As the ${adapterName}, I can help you with:\n` +
               `1. Writing clean, maintainable code\n` +
               `2. Following best practices\n` +
               `3. Optimizing performance\n` +
               `\nWhat specific code task would you like assistance with?`;
  } else {
    response = `[${adapterName}] Processing your request: "${prompt}"\n\n` +
               `This is a mock response from the test server. ` +
               `In production, this would be generated by the actual LoRA adapter.`;
  }

  res.json({
    text: response,
    output: response,
    adapter_used: adapter_id || 'base',
    tokens_generated: Math.floor(response.length / 4),
    generation_time_ms: 123
  });
});

// Training endpoints (mock)
app.get('/v1/training/datasets', (req, res) => {
  res.json({
    datasets: [
      {
        id: 'dataset-1',
        name: 'Python Code Examples',
        size_mb: 45,
        examples: 1200,
        status: 'ready'
      }
    ],
    total: 1
  });
});

app.get('/v1/training/jobs', (req, res) => {
  res.json({
    jobs: [
      {
        id: 'job-1',
        dataset_id: 'dataset-1',
        status: 'completed',
        progress_percent: 100,
        created_at: new Date(Date.now() - 3600000).toISOString()
      }
    ],
    total: 1
  });
});

// System metrics
app.get('/v1/system/metrics', (req, res) => {
  res.json({
    cpu_usage_percent: 12.5,
    memory_usage_percent: 26.3,
    gpu_usage_percent: 0,
    disk_usage_percent: 45.2,
    network_rx_mbps: 0.5,
    network_tx_mbps: 0.2,
    timestamp: new Date().toISOString()
  });
});

// Fallback for SPA routing - serve index.html for unknown routes
app.use((req, res) => {
  const indexPath = path.join(staticPath, 'index-minimal.html');
  if (fs.existsSync(indexPath)) {
    res.sendFile(indexPath);
  } else {
    res.status(404).json({
      error: 'Not found',
      path: req.path,
      message: 'Build the UI first with: pnpm vite build --config vite.config.minimal.ts'
    });
  }
});

// Error handling
app.use((err, req, res, next) => {
  console.error('Server error:', err);
  res.status(500).json({
    error: 'Internal server error',
    message: err.message
  });
});

// Start server
app.listen(PORT, () => {
  console.log('');
  console.log('='.repeat(60));
  console.log('AdapterOS Test Server');
  console.log('='.repeat(60));
  console.log(`Server running at: http://localhost:${PORT}`);
  console.log(`Minimal UI: http://localhost:${PORT}/index-minimal.html`);
  console.log(`API Test: http://localhost:${PORT}/api-test.html`);
  console.log('');
  console.log('Available endpoints:');
  console.log('  GET  /health');
  console.log('  GET  /v1/system/info');
  console.log('  GET  /v1/adapters');
  console.log('  GET  /v1/adapters/:id');
  console.log('  POST /v1/adapters/:id/load');
  console.log('  POST /v1/adapters/:id/unload');
  console.log('  POST /v1/adapters/:id/swap');
  console.log('  POST /v1/generate');
  console.log('  GET  /v1/training/datasets');
  console.log('  GET  /v1/training/jobs');
  console.log('  GET  /v1/system/metrics');
  console.log('');
  console.log('Press Ctrl+C to stop the server');
  console.log('='.repeat(60));
});