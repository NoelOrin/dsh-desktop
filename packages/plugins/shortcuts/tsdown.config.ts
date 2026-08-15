import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "tsdown";
import { CLIENT_EXTERNALS, clientBundlePlugins } from "../client-kit/tsdown.ts";

const CLIENT_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "lib");

export default defineConfig([
  {
    // host 面：Node ESM，快捷键设置由 client 面提供
    name: "@dsh-desktop/plugin-shortcuts",
    entry: ["src/index.ts"],
    outDir: "lib",
    format: ["esm"],
    platform: "node",
    target: "es2024",
    fixedExtension: false,
    dts: false,
    clean: true,
  },
  {
    // client 面：浏览器 CJS + __ModuleLoader__ 包装（tsdown.client.ts 协议）
    name: "@dsh-desktop/plugin-shortcuts/client",
    entry: { client: "src/client.tsx" },
    outDir: "lib",
    format: "cjs",
    platform: "browser",
    dts: false,
    sourcemap: true,
    clean: false,
    external: CLIENT_EXTERNALS,
    define: {
      "process.env.NODE_ENV": JSON.stringify("production"),
      "import.meta.env.MODE": JSON.stringify("production"),
      "import.meta.env": JSON.stringify({ MODE: "production" }),
    },
    noExternal: (id: string) => (CLIENT_EXTERNALS.includes(id) ? undefined : true),
    plugins: clientBundlePlugins(CLIENT_DIR),
    outputOptions: {
      entryFileNames: "client.js",
      banner:
        'window.__ModuleLoader__.load({ id: "@dsh-desktop/plugin-shortcuts", factory: (require) => {',
      footer: "return module.exports; } });",
      intro: "var module = { exports: {} }; var exports = module.exports;",
    },
  },
]);
