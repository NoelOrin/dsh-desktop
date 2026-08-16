/**
 * 桌面壳形态：把平台标记、拖动区和窗口控制从 Rust 注入迁移到 bridge client。
 *
 * bridge 被桌面壳通过 --patch 挂载时才存在 `window.__DSH_DESKTOP__`；普通 dsh web
 * 不会触发这些改动。
 */
import { getBridge } from "../../../client-kit/inject";
import { applyAdvancedModeMarker } from "./advanced/theme-presenter";

const CONTROLS_ID = "dsh-desktop-controls";
const DRAG_ID = "dsh-desktop-drag";
const MAC_TRAFFIC_WIDTH = 80;
const WINDOW_CONTROLS_WIDTH = 96;

export type DesktopPlatform = "darwin" | "win32" | "linux";

export type DesktopMode = "compatibility" | "advanced";

interface DesktopBridgeLike {
  platform?: string;
  windowAction(action: "minimize" | "maximize" | "close" | "toggle-visible"): Promise<void>;
  onWindowState(cb: (state: { maximized: boolean }) => void): Promise<() => void>;
}

export function resolveDesktopPlatform(raw: string | undefined): DesktopPlatform {
  const value = (raw ?? navigator.platform ?? "").toLowerCase();
  if (
    value.includes("mac") ||
    value.includes("darwin") ||
    value.includes("iphone") ||
    value.includes("ipad")
  ) {
    return "darwin";
  }
  if (value.includes("win")) {
    return "win32";
  }
  return "linux";
}

function findTopBar(): HTMLElement | null {
  for (const button of Array.from(document.querySelectorAll("button"))) {
    const label = `${button.getAttribute("aria-label") ?? ""} ${button.textContent ?? ""}`;
    if (/session\s*log/i.test(label)) {
      const header = button.closest("header");
      if (header instanceof HTMLElement) return header;
    }
  }
  for (const node of Array.from(document.querySelectorAll("header, [role=banner]"))) {
    const rect = node.getBoundingClientRect();
    if (rect.top <= 8 && rect.height >= 32 && rect.height <= 160) {
      return node instanceof HTMLElement ? node : null;
    }
  }
  return null;
}

function createDragStrip(): HTMLElement {
  let strip = document.getElementById(DRAG_ID);
  if (strip instanceof HTMLElement) return strip;
  strip = document.createElement("div");
  strip.id = DRAG_ID;
  strip.setAttribute("data-tauri-drag-region", "deep");
  document.body.appendChild(strip);
  return strip;
}

function createControls(bridge: DesktopBridgeLike | null): HTMLElement | null {
  let host = document.getElementById(CONTROLS_ID);
  if (host instanceof HTMLElement) return host;
  if (bridge === null) return null;
  const platform = resolveDesktopPlatform(bridge?.platform);
  if (platform === "darwin") return null;

  host = document.createElement("div");
  host.id = CONTROLS_ID;
  host.innerHTML = [
    '<button type="button" data-action="minimize" aria-label="最小化">' +
      '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2" y="5.4" width="8" height="1.2" rx="0.6" fill="currentColor"/></svg>' +
      "</button>",
    '<button type="button" data-action="maximize" aria-label="最大化">' +
      '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>' +
      "</button>",
    '<button type="button" data-action="close" aria-label="关闭">' +
      '<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3L3 9" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></svg>' +
      "</button>",
  ].join("");
  host.addEventListener("click", (event) => {
    const target = event.target;
    const button = target instanceof Element ? target.closest("[data-action]") : null;
    if (!(button instanceof HTMLElement)) return;
    const action = button.dataset.action as "minimize" | "maximize" | "close" | undefined;
    if (action !== undefined) void bridge.windowAction(action).catch(() => {});
  });
  document.body.appendChild(host);
  return host;
}

function updateMaximizeIcon(host: HTMLElement, maximized: boolean): void {
  const button = host.querySelector<HTMLElement>('[data-action="maximize"]');
  if (!button) return;
  button.innerHTML = maximized
    ? '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="3.4" y="2.2" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/><rect x="2.2" y="3.6" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/></svg>'
    : '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>';
  button.setAttribute("aria-label", maximized ? "还原" : "最大化");
}

function attachTopBar(bar: HTMLElement, platform: DesktopPlatform, hasControls: boolean): void {
  if (platform === "darwin") {
    const previous = parseFloat(bar.style.paddingLeft) || 0;
    bar.style.paddingLeft = `${Math.max(previous, MAC_TRAFFIC_WIDTH)}px`;
    return;
  }
  bar.setAttribute("data-tauri-drag-region", "deep");
  const strip = document.getElementById(DRAG_ID);
  if (strip) strip.style.pointerEvents = "none";
  const reserved = hasControls ? WINDOW_CONTROLS_WIDTH : 0;
  const previous = parseFloat(bar.style.paddingRight) || 0;
  bar.style.paddingRight = `${Math.max(previous, reserved)}px`;
}

/**
 * 应用桌面壳平台标记与标题栏形态。返回 disposer，插件 HMR 时由 cordis 清理。
 */
export function applyDesktopShell(
  mode: DesktopMode = "compatibility",
  platformHint?: string,
): () => void {
  if (typeof document === "undefined") return () => {};
  const bridge = getBridge<DesktopBridgeLike>();
  const platform = resolveDesktopPlatform(platformHint ?? bridge?.platform);
  const root = document.documentElement;
  root.dataset.dshDesktop = "true";
  root.dataset.dshDesktopPlatform = platform;
  root.dataset.dshDesktopMode = mode;

  if (mode === "advanced") {
    // 上游 ui-layout 的 AppFrame 已提供三栏、拖拽把手与 overlay；这里只切换
    // advanced 标记，不重复注册 root，避免 shadow 官方布局。
    const removeMarker = applyAdvancedModeMarker();
    return () => {
      removeMarker();
      root.removeAttribute("data-dsh-desktop");
      root.removeAttribute("data-dsh-desktop-platform");
      root.removeAttribute("data-dsh-desktop-mode");
    };
  }

  if (bridge === null) {
    return () => {
      root.removeAttribute("data-dsh-desktop");
      root.removeAttribute("data-dsh-desktop-platform");
      root.removeAttribute("data-dsh-desktop-mode");
    };
  }

  const controls = createControls(bridge);
  const drag = createDragStrip();
  let topBar = findTopBar();
  if (topBar) attachTopBar(topBar, platform, controls !== null);

  const observer = new MutationObserver(() => {
    const current = findTopBar();
    if (current && current !== topBar) {
      topBar = current;
      attachTopBar(current, platform, controls !== null);
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });

  let unlistenWindowState: (() => void) | null = null;
  if (controls && bridge?.onWindowState) {
    bridge
      .onWindowState((state: { maximized: boolean }) =>
        updateMaximizeIcon(controls, state.maximized),
      )
      .then((unlisten: () => void) => {
        unlistenWindowState = unlisten;
      })
      .catch(() => {});
  }

  return () => {
    observer.disconnect();
    unlistenWindowState?.();
    controls?.remove();
    drag.remove();
    root.removeAttribute("data-dsh-desktop");
    root.removeAttribute("data-dsh-desktop-platform");
    root.removeAttribute("data-dsh-desktop-mode");
  };
}
