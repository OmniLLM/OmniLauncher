import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const isDebug = !!process.env.TAURI_DEBUG

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: ['es2021', 'chrome100', 'safari13'],
    minify: !isDebug,
    sourcemap: isDebug,
    cssCodeSplit: true,
    reportCompressedSize: false,
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (!id.includes('node_modules')) return
          if (id.includes('react-dom') || id.includes('/react/')) {
            return 'react-vendor'
          }
          if (id.includes('@tauri-apps')) {
            return 'tauri-vendor'
          }
        },
      },
    },
  },
})
