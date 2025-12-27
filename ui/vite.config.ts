/// <reference types="vite" />

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react-swc'
import tailwindcss from '@tailwindcss/vite'
import path from 'path';
import sharedAliases from './vite.aliases.json';

// Determine build mode from environment or command line
const getBuildMode = () => {
  // Check for explicit build mode flags
  if (process.env.VITE_BUILD_MODE === 'minimal') return 'minimal';
  if (process.env.VITE_BUILD_MODE === 'service-panel') return 'service-panel';
  if (process.argv.includes('--mode=minimal')) return 'minimal';
  if (process.argv.includes('--mode=service-panel')) return 'service-panel';

  // Check for config file flag (backwards compatibility)
  const configArg = process.argv.find(arg => arg.includes('--config'));
  if (configArg?.includes('minimal')) return 'minimal';
  if (configArg?.includes('service-panel')) return 'service-panel';

  return 'default';
};

const buildMode = getBuildMode();

// Build shared aliases with path resolution
const buildAliases = () => {
  const aliases: Record<string, string> = {
    ...sharedAliases,
    '@': path.resolve(__dirname, './src'),
  };

  // Service panel needs react-mermaid shim
  if (buildMode === 'service-panel') {
    aliases['react-mermaid'] = path.resolve(__dirname, './src/shims/react-mermaid.tsx');
  }

  return aliases;
};

// Build configuration based on mode
const getBuildConfig = () => {
  switch (buildMode) {
    case 'minimal':
      return {
        outDir: '../crates/adapteros-server/static-minimal',
        emptyOutDir: true,
        rollupOptions: {
          input: {
            main: path.resolve(__dirname, 'index-minimal.html'),
          },
        },
        minify: 'terser',
        terserOptions: {
          compress: {
            drop_console: true,
            drop_debugger: true,
          },
        },
        chunkSizeWarningLimit: 1000,
        reportCompressedSize: true,
      };

    case 'service-panel':
      return {
        outDir: 'dist-service-panel',
        emptyOutDir: true,
      };

    default:
      return {
        target: 'esnext',
        outDir: '../crates/adapteros-server/static',
        emptyOutDir: true,
        minify: 'terser',
        terserOptions: {
          compress: {
            drop_console: true,
            drop_debugger: true,
          },
        },
        rollupOptions: {
          output: {
            manualChunks: (id) => {
              if (id.includes('node_modules/react') || id.includes('node_modules/react-dom')) {
                return 'react-vendor';
              }
              if (id.includes('@radix-ui')) {
                return 'radix-ui';
              }
              if (id.includes('recharts') || id.includes('d3-')) {
                return 'charts';
              }
              if (id.includes('lucide-react')) {
                return 'icons';
              }
              if (id.includes('@tanstack/react-query')) {
                return 'react-query';
              }
              if (id.includes('node_modules')) {
                return 'vendor';
              }
            },
          },
        },
        chunkSizeWarningLimit: 1000,
      };
  }
};

