import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;
const codeMirrorPackages = [
  "autocomplete",
  "commands",
  "lang-json",
  "language",
  "legacy-modes",
  "lint",
  "search",
  "state",
  "theme-one-dark",
  "view",
];
const codeMirrorAliases = Object.fromEntries(
  codeMirrorPackages.map((name) => [`@codemirror/${name}`, path.resolve(process.cwd(), "node_modules", "@codemirror", name)]),
);

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: codeMirrorAliases,
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws" as const,
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  optimizeDeps: {
    // Keep CodeMirror's state/view modules in one native ESM graph. Rolldown
    // otherwise inlines a second state copy into each dependent package.
    exclude: [
      "@codemirror/autocomplete",
      "@codemirror/commands",
      "@codemirror/lang-json",
      "@codemirror/language",
      "@codemirror/legacy-modes/mode/toml",
      "@codemirror/lint",
      "@codemirror/search",
      "@codemirror/state",
      "@codemirror/theme-one-dark",
      "@codemirror/view",
    ],
  },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
});
