import { useEffect, useRef, useState, useSyncExternalStore } from "react";

/** 桥接对象最小切片（与 src/index.ts 的 DshDesktopBridge 对齐；client 侧内联读取 window，不跨侧 import）。 */
interface BridgeLike {
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
  update: {
    check(): Promise<string | null>;
    install(): Promise<void>;
  };
}

/** 运行状态快照（与 packages/contracts 的 RuntimeSnapshot 对齐；client 侧内联声明，不跨侧 import）。 */
interface RuntimeSnapshot {
  phase: string;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  log_dir: string | null;
  logs: string[];
}

/** 壳侧配置（与 packages/contracts 的 DshConfig 对齐）。 */
interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
}

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
function getBridge(): BridgeLike | null {
  const bridge = (window as unknown as { __DSH_DESKTOP__?: BridgeLike }).__DSH_DESKTOP__;
  return bridge ?? null;
}

/** desktop 设置命名空间的快照形状（与 host 侧 z.object 对齐）。 */
interface DesktopConfig {
  autostart?: boolean;
  dsh_bin?: string | null;
  dsh_node?: string | null;
  dsh_home?: string | null;
}

/** SettingsScope<T> 的最小形状：getSnapshot / subscribe / set（来自 @deepseek-ai/dsh-client-runtime/client）。 */
interface SettingsScopeLike<T> {
  getSnapshot(): { status: "loading" | "ready" | "unavailable"; value: T | undefined };
  subscribe(listener: () => void): () => void;
  set(field: string, value: unknown): Promise<void>;
}

/** 翻译函数形状（ctx.locale.bind 的返回）。 */
type Translate = (key: string) => string;

/**
 * client 面所需服务的本地结构类型。dsh client 服务（slots / locale / settingsScope / connection /
 * remote）由 dsh 生态注入，这里只声明本插件用到的面，避免跨侧 import host 类型。
 */
interface ClientContextLike {
  effect(effect: () => unknown, label?: string): void;
  locale: {
    bind(ns: string): Translate;
    register(ns: string, locale: string, dict: Record<string, string>): unknown;
  };
  settingsScope: {
    bind(spec: { namespace: string }): SettingsScopeLike<DesktopConfig>;
  };
  slots: {
    inject(key: string, callback: () => unknown): unknown;
    register(options: unknown, component: unknown): unknown;
  };
}

/** 开机自启开关：绑定 desktop 命名空间快照，切换时同步 Tauri 壳（壳不可达时仅写 settings）。 */
export function AutostartSwitch(props: {
  scope: SettingsScopeLike<DesktopConfig>;
  t: Translate;
}): JSX.Element {
  const snapshot = useSyncExternalStore(
    (listener) => props.scope.subscribe(listener),
    () => props.scope.getSnapshot(),
  );
  const value = snapshot.value?.autostart ?? false;
  const onChange = async (next: boolean) => {
    const bridge = getBridge();
    if (bridge) {
      try {
        await bridge.autostart.set(next);
      } catch {
        // 壳不可达或调用失败：仍写 settings（由下一次启动 / 桥接补偿）
      }
    }
    await props.scope.set("autostart", next);
  };
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 0" }}>
      <div style={{ flex: 1 }}>
        <div style={{ fontWeight: 600 }}>{props.t("autostart.title")}</div>
        <div style={{ fontSize: 12, opacity: 0.6 }}>{props.t("autostart.desc")}</div>
      </div>
      <input
        type="checkbox"
        checked={value}
        onChange={(e) => void onChange(e.target.checked)}
        aria-label={props.t("autostart.title")}
      />
    </div>
  );
}

const MAX_LOGS = 500;
const SHOW_LOGS = new Set(["installing", "starting", "failed"]);

