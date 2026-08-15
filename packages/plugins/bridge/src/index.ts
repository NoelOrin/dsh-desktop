import type { Context } from "@deepseek-ai/cordis";
import type {
  DesktopSettings,
  DshConfig,
  FileDropPayload,
  NotificationActionPayload,
  ProjectEntry,
  ProjectWorktreeResult,
  RuntimeSnapshot,
} from "@dsh-desktop/contracts";

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

export interface DeepLinkPayload {
  id: string;
  url: string;
  raw: string;
  received_at: string;
  source: "deep_link" | "second_instance";
  args: string[];
  cwd: string;
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
  windowAction(action: "minimize" | "maximize" | "close" | "toggle-visible"): Promise<void>;
  onWindowState(cb: (state: { maximized: boolean }) => void): Promise<() => void>;
  getStatus(): Promise<RuntimeSnapshot>;
  restart(): Promise<void>;
  installDsh(): Promise<void>;
  updateDsh(): Promise<void>;
  openLogDirectory(): Promise<void>;
  openPaths(paths: string[]): Promise<void>;
  importPaths(paths: string[]): Promise<void>;
  getConfig(): Promise<DshConfig>;
  setConfig(config: DshConfig): Promise<void>;
  onStatus(cb: (snapshot: RuntimeSnapshot) => void): Promise<() => void>;
  onLog(cb: (line: string) => void): Promise<() => void>;
  onFileDrop(cb: (payload: FileDropPayload) => void): Promise<() => void>;
  getPendingDeepLinks(): Promise<DeepLinkPayload[]>;
  ackDeepLink(id: string): Promise<void>;
  onDeepLink(cb: (payload: DeepLinkPayload) => void): Promise<() => void>;
  requestNotificationPermission(): Promise<"granted" | "prompt" | "denied">;
  onNotificationAction(cb: (payload: NotificationActionPayload) => void): Promise<() => void>;
  autostart: {
    get(): Promise<boolean>;
    set(enabled: boolean): Promise<void>;
  };
  desktop: {
    get(): Promise<DesktopSettings>;
    set(settings: DesktopSettings): Promise<void>;
  };
  shortcuts: {
    register(shortcut: string, cb?: () => void): Promise<() => void>;
    unregister(shortcut: string): Promise<void>;
    list(): Promise<Array<{ shortcut: string; registered: boolean }>>;
    unregisterAll(): Promise<void>;
  };
  onShortcut(cb: (shortcut: string) => void): Promise<() => void>;
  projects: {
    list(): Promise<ProjectEntry[]>;
    add(path: string): Promise<ProjectEntry>;
    update(project: ProjectEntry): Promise<ProjectEntry>;
    remove(id: string): Promise<void>;
    setPinned(id: string, pinned: boolean): Promise<ProjectEntry>;
    markRead(id: string): Promise<ProjectEntry>;
    setArchived(id: string, archived: boolean): Promise<ProjectEntry>;
    createWorktree(id: string): Promise<ProjectWorktreeResult>;
    showInFinder(id: string): Promise<void>;
  };
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

interface BridgeHealthContext {
  inject(
    dependencies: never,
    callback: (ctx: {
      webServer: {
        register(route: {
          kind: "exact";
          path: string;
          handler: (
            req: unknown,
            res: {
              writeHead(code: number, headers?: Record<string, string>): void;
              end(body?: string): void;
            },
          ) => void | Promise<void>;
        }): () => void;
      };
    }) => void,
  ): unknown;
}

export function apply(ctx: Context): void {
  // 固定健康端点由桥接插件提供，壳侧用 HTTP 探测 dsh web 是否真正就绪。
  (ctx as unknown as BridgeHealthContext).inject(["webServer"] as never, (sctx) => {
    sctx.webServer.register({
      kind: "exact",
      path: "/dsh-desktop/health",
      handler(_req, res) {
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ ok: true }));
      },
    });
  });
}
