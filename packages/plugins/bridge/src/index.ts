import type { DshConfig, RuntimeSnapshot } from "@dsh-desktop/contracts";

// 桥接 Tauri 壳能力的 dsh 插件（骨架）。
// 目标：让 dsh web 内的插件/页面能调用 Tauri 壳能力——桥接命令契约由本插件自持，
// Rust 侧在 apps/shell/src-tauri/capabilities/bridge.json 声明实现与权限，
// 白名单见 docs/plugin-tauri-boundary.md。契约与 packages/contracts（native）分离。
export const name = "bridge";

/** 文件对话框选项（对应 dialog 插件 OpenDialogOptions，camelCase）。 */
export interface BridgeOpenDialogOptions {
  title?: string;
  multiple?: boolean;
  directory?: boolean;
  defaultPath?: string;
}

/** 壳注入到 dsh web 页面的受控桥接 API（window.__DSH_DESKTOP__）。 */
export interface DshDesktopBridge {
  platform: string;
  notify(title: string, body: string): Promise<void>;
  clipboard: {
    readText(): Promise<string>;
    writeText(text: string): Promise<void>;
  };
  dialog: {
    openFile(options?: BridgeOpenDialogOptions): Promise<string | string[] | null>;
    saveFile(options?: BridgeOpenDialogOptions): Promise<string | null>;
  };
  openExternal(target: string): Promise<void>;
  windowAction(action: "minimize" | "maximize" | "close"): Promise<void>;
  onWindowState(cb: (state: { maximized: boolean }) => void): Promise<() => void>;
  getStatus(): Promise<RuntimeSnapshot>;
  restart(): Promise<void>;
  installDsh(): Promise<void>;
  openLogDirectory(): Promise<void>;
  getConfig(): Promise<DshConfig>;
  setConfig(config: DshConfig): Promise<void>;
  onStatus(cb: (snapshot: RuntimeSnapshot) => void): Promise<() => void>;
  onLog(cb: (line: string) => void): Promise<() => void>;
  onFileDrop(cb: (paths: string[]) => void): Promise<() => void>;
  onDeepLink(cb: (url: string) => void): Promise<() => void>;
  autostart: {
    get(): Promise<boolean>;
    set(enabled: boolean): Promise<void>;
  };
  shortcuts: {
    register(shortcut: string, cb: () => void): Promise<void>;
    unregister(shortcut: string): Promise<void>;
  };
  onShortcut(cb: (shortcut: string) => void): Promise<() => void>;
  update: {
    check(): Promise<string | null>;
    /** 静默下载最新版安装包到本地，返回下载路径（由用户手动运行安装）。 */
    install(): Promise<string>;
  };
}

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
export function getBridge(): DshDesktopBridge | null {
  const bridge = (window as unknown as { __DSH_DESKTOP__?: DshDesktopBridge }).__DSH_DESKTOP__;
  return bridge ?? null;
}

export function apply(): void {
  // 骨架阶段：桥接能力由 Rust 侧注入（见 BRIDGE_SCRIPT），本插件只负责类型与入口；
  // 具体能力消费（如 pick_directory）在后续按最小切片扩展，不并入 packages/contracts。
}
