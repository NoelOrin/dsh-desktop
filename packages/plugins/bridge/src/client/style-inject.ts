/** 让 dsh web 直接加载 host 提供的 style.css 文件。 */

const STYLE_ID = "@dsh-desktop/plugin-bridge/ui";

let injected = false;

export function ensureUiStyle(): void {
  if (typeof document === "undefined" || injected) return;
  injected = true;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = "/dsh-desktop/style.css";
  link.dataset.plugin = "@dsh-desktop/plugin-bridge";
  link.dataset.pluginCss = STYLE_ID;
  document.head.appendChild(link);
}