/** 状态面板：展示 dsh 运行状态 / 日志与常用控制按钮（原控制中心 Dashboard）。 */
function StatusPanel({ t }: { t: Translate }): JSX.Element {
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const logsRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) return;
    bridge
      .getStatus()
      .then((s) => {
        if (disposed) return;
        setSnapshot(s);
        setLogs((s.logs ?? []).slice(-MAX_LOGS));
      })
      .catch((error: unknown) => {
        if (disposed) return;
        setSnapshot({
          phase: "failed",
          message: `无法读取运行状态: ${String(error)}`,
          url: null,
          dsh_installed: false,
          node_found: false,
          log_dir: null,
          logs: [],
        });
      });
    const un1 = bridge.onStatus((s) => {
      if (!disposed) setSnapshot(s);
    });
    const un2 = bridge.onLog((line) => {
      if (!disposed) setLogs((prev) => [...prev, line].slice(-MAX_LOGS));
    });
    return () => {
      disposed = true;
      void un1.then((u) => u()).catch(() => {});
      void un2.then((u) => u()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (logs.length && logsRef.current) {
      logsRef.current.scrollTop = logsRef.current.scrollHeight;
    }
  }, [logs]);

  const phase = snapshot?.phase ?? "detecting";
  const showLogs = SHOW_LOGS.has(phase);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: "8px 0" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background:
              phase === "ready"
                ? "#0f9d58"
                : phase === "failed"
                  ? "#d92d20"
                  : phase === "missing"
                    ? "#b7791f"
                    : "#3f63f4",
          }}
        />
        <span style={{ fontWeight: 600 }}>{snapshot?.message ?? "正在检测运行环境..."}</span>
        {snapshot?.url && <code style={{ fontSize: 12, opacity: 0.7 }}>{snapshot.url}</code>}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <button
          type="button"
          hidden={phase !== "missing" || snapshot?.node_found === false}
          onClick={() =>
            void getBridge()
              ?.installDsh()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.install")}
        </button>
        <button
          type="button"
          hidden={phase !== "failed"}
          onClick={() =>
            void getBridge()
              ?.restart()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.retry")}
        </button>
        <button
          type="button"
          hidden={phase !== "failed"}
          onClick={() =>
            void getBridge()
              ?.openLogDirectory()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.openLogs")}
        </button>
      </div>
      {showLogs && (
        <pre
          ref={logsRef}
          style={{
            margin: 0,
            maxHeight: 300,
            overflow: "auto",
            padding: "12px 14px",
            border: "1px solid var(--line, #e3e6eb)",
            borderRadius: 6,
            fontSize: 11,
            lineHeight: 1.55,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          {logs.join("\n")}
        </pre>
      )}
    </div>
  );
}

/** 配置面板：DSH_BIN / DSH_NODE / DSH_HOME 表单（原控制中心 Settings）。 */
function ConfigPanel({ t }: { t: Translate }): JSX.Element {
  const [dshBin, setDshBin] = useState("");
  const [dshNode, setDshNode] = useState("");
  const [dshHome, setDshHome] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    getBridge()
      ?.getConfig()
      .then((c) => {
        if (disposed) return;
        setDshBin(c.dsh_bin ?? "");
        setDshNode(c.dsh_node ?? "");
        setDshHome(c.dsh_home ?? "");
      })
      .catch((e: unknown) => {
        if (disposed) return;
        setError(`读取配置失败: ${String(e)}`);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const save = async () => {
    const config: DshConfig = {
      dsh_bin: dshBin || null,
      dsh_node: dshNode || null,
      dsh_home: dshHome || null,
    };
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      return;
    }
    try {
      await bridge.setConfig(config);
      setSaved(true);
      setError(null);
    } catch (e) {
      setSaved(false);
      setError(`保存失败: ${String(e)}`);
    }
  };

  return (
    <div
      style={{ display: "flex", flexDirection: "column", gap: 12, padding: "8px 0", maxWidth: 520 }}
    >
      <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: 13 }}>
        <span>DSH 入口 (DSH_BIN)</span>
        <input
          style={{
            padding: "8px 10px",
            border: "1px solid var(--line, #e3e6eb)",
            borderRadius: 5,
            fontSize: 13,
          }}
          value={dshBin}
          onInput={(e) => {
            setDshBin(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="例如 /path/to/dsh"
        />
      </label>
      <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: 13 }}>
        <span>Node 解释器 (DSH_NODE)</span>
        <input
          style={{
            padding: "8px 10px",
            border: "1px solid var(--line, #e3e6eb)",
            borderRadius: 5,
            fontSize: 13,
          }}
          value={dshNode}
          onInput={(e) => {
            setDshNode(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="例如 /path/to/node"
        />
      </label>
      <label style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: 13 }}>
        <span>Harness 数据目录 (DSH_HOME)</span>
        <input
          style={{
            padding: "8px 10px",
            border: "1px solid var(--line, #e3e6eb)",
            borderRadius: 5,
            fontSize: 13,
          }}
          value={dshHome}
          onInput={(e) => {
            setDshHome(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="留空则继承环境变量"
        />
      </label>
      <div style={{ display: "flex", gap: 8 }}>
        <button type="button" onClick={() => void save()}>
          {t("config.save")}
        </button>
        <button
          type="button"
          onClick={() =>
            void getBridge()
              ?.restart()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("config.restart")}
        </button>
      </div>
      {error && <p style={{ fontSize: 12, color: "#d92d20", margin: 0 }}>{error}</p>}
      {saved && <p style={{ fontSize: 12, opacity: 0.7, margin: 0 }}>{t("config.saved")}</p>}
    </div>
  );
}

/** 工具面板：打开日志目录 / 检查更新（原控制中心 Tools）。 */
function ToolsPanel({ t }: { t: Translate }): JSX.Element {
  const [msg, setMsg] = useState<string | null>(null);

  const doCheckUpdate = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setMsg("桌面壳桥接不可用");
      return;
    }
    try {
      const version = await bridge.update.check();
      if (!version) {
        setMsg("当前已是最新版本");
        return;
      }
      if (window.confirm(`发现新版本 ${version}，是否下载并安装？`)) {
        await bridge.update.install();
        setMsg("已安装，请重启应用");
      }
    } catch (e) {
      setMsg(`检查/安装更新失败: ${String(e)}`);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: "8px 0" }}>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 4,
          padding: "12px 14px",
          border: "1px solid var(--line, #e3e6eb)",
          borderRadius: 6,
        }}
      >
        <div style={{ fontWeight: 600, fontSize: 14 }}>{t("tools.openLogs.title")}</div>
        <div style={{ fontSize: 12, opacity: 0.6 }}>{t("tools.openLogs.desc")}</div>
        <button
          type="button"
          style={{ marginTop: 8, alignSelf: "flex-start" }}
          onClick={() =>
            void getBridge()
              ?.openLogDirectory()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("tools.openLogs.action")}
        </button>
      </div>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 4,
          padding: "12px 14px",
          border: "1px solid var(--line, #e3e6eb)",
          borderRadius: 6,
        }}
      >
        <div style={{ fontWeight: 600, fontSize: 14 }}>{t("tools.checkUpdate.title")}</div>
        <div style={{ fontSize: 12, opacity: 0.6 }}>{t("tools.checkUpdate.desc")}</div>
        <button
          type="button"
          style={{ marginTop: 8, alignSelf: "flex-start" }}
          onClick={() => void doCheckUpdate()}
        >
          {t("tools.checkUpdate.action")}
        </button>
        {msg && <p style={{ fontSize: 12, opacity: 0.7, margin: 0 }}>{msg}</p>}
      </div>
    </div>
  );
}

