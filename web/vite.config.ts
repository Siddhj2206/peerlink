/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and ignores Vite's clear-screen noise.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  test: {
    environment: "jsdom",
    globals: true,
  },
});
