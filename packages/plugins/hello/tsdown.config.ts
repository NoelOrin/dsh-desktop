import { defineConfig } from "tsdown";

export default defineConfig([
  {
    // host 面：Node ESM
    name: "@dsh-desktop/plugin-hello",
    entry: ["src/index.ts"],
    outDir: "lib",
    format: ["esm"],
    platform: "node",
    target: "es2024",
    fixedExtension: false,
    dts: false,
    clean: true,
  },
]);
