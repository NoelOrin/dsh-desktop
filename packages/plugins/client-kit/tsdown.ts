import { readFileSync, writeFileSync } from "node:fs";

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

export const CLIENT_EXTERNALS = [...PLATFORM_MODULES, RUNTIME_STORE_EXEMPTION];

const UI_CSS_PLACEHOLDER = JSON.stringify("__DSH_PLUGIN_CSS_PLACEHOLDER__");

/**
 * 按 dsh client module 的样式约定，把构建后的 style.css 文本嵌入 client bundle；
 * `client-kit/inject.ts` 会在 factory 物化时以 `<style data-plugin-css>` 注入。
 */
function injectClientCss(clientDir: string) {
  return {
    name: "dsh-client-css-inject",
    closeBundle() {
      const clientPath = `${clientDir}/client.js`;
      const cssPath = `${clientDir}/style.css`;
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
}

/** client bundle 公共构建插件：CSS 内嵌 + 跨插件值导入 purity gate。 */
export function clientBundlePlugins(clientDir: string) {
  return [
    injectClientCss(clientDir),
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
  ];
}
