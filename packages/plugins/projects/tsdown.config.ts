import { defineConfig } from "tsdown";

export default defineConfig([
  {
    // host 面：Node ESM，只提供侧边栏右键菜单所需的端点
    name: "@dsh-desktop/plugin-projects",
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
