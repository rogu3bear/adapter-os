/// <reference types="vite" />

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react-swc'
import path from 'path';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 3200,
    host: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3300',
        changeOrigin: true,
        secure: false,
      },
    },
    build: {
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
            // Split react vendor bundle
            if (id.includes('node_modules/react') || id.includes('node_modules/react-dom')) {
              return 'react-vendor';
            }
            // Split radix UI components
            if (id.includes('@radix-ui')) {
              return 'radix-ui';
            }
            // Split charts library
            if (id.includes('recharts') || id.includes('d3-')) {
              return 'charts';
            }
            // Split icons
            if (id.includes('lucide-react')) {
              return 'icons';
            }
            // Split react query
            if (id.includes('@tanstack/react-query')) {
              return 'react-query';
            }
            // Keep other node_modules as general vendor
            if (id.includes('node_modules')) {
              return 'vendor';
            }
          },
        },
      },
      chunkSizeWarningLimit: 1000,
    },
    server: {
      port: 3200,
      strictPort: true,
      open: true,
      proxy: {
        '/api': {
          target: 'http://localhost:3300',
          changeOrigin: true,
          secure: false,
          cookieDomainRewrite: 'localhost',
          ws: true, // Enable WebSocket/SSE support
          configure: (proxy, _options) => {
            proxy.on('error', (err, _req, res) => {
              console.log('proxy error', err);
            });
            proxy.on('proxyReq', (proxyReq, req, _res) => {
              // Ensure cookies are forwarded
              if (req.headers.cookie) {
                proxyReq.setHeader('Cookie', req.headers.cookie);
              }
            });
            // Handle SSE streams properly
            proxy.on('proxyRes', (proxyRes, req, res) => {
              // Set headers for SSE
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
    },
  });
