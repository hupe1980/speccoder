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
