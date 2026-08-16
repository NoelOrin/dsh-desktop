import type { Context } from "@deepseek-ai/cordis";
import { settingsNamespace } from "@deepseek-ai/dsh-settings";
import z from "@deepseek-ai/schemastery";
import type {
  DesktopProfileState,
  DesktopSettings,
  DshConfig,
  DshProfileSummary,
  InstalledPluginSummary,
  PluginOperationResult,
  ProfileSelectionResult,
  RemotePluginPreset,
  RuntimeSnapshot,
  WindowAction,
} from "@dsh-desktop/contracts";

// 桥接 Tauri 壳能力的 dsh 插件（骨架）。
// 目标：让 dsh web 内的插件/页面能调用 Tauri 壳能力——桥接命令契约由本插件自持，
// Rust 侧在 apps/shell/src-tauri/capabilities/bridge.json 声明实现与权限，
// 白名单见 docs/plugin-tauri-boundary.md。契约与 packages/contracts（native）分离。
export const name = "bridge";

/** 壳注入到 dsh web 页面的受控桥接 API（window.__DSH_DESKTOP__）。 */
export interface DshDesktopBridge {
  openExternal(target: string): Promise<void>;
  /** 仅包装 native `window_action` 命令切片，不暴露 Tauri `core:window` 全量 API。 */
  windowAction(action: WindowAction): Promise<void>;
  onWindowState(cb: (state: { maximized: boolean }) => void): Promise<() => void>;
  getStatus(): Promise<RuntimeSnapshot>;
  restart(): Promise<void>;
  installDsh(): Promise<void>;
  openLogDirectory(): Promise<void>;
  getConfig(): Promise<DshConfig>;
  setConfig(config: DshConfig): Promise<void>;
  onStatus(cb: (snapshot: RuntimeSnapshot) => void): Promise<() => void>;
  onLog(cb: (line: string) => void): Promise<() => void>;
  autostart: {
    get(): Promise<boolean>;
    set(enabled: boolean): Promise<void>;
  };
  desktop: {
    get(): Promise<DesktopSettings>;
    set(settings: DesktopSettings): Promise<void>;
  };
  /** profile 发现与切换；active 为状态机快照，pending 需重启后生效。 */
  profiles: {
    list(): Promise<DshProfileSummary[]>;
    active(): Promise<DesktopProfileState>;
    select(name: string): Promise<ProfileSelectionResult>;
  };
  /** 远程插件预设：启动时自动安装到 active profile。 */
  remotePlugins: {
    list(): Promise<RemotePluginPreset[]>;
    save(presets: RemotePluginPreset[]): Promise<RemotePluginPreset[]>;
  };
  /** 受管 dsh 插件操作；profile 由 Rust 自动取 active。 */
  plugins: {
    installed(): Promise<InstalledPluginSummary[]>;
    install(spec: string): Promise<PluginOperationResult>;
    remove(name: string): Promise<PluginOperationResult>;
    update(): Promise<PluginOperationResult>;
    sync(group?: string): Promise<PluginOperationResult[]>;
  };
  shortcuts: {
    register(shortcut: string, cb?: () => void): Promise<() => void>;
    unregister(shortcut: string): Promise<void>;
    list(): Promise<Array<{ shortcut: string; registered: boolean }>>;
    unregisterAll(): Promise<void>;
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
  const DESKTOP_NS = settingsNamespace("dsh-desktop");
  ctx.settings.register(
    DESKTOP_NS,
    z.object({
      mode: z.union(["compatibility", "advanced"] as const).default("compatibility"),
    }),
    { applies: "restart" },
  );
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
