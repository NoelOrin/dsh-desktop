/** advanced 模式把上游 ThemeSnapshot 投影到 document 的最小 presenter。 */
export interface ThemeSnapshot {
  colorScheme: "light" | "dark";
  darkMarker?: boolean;
  tokens?: Record<string, string>;
  themeColor?: string;
}

const DATA_ATTRIBUTE = "data-dsh-theme";

/** 应用主题快照；dispose 时只清理本 presenter 写入的属性。 */
export function applyThemePresenter(snapshot: ThemeSnapshot): () => void {
  const root = document.documentElement;
  root.setAttribute(DATA_ATTRIBUTE, snapshot.colorScheme);
  root.style.setProperty("--dsh-theme-scheme", snapshot.colorScheme);
  if (snapshot.themeColor) {
    root.style.setProperty("--dsh-theme-color", snapshot.themeColor);
  }
  for (const [name, value] of Object.entries(snapshot.tokens ?? {})) {
    root.style.setProperty(`--dsh-theme-${name}`, value);
  }
  return () => {
    root.removeAttribute(DATA_ATTRIBUTE);
    root.style.removeProperty("--dsh-theme-scheme");
    root.style.removeProperty("--dsh-theme-color");
    for (const name of Object.keys(snapshot.tokens ?? {})) {
      root.style.removeProperty(`--dsh-theme-${name}`);
    }
  };
}

/** 从 URL 查询读取 advanced 模式参数。 */
export function resolveAdvancedParams(search: string): {
  mode: "compatibility" | "advanced";
  platform: "darwin" | "win32" | "linux";
} {
  const params = new URLSearchParams(search);
  const mode = params.get("dsh-desktop-mode") === "advanced" ? "advanced" : "compatibility";
  const rawPlatform = params.get("dsh-desktop-platform") ?? "";
  const platform =
    rawPlatform === "darwin" || rawPlatform === "win32" || rawPlatform === "linux"
      ? rawPlatform
      : "linux";
  return { mode, platform };
}

/** 为 advanced 模式设置 body 标记；返回 disposer。 */
export function applyAdvancedModeMarker(): () => void {
  document.body.classList.add("dsh-desktop-advanced");
  return () => document.body.classList.remove("dsh-desktop-advanced");
}
