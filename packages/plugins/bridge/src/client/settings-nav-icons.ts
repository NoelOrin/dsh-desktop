/**
 * 设置菜单图标补齐：上游设置壳只按固定 section id 映射图标，
 * 没有为 desktop-plugins 提供插件图标；这里在 bridge 层标记所有插件相关菜单/标签/卡片。
 */
const PLUGIN_NAV_LABEL_RE = /插件|plugin/i;

const SETTINGS_NAV_BUTTON_SELECTOR = 'button[class*="navCell"], button[role="tab"]';
const ICON_ATTR = "data-dsh-settings-nav-icon";
const PLUGIN_CARD_SELECTOR =
  'button[aria-label^="展开设置:"], button[aria-label^="收起设置:"], button[aria-label^="Expand settings:"], button[aria-label^="Collapse settings:"], button[class*="cardContent"]';
const PLUGIN_CARD_ATTR = "data-dsh-settings-plugin-card";

function navButtonLabel(button: HTMLButtonElement): string {
  const label = button.querySelector<HTMLElement>('[class*="navLabel"]');
  return (label?.textContent ?? button.textContent ?? "").trim();
}

function syncSettingsNavIcons(): void {
  document.querySelectorAll<HTMLButtonElement>(SETTINGS_NAV_BUTTON_SELECTOR).forEach((button) => {
    if (PLUGIN_NAV_LABEL_RE.test(navButtonLabel(button))) {
      button.setAttribute(ICON_ATTR, "plugin");
    } else if (button.hasAttribute(ICON_ATTR)) {
      button.removeAttribute(ICON_ATTR);
    }
  });
  document.querySelectorAll<HTMLButtonElement>(PLUGIN_CARD_SELECTOR).forEach((button) => {
    button.setAttribute(PLUGIN_CARD_ATTR, "true");
  });
}

/** 为设置面板里的插件菜单项挂上图标标记；返回 disposer，供 HMR 清理。 */
export function applySettingsNavIcons(): () => void {
  if (typeof document === "undefined") return () => {};
  syncSettingsNavIcons();
  const observer = new MutationObserver(syncSettingsNavIcons);
  observer.observe(document.documentElement, { childList: true, subtree: true });
  return () => {
    observer.disconnect();
    document
      .querySelectorAll<HTMLButtonElement>(`${SETTINGS_NAV_BUTTON_SELECTOR}[${ICON_ATTR}]`)
      .forEach((button) => {
        button.removeAttribute(ICON_ATTR);
      });
    document.querySelectorAll<HTMLButtonElement>(`[${PLUGIN_CARD_ATTR}]`).forEach((button) => {
      button.removeAttribute(PLUGIN_CARD_ATTR);
    });
  };
}
