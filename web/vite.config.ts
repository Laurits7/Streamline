import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// In dev, Vite serves the UI and proxies the API to the Rust server (`cargo run`).
const backend = process.env.STREAMLINE_BACKEND ?? 'http://127.0.0.1:3000'

export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': { target: backend, changeOrigin: false },
    },
  },
  build: {
    target: 'es2022',
    cssCodeSplit: false,
    reportCompressedSize: true,
  },
})
