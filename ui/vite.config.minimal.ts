/**
 * DEPRECATED: This config is superseded by vite.config.ts with VITE_BUILD_MODE=minimal
 *
 * Migration:
 *   Old: vite build --config vite.config.minimal.ts
 *   New: VITE_BUILD_MODE=minimal vite build
 *
 * This file is kept for backwards compatibility and will be removed in a future release.
 */

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';
import path from 'path';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  root: './',
  base: '/',
  build: {
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
        drop_debugger: true
      }
    },
    chunkSizeWarningLimit: 1000,
    reportCompressedSize: true
  },
  server: {
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
      },
    }
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
});