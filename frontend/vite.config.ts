import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  define: {
    // Excalidraw (et certaines libs npm) référencent process.env.NODE_ENV
    // qui n'existe pas dans le browser/WebView Tauri — on le polyfille ici.
    'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV ?? 'production'),
    // Excalidraw 0.17.x : main.js teste IS_PREACT avant de choisir le bundle.
    // Sans cette définition, esbuild laisse la branche indéterminée en mode dev.
    'process.env.IS_PREACT': JSON.stringify('false'),
  },

  server: {
    port: 3010,
    proxy: {
      '/api-proxy': {
        target: 'http://localhost:45678',
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyRes', (proxyRes) => {
            const ct = proxyRes.headers['content-type'] || '';
            if (ct.includes('text/event-stream')) {
              proxyRes.headers['x-accel-buffering'] = 'no';
              proxyRes.headers['cache-control']     = 'no-cache';
            }
          });
        },
      },
    },
  },
  build: {
    outDir: 'build',
    emptyOutDir: true,
  },
});
