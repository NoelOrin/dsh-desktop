/**
 * client 面注入约定：桥接对象与样式注入由多个 dsh 插件共用。
 * 本目录不是 dsh 插件，只提供源码级 helper；构建时随各插件 client bundle 内联。
 */

const BRIDGE_WINDOW_KEY = "__DSH_DESKTOP__";
const PLUGIN_CSS_PLACEHOLDER = "__DSH_PLUGIN_CSS_PLACEHOLDER__";

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
export function getBridge<T extends object>(): T | null {
  if (typeof window === "undefined") {
    return null;
  }
  const bridge = (window as unknown as Record<string, T | undefined>)[BRIDGE_WINDOW_KEY];
  return bridge ?? null;
}

/**
 * 按 dsh client module 的样式约定注入插件 CSS。
 * CSS 文本随 client bundle 打包，并在 factory 物化时以
 * `<style data-plugin-css>` 注入，由 dsh-client-modules 记录与清理。
 */
export function injectPluginCss(pluginName: string, styleId: string): void {
  if (typeof document === "undefined") {
    return;
  }
  const selector = `style[data-plugin-css=${JSON.stringify(styleId)}]`;
  if (document.querySelector(selector) !== null) {
    return;
  }
  const style = document.createElement("style");
  style.dataset.plugin = pluginName;
  style.dataset.pluginCss = styleId;
  style.textContent = PLUGIN_CSS_PLACEHOLDER;
  document.head.appendChild(style);
}
