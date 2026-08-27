import { defineConfig } from "vite";
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
  envPrefix: ["VITE_", "TAURI_"],
});
