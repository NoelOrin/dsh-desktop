/**
 * dsh client module 的样式注入约定：CSS 文本随 client bundle 打包，并在 factory
 * 物化时以 <style data-plugin-css> 注入，由 dsh-client-modules 记录与清理。
 */
const STYLE_ID = "@dsh-desktop/plugin-bridge/ui";

if (
  typeof document !== "undefined" &&
  document.querySelector(`style[data-plugin-css=${JSON.stringify(STYLE_ID)}]`) === null
) {
  const style = document.createElement("style");
  style.dataset.plugin = "@dsh-desktop/plugin-bridge";
  style.dataset.pluginCss = STYLE_ID;
  style.textContent = "__DSH_PLUGIN_CSS_PLACEHOLDER__";
  document.head.appendChild(style);
}
