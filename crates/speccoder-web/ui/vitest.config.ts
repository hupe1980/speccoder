import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The page's own tests: pure modules and components mounted in a DOM
// stand-in. Separate from vite.config.ts, so the bundle the binary embeds is
// built exactly as before; nothing here ships.
export default defineConfig({
  plugins: [svelte()],
  // Svelte's client runtime (`mount`), not its server one.
  resolve: { conditions: ["browser"] },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
  },
});
