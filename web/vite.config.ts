import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// `bun run dev` proxies the API to a running `picle serve` on the default port.
export default defineConfig({
  plugins: [svelte()],
  server: { proxy: { "/api": "http://127.0.0.1:7420" } },
});
