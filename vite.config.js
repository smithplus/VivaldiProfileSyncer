import { defineConfig } from 'vite'

export default defineConfig({
  clearScreen: false,
  root: 'src',
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: ['es2021', 'safari13'],
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    outDir: '../dist',
    emptyOutDir: true,
  },
})
