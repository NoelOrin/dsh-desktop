import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  base: "/control-center/",
  build: {
    outDir: "../../dist/control-center",
    emptyOutDir: true,
  },
  server: {
    port: 5174,
    strictPort: true,
  },
});
