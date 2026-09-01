import { defineConfig } from "vitest/config";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: {
    strictPort: true,
    port: 1420,
    watch: {
      ignored: ["**/src-tauri/target/**"],
    },
  },
  test: {
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    exclude: [
      "**/node_modules/**",
      "**/dist/**",
      "**/.worktrees/**",
      "**/.cargo-target-clipboard/**",
      "**/target/**",
      "**/src-tauri/**",
    ],
  },
  envPrefix: ["VITE_", "TAURI_"],
});
