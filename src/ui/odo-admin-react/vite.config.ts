import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// https://vite.dev/config/
export default defineConfig({
  // The container image builds with a placeholder base and substitutes the
  // real one at startup (docker-entrypoint.d/40-base-path.sh), so one image
  // can serve at /odo/admin or at a host's root. The router reads it back
  // as import.meta.env.BASE_URL. The APIs stay at the host root either way.
  base: process.env.VITE_BASE_PATH || "/",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  // Listen on all interfaces and accept any Host header on the dev and
  // preview servers. This turns off Vite's DNS-rebinding guard, which is
  // acceptable for a dev server on a trusted network; hostname policy for
  // real traffic belongs to Envoy.
  server: {
    host: true,
    allowedHosts: true,
    // The dev cluster's gateway; same target as the Angular app's proxy.
    proxy: {
      "/api": {
        target: process.env.ODO_API_TARGET || "http://localhost:30080",
        changeOrigin: true,
        cookieDomainRewrite: "",
      },
    },
  },
  preview: { host: true, allowedHosts: true },
})
