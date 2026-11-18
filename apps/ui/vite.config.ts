import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri loads the built files from dist/. No remote assets: keeps the web view offline.
export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist", target: "es2022" },
  server: { port: 5173, strictPort: true },
});