/** 桌面设置节整体内容：状态 + 配置 + 工具 + 开机自启。 */
function DesktopPanel(props: {
  scope: SettingsScopeLike<DesktopConfig>;
  t: Translate;
}): JSX.Element {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
      <section>
        <h3 style={{ fontSize: 15, margin: "0 0 8px" }}>{props.t("nav.status")}</h3>
        <StatusPanel t={props.t} />
      </section>
      <section>
        <h3 style={{ fontSize: 15, margin: "0 0 8px" }}>{props.t("nav.config")}</h3>
        <ConfigPanel t={props.t} />
      </section>
      <section>
        <h3 style={{ fontSize: 15, margin: "0 0 8px" }}>{props.t("nav.tools")}</h3>
        <ToolsPanel t={props.t} />
      </section>
      <section>
        <h3 style={{ fontSize: 15, margin: "0 0 8px" }}>{props.t("nav.autostart")}</h3>
        <AutostartSwitch scope={props.scope} t={props.t} />
      </section>
    </div>
  );
}

/** 所需服务（cordis fiber inject）；settingsScope 的 bind 内部需要 connection / remote。 */
export const inject = ["slots", "locale", "connection", "remote", "settingsScope"];

/** client 面入口：注册“桌面”设置节与开机自启开关。 */
export function apply(ctx: ClientContextLike): void {
  const NS = "settings.desktop";
  const namespace = "desktop";
  const t = ctx.locale.bind(NS);
  ctx.effect(
    () =>
      ctx.locale.register(NS, "zh", {
        nav: "桌面",
        "nav.status": "状态",
        "nav.config": "配置",
        "nav.tools": "工具",
        "nav.autostart": "开机自启",
        "status.install": "安装 DSH",
        "status.retry": "重试",
        "status.openLogs": "日志目录",
        "config.save": "保存",
        "config.restart": "重启 dsh 生效",
        "config.saved": "已保存，重启 dsh 后生效。",
        "tools.openLogs.title": "打开日志目录",
        "tools.openLogs.desc": "打开 dsh 运行日志目录",
        "tools.openLogs.action": "打开",
        "tools.checkUpdate.title": "检查更新",
        "tools.checkUpdate.desc": "检查 GitHub Release 是否有新版本并安装",
        "tools.checkUpdate.action": "检查",
        "autostart.title": "开机自启",
        "autostart.desc": "登录系统时自动启动桌面应用",
      }),
    "bridge: 中文字典",
  );
  ctx.effect(
    () =>
      ctx.locale.register(NS, "en", {
        nav: "Desktop",
        "nav.status": "Status",
        "nav.config": "Config",
        "nav.tools": "Tools",
        "nav.autostart": "Launch at login",
        "status.install": "Install DSH",
        "status.retry": "Retry",
        "status.openLogs": "Log directory",
        "config.save": "Save",
        "config.restart": "Restart dsh",
        "config.saved": "Saved. Restart dsh to apply.",
        "tools.openLogs.title": "Open log directory",
        "tools.openLogs.desc": "Open the dsh runtime log directory",
        "tools.openLogs.action": "Open",
        "tools.checkUpdate.title": "Check for updates",
        "tools.checkUpdate.desc": "Check GitHub releases for a new version and install it",
        "tools.checkUpdate.action": "Check",
        "autostart.title": "Launch at login",
        "autostart.desc": "Start the desktop app automatically when you log in",
      }),
    "bridge: English dictionary",
  );
  // settingsScope 由 @deepseek-ai/dsh-client-ui-settings 提供，bind 返回 SettingsScope<T>
  const settingsScope = ctx.settingsScope.bind({ namespace });
  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "desktop",
        order: 90,
        label: () => t("nav"),
        locale: NS,
        children: {},
      },
      () => <DesktopPanel scope={settingsScope} t={t} />,
    ),
  );
}
