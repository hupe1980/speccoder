import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Flat, stable names: the binary embeds dist/ as it is, and the host serves a
// file by its name or nothing (no directory walk to defend).
export default defineConfig({
  plugins: [svelte()],
  base: "/",
  build: {
    outDir: "dist",
    assetsDir: "",
    emptyOutDir: true,
    sourcemap: false,
    cssCodeSplit: false,
    // The browsers the page is for: the build adds the prefixes they need
    // (Safari's backdrop-filter) and nothing older.
    cssTarget: ["chrome111", "edge111", "firefox128", "safari16.4"],
    target: ["chrome111", "edge111", "firefox128", "safari16.4"],
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "[name].js",
        assetFileNames: "[name][extname]",
      },
    },
  },
  server: {
    // `npm run dev`: the API comes from a running `speccoder ui`.
    proxy: { "/api": "http://127.0.0.1:7377" },
  },
});
