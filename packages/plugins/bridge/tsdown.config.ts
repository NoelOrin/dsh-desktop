import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "tsdown";

/** 浏览器平台模块表（复制自 deepseek-harness packages/client/web/src/platform.ts）。 */
const PLATFORM_MODULES = [
  "react",
  "react/jsx-runtime",
  "react-dom",
  "react-dom/client",
  "@deepseek-ai/cordis",
  "@deepseek-ai/dsh-client-ui-slots",
  "@deepseek-ai/dsh-client-web-react",
  "@deepseek-ai/dsh-client-ui-primitives",
  "@deepseek-ai/dsh-client-ui-attachment",
  "@deepseek-ai/dsh-client-schema-form",
];

/** 文档化的临时豁免（deepseek-harness 同款）。 */
const RUNTIME_STORE_EXEMPTION = "@deepseek-ai/dsh-client-runtime/client";

const CLIENT_EXTERNALS = [...PLATFORM_MODULES, RUNTIME_STORE_EXEMPTION];
const CLIENT_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "lib");
const UI_CSS_PLACEHOLDER = JSON.stringify("__DSH_PLUGIN_CSS_PLACEHOLDER__");

/**
 * 按 dsh client module 的样式约定，把构建后的 style.css 文本嵌入 client bundle；
 * style-inject.ts 会在 factory 物化时以 <style data-plugin-css> 注入。
 */
const injectClientCss = {
  name: "dsh-client-css-inject",
  closeBundle() {
    const clientPath = path.join(CLIENT_DIR, "client.js");
    const cssPath = path.join(CLIENT_DIR, "style.css");
    let client: string;
    try {
      client = readFileSync(clientPath, "utf8");
    } catch {
      return;
    }
    if (!client.includes(UI_CSS_PLACEHOLDER)) return;
    const css = readFileSync(cssPath, "utf8");
    writeFileSync(clientPath, client.replaceAll(UI_CSS_PLACEHOLDER, JSON.stringify(css)), "utf8");
  },
};

export default defineConfig([
  {
    // host 面：Node ESM
    name: "@dsh-desktop/plugin-bridge",
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
    name: "@dsh-desktop/plugin-bridge/client",
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
    plugins: [
      injectClientCss,
      {
        // purity gate：非平台模块的 @deepseek-ai 值导入直接构建失败
        name: "dsh-client-bundle-purity",
        resolveId(source: string) {
          if (!source.startsWith("@deepseek-ai/")) return null;
          if (CLIENT_EXTERNALS.includes(source)) return null;
          throw new Error(
            `client bundle purity: "${source}" is not a platform module - cross-plugin value imports are forbidden; collaborate through cordis services`,
          );
        },
      },
    ],
    outputOptions: {
      entryFileNames: "client.js",
      banner:
        'window.__ModuleLoader__.load({ id: "@dsh-desktop/plugin-bridge", factory: (require) => {',
      footer: "return module.exports; } });",
      intro: "var module = { exports: {} }; var exports = module.exports;",
    },
  },
  {
    // shared 面：纯函数共享模型，供 host 单测与 client 内联打包
    name: "@dsh-desktop/plugin-bridge/shared",
    entry: { shared: "src/shared/theme.ts" },
    outDir: "lib",
    format: ["esm"],
    platform: "neutral",
    target: "es2024",
    fixedExtension: false,
    dts: false,
    clean: false,
  },
]);