// Server configuration based on mode
const getServerConfig = () => {
  switch (buildMode) {
    case 'minimal':
      return {
        port: parseInt(process.env.AOS_UI_PORT || '3200', 10),
        strictPort: true,
        proxy: {
          '/api': {
            target: `http://localhost:${process.env.AOS_SERVER_PORT || '8080'}`,
            changeOrigin: true,
            secure: false,
          },
          '/v1': {
            target: `http://localhost:${process.env.AOS_SERVER_PORT || '8080'}`,
            changeOrigin: true,
            secure: false,
            rewrite: (path) => `/api${path}`,
          },
        },
      };

    case 'service-panel':
      return {
        port: parseInt(process.env.AOS_PANEL_PORT || '3300', 10),
        strictPort: true,
        host: '0.0.0.0',
        open: false,
        proxy: {
          '/api/services': {
            target: `http://localhost:${parseInt(process.env.AOS_PANEL_PORT || '3301', 10)}`,
            changeOrigin: true,
            secure: false,
          },
          '/api': {
            target: `http://localhost:${process.env.AOS_SERVER_PORT || '8080'}`,
            changeOrigin: true,
            secure: false,
          },
        },
        headers: {
          'Content-Security-Policy': "default-src * 'unsafe-inline' 'unsafe-eval'; script-src * 'unsafe-inline' 'unsafe-eval'; style-src * 'unsafe-inline'; img-src * data:; font-src * data:; connect-src * ws: wss:;",
          'X-Frame-Options': 'DENY',
          'X-Content-Type-Options': 'nosniff',
          'Referrer-Policy': 'strict-origin-when-cross-origin',
          'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
          'Cache-Control': 'no-cache, no-store, must-revalidate',
          'Pragma': 'no-cache',
          'Expires': '0',
        },
      };

    default:
      return {
        port: parseInt(process.env.AOS_UI_PORT || '3200', 10),
        host: true,
        strictPort: true,
        open: true,
        proxy: {
          '/api': {
            target: `http://localhost:${process.env.AOS_SERVER_PORT || '8080'}`,
            changeOrigin: true,
            secure: false,
            cookieDomainRewrite: 'localhost',
            ws: true,
            configure: (proxy, _options) => {
              proxy.on('error', (err, _req, _res) => {
                console.log('proxy error', err);
              });
              proxy.on('proxyReq', (proxyReq, req, _res) => {
                if (req.headers.cookie) {
                  proxyReq.setHeader('Cookie', req.headers.cookie);
                }
              });
              proxy.on('proxyRes', (proxyRes, req, _res) => {
                if (req.url?.includes('/stream/')) {
                  proxyRes.headers['cache-control'] = 'no-cache';
                  proxyRes.headers['connection'] = 'keep-alive';
                  proxyRes.headers['x-accel-buffering'] = 'no';
                }
              });
            },
          },
          '/v1': {
            target: `http://localhost:${process.env.AOS_SERVER_PORT || '8080'}`,
            changeOrigin: true,
            secure: false,
            rewrite: (path) => `/api${path}`,
            configure: (proxy, _options) => {
              proxy.on('error', (err, _req, _res) => {
                console.log('proxy error', err);
              });
              proxy.on('proxyReq', (proxyReq, req, _res) => {
                if (req.headers.cookie) {
                  proxyReq.setHeader('Cookie', req.headers.cookie);
                }
              });
              proxy.on('proxyRes', (proxyRes, req, _res) => {
                if (req.url?.includes('/stream/')) {
                  proxyRes.headers['cache-control'] = 'no-cache';
                  proxyRes.headers['connection'] = 'keep-alive';
                  proxyRes.headers['x-accel-buffering'] = 'no';
                }
              });
            },
          },
        },
        headers: {
          'Content-Security-Policy': "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data:; connect-src 'self' ws: wss: http://localhost:* http://127.0.0.1:*;",
          'X-Frame-Options': 'DENY',
          'X-Content-Type-Options': 'nosniff',
          'Referrer-Policy': 'strict-origin-when-cross-origin',
          'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
          'Cache-Control': 'no-cache, no-store, must-revalidate',
          'Pragma': 'no-cache',
          'Expires': '0',
        },
      };
  }
};

// Plugin configuration based on mode
const getPlugins = () => {
  // Minimal mode doesn't use Tailwind CSS
  if (buildMode === 'minimal') {
    return [react()];
  }
  return [tailwindcss(), react()];
};

// Optimize deps configuration
const getOptimizeDeps = () => {
  if (buildMode === 'service-panel') {
    return {
      entries: ['service-panel.html'],
    };
  }
  return undefined;
};

// https://vitejs.dev/config/
export default defineConfig({
  plugins: getPlugins(),
  root: buildMode === 'service-panel' ? '.' : undefined,
  base: buildMode === 'minimal' ? '/' : undefined,
  resolve: {
    extensions: ['.js', '.jsx', '.ts', '.tsx', '.json'],
    alias: buildAliases(),
  },
  build: getBuildConfig(),
  server: getServerConfig(),
  optimizeDeps: getOptimizeDeps(),
});
