import { Button, Input, StateDot } from "@deepseek-ai/dsh-client-ui-primitives";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { AppearanceSection } from "./client/AppearanceSection";
import css from "./client/desktop.module.css";
import { SettingsPage, SettingsSection } from "./client/settings-layout";
import { applyThemeSection } from "./client/theme-apply";
import { createThemeStore } from "./client/theme-store";
import { THEME_SETTINGS_NAMESPACE, type ThemeSettings } from "./shared/theme";

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
  openExternal(target: string): Promise<void>;
  update: {
    check(): Promise<string | null>;
    install(): Promise<string>;
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
  startupMode?: "normal" | "tray" | "minimized";
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
    bind<T = DesktopConfig>(spec: { namespace: string }): SettingsScopeLike<T>;
  };
  slots: {
    inject(key: string, callback: () => unknown): unknown;
    register(options: unknown, component: unknown): unknown;
  };
}

/** 开机自启设置：settings 与系统自启事务同步，并支持自启后窗口状态。 */
function StartupSettings(props: {
  scope: SettingsScopeLike<DesktopConfig>;
  t: Translate;
}): JSX.Element {
  const snapshot = useSyncExternalStore(
    (listener) => props.scope.subscribe(listener),
    () => props.scope.getSnapshot(),
  );
  const autostart = snapshot.value?.autostart ?? false;
  const startupMode = snapshot.value?.startupMode ?? "normal";
  const [osEnabled, setOsEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getBridge()
      ?.autostart.get()
      .then(setOsEnabled)
      .catch((e: unknown) => setError(`读取系统自启状态失败: ${String(e)}`));
  }, []);

  const apply = async (nextAutostart: boolean, nextMode: "normal" | "tray" | "minimized") => {
    const previousAutostart = autostart;
    const bridge = getBridge();
    if (bridge && osEnabled !== null && osEnabled !== nextAutostart) {
      try {
        await bridge.autostart.set(nextAutostart);
      } catch (e) {
        setError(`系统自启设置失败: ${String(e)}`);
        return;
      }
    }
    try {
      await props.scope.set("autostart", nextAutostart);
      await props.scope.set("startupMode", nextMode);
      setError(null);
    } catch (e) {
      setError(`保存桌面设置失败: ${String(e)}`);
      if (bridge && nextAutostart !== previousAutostart) {
        await bridge.autostart.set(previousAutostart).catch(() => {});
      }
    }
  };

  return (
    <div className={css.block}>
      <label className={css.toggleRow} htmlFor="startup-enabled">
        <div className={css.toggleBody}>
          <div className={css.toggleTitle}>{props.t("autostart.title")}</div>
          <div className={css.toggleDesc}>{props.t("autostart.desc")}</div>
        </div>
        <input
          id="startup-enabled"
          type="checkbox"
          className={css.toggle}
          checked={autostart}
          onChange={(e) => void apply(e.currentTarget.checked, startupMode)}
          aria-label={props.t("autostart.title")}
        />
      </label>
      <label className={css.modeRow} htmlFor="startup-mode">
        <span>{props.t("autostart.mode")}</span>
        <select
          id="startup-mode"
          className={css.select}
          disabled={!autostart}
          value={startupMode}
          onChange={(e) =>
            void apply(autostart, e.currentTarget.value as "normal" | "tray" | "minimized")
          }
        >
          <option value="normal">{props.t("autostart.mode.normal")}</option>
          <option value="tray">{props.t("autostart.mode.tray")}</option>
          <option value="minimized">{props.t("autostart.mode.minimized")}</option>
        </select>
      </label>
      {error && <p className={css.messageError}>{error}</p>}
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
  const phaseState =
    phase === "ready"
      ? "done"
      : phase === "failed"
        ? "error"
        : phase === "missing"
          ? "warning"
          : "ongoing";

  return (
    <div className={css.block}>
      <div className={css.statusRow}>
        <StateDot state={phaseState} size={10} />
        <span className={css.statusText}>{snapshot?.message ?? "正在检测运行环境..."}</span>
        {snapshot?.url && <code className={css.url}>{snapshot.url}</code>}
      </div>
      <div className={css.actions}>
        <Button
          type="button"
          variant="outline"
          hidden={phase !== "missing" || snapshot?.node_found === false}
          onClick={() =>
            void getBridge()
              ?.installDsh()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.install")}
        </Button>
        <Button
          type="button"
          variant="outline"
          hidden={phase !== "failed"}
          onClick={() =>
            void getBridge()
              ?.restart()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.retry")}
        </Button>
        <Button
          type="button"
          variant="ghost"
          hidden={phase !== "failed"}
          onClick={() =>
            void getBridge()
              ?.openLogDirectory()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("status.openLogs")}
        </Button>
      </div>
      {showLogs && (
        <pre ref={logsRef} className={css.log}>
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
    <div className={css.block}>
      <label className={css.field} htmlFor="desktop-dsh-bin">
        <span>DSH 入口 (DSH_BIN)</span>
        <Input
          className={css.input}
          id="desktop-dsh-bin"
          value={dshBin}
          onChange={(e) => {
            setDshBin(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="例如 /path/to/dsh"
        />
      </label>
      <label className={css.field} htmlFor="desktop-dsh-node">
        <span>Node 解释器 (DSH_NODE)</span>
        <Input
          className={css.input}
          id="desktop-dsh-node"
          value={dshNode}
          onChange={(e) => {
            setDshNode(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="例如 /path/to/node"
        />
      </label>
      <label className={css.field} htmlFor="desktop-dsh-home">
        <span>Harness 数据目录 (DSH_HOME)</span>
        <Input
          className={css.input}
          id="desktop-dsh-home"
          value={dshHome}
          onChange={(e) => {
            setDshHome(e.currentTarget.value);
            setSaved(false);
            setError(null);
          }}
          placeholder="留空则继承环境变量"
        />
      </label>
      <div className={css.formActions}>
        <Button type="button" onClick={() => void save()}>
          {t("config.save")}
        </Button>
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            void getBridge()
              ?.restart()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("config.restart")}
        </Button>
      </div>
      {error && <p className={css.messageError}>{error}</p>}
      {saved && <p className={css.messageInfo}>{t("config.saved")}</p>}
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
      if (window.confirm(`发现新版本 ${version}，是否静默下载？`)) {
        setMsg("正在下载更新，请稍候…");
        const path = await bridge.update.install();
        setMsg(`更新已下载：${path}`);
        if (window.confirm("更新已下载到本地，是否打开安装包？")) {
          await bridge.openExternal(path);
          setMsg(`已打开安装包：${path}`);
        } else {
          setMsg(`更新已下载（未打开）：${path}`);
        }
      }
    } catch (e) {
      setMsg(`检查/下载更新失败: ${String(e)}`);
    }
  };

  return (
    <div className={css.block}>
      <div className={css.toolCard}>
        <div className={css.toolTitle}>{t("tools.openLogs.title")}</div>
        <div className={css.toolDesc}>{t("tools.openLogs.desc")}</div>
        <div className={css.actions}>
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              void getBridge()
                ?.openLogDirectory()
                .catch((e: unknown) => console.error(e))
            }
          >
            {t("tools.openLogs.action")}
          </Button>
        </div>
      </div>
      <div className={css.toolCard}>
        <div className={css.toolTitle}>{t("tools.checkUpdate.title")}</div>
        <div className={css.toolDesc}>{t("tools.checkUpdate.desc")}</div>
        <div className={css.actions}>
          <Button type="button" variant="outline" onClick={() => void doCheckUpdate()}>
            {t("tools.checkUpdate.action")}
          </Button>
        </div>
        {msg && <p className={css.messageInfo}>{msg}</p>}
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
    <SettingsPage>
      <SettingsSection title={props.t("nav.status")}>
        <StatusPanel t={props.t} />
      </SettingsSection>
      <SettingsSection title={props.t("nav.config")}>
        <ConfigPanel t={props.t} />
      </SettingsSection>
      <SettingsSection title={props.t("nav.tools")}>
        <ToolsPanel t={props.t} />
      </SettingsSection>
      <SettingsSection title={props.t("nav.autostart")}>
        <StartupSettings scope={props.scope} t={props.t} />
      </SettingsSection>
    </SettingsPage>
  );
}

/** 所需服务（cordis fiber inject）；settingsScope 的 bind 内部需要 connection / remote。 */
export const inject = ["slots", "locale", "connection", "remote", "settingsScope"];

/** client 面入口：注册“桌面”设置节与开机自启开关。 */
export function apply(ctx: ClientContextLike): void {
  const NS = "settings.desktop";
  const namespace = "desktop";
  const t = ctx.locale.bind(NS);

  // ── 主题与背景（ui-theme 命名空间由上游 dsh-client-ui-theme host 注册，这里只 bind）──
  const THEME_NS = "settings.appearance";
  const themeScope = ctx.settingsScope.bind<ThemeSettings>({
    namespace: THEME_SETTINGS_NAMESPACE,
  });
  const themeStore = createThemeStore(themeScope);

  const systemDark = (): boolean =>
    typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;

  const applyNow = (): void => {
    applyThemeSection(themeStore.getSnapshot(), systemDark(), themeStore.getPreview());
  };

  ctx.effect(() => {
    applyNow();
    const unsubscribe = themeStore.subscribe(applyNow);
    const media = matchMedia("(prefers-color-scheme: dark)");
    const onMedia = (): void => {
      if (themeStore.getSnapshot().preference === "system") applyNow();
    };
    media.addEventListener("change", onMedia);
    return () => {
      unsubscribe();
      media.removeEventListener("change", onMedia);
    };
  }, "bridge: 主题应用");

  const themeT = ctx.locale.bind(THEME_NS);
  ctx.effect(() => ctx.locale.register(THEME_NS, "zh", themeSectionZh), "bridge: 外观中文字典");
  ctx.effect(() => ctx.locale.register(THEME_NS, "en", themeSectionEn), "bridge: 外观英文字典");
  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "appearance",
        order: 5,
        label: () => themeT("nav"),
        locale: THEME_NS,
        children: {},
      },
      () => <AppearanceSection store={themeStore} t={themeT} />,
    ),
  );

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
        "autostart.mode": "自启后窗口状态",
        "autostart.mode.normal": "正常显示",
        "autostart.mode.tray": "启动到托盘",
        "autostart.mode.minimized": "启动时最小化",
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
        "autostart.mode": "Window state after autostart",
        "autostart.mode.normal": "Show normally",
        "autostart.mode.tray": "Start in tray",
        "autostart.mode.minimized": "Start minimized",
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

const themeSectionZh: Record<string, string> = {
  nav: "外观",
  "pref.title": "主题偏好",
  "pref.desc": "切换界面明暗与系统跟随",
  "pref.light": "浅色",
  "pref.dark": "深色",
  "pref.system": "跟随系统",
  "library.title": "主题库",
  "library.desc": "每张卡有浅、深两半，点哪半用哪半",
  "library.lightHalf": "浅色半",
  "library.darkHalf": "深色半",
  "library.builtin": "内置",
  "library.custom": "自定义",
  "wallpaper.title": "背景图",
  "wallpaper.desc": "选一张图铺在整个界面后面，可调毛玻璃与像素化",
  "wallpaper.choose": "选择图片",
  "wallpaper.clear": "清除",
  "wallpaper.blur": "毛玻璃",
  "wallpaper.pixelate": "像素化",
  "wallpaper.reset": "重置效果",
  "glass.title": "玻璃透明度",
  "glass.desc": "数值越低，侧栏、对话框和输入框越通透",
  "glass.opacity": "透明度",
  "custom.title": "自定义主题",
  "custom.create": "新建",
  "custom.import": "导入",
  "custom.edit": "编辑",
  "custom.copy": "复制",
  "custom.export": "导出",
  "custom.remove": "删除",
  "custom.name": "名称",
  "custom.lightHalf": "浅色",
  "custom.darkHalf": "深色",
  "custom.accent": "强调色",
  "custom.background": "背景",
  "custom.foreground": "前景",
  "custom.contrast": "对比度",
  "custom.save": "保存",
  "custom.cancel": "取消",
  "custom.newName": "新主题",
  "type.title": "排版",
  "type.desc": "界面与代码字号、字体族",
  "type.interfaceSize": "界面字号",
  "type.codeSize": "代码字号",
  "type.sans": "界面字体族",
  "type.code": "代码字体族",
  "type.composer": "输入框字体族",
  "type.terminal": "终端字体族",
  "type.default": "留空使用默认栈",
};

const themeSectionEn: Record<string, string> = {
  nav: "Appearance",
  "pref.title": "Theme preference",
  "pref.desc": "Switch between light, dark and system appearance",
  "pref.light": "Light",
  "pref.dark": "Dark",
  "pref.system": "System",
  "library.title": "Theme library",
  "library.desc": "Each card has a light and dark half; click the half to use it",
  "library.lightHalf": "Light half",
  "library.darkHalf": "Dark half",
  "library.builtin": "Built-in",
  "library.custom": "Custom",
  "wallpaper.title": "Wallpaper",
  "wallpaper.desc": "Pick an image to lay behind the whole UI; adjust frost and pixelation",
  "wallpaper.choose": "Choose image",
  "wallpaper.clear": "Clear",
  "wallpaper.blur": "Frosted glass",
  "wallpaper.pixelate": "Pixelation",
  "wallpaper.reset": "Reset effects",
  "glass.title": "Glass opacity",
  "glass.desc": "Lower values make sidebar, dialogs and composer more translucent",
  "glass.opacity": "Opacity",
  "custom.title": "Custom themes",
  "custom.create": "Create",
  "custom.import": "Import",
  "custom.edit": "Edit",
  "custom.copy": "Duplicate",
  "custom.export": "Export",
  "custom.remove": "Remove",
  "custom.name": "Name",
  "custom.lightHalf": "Light",
  "custom.darkHalf": "Dark",
  "custom.accent": "Accent",
  "custom.background": "Background",
  "custom.foreground": "Foreground",
  "custom.contrast": "Contrast",
  "custom.save": "Save",
  "custom.cancel": "Cancel",
  "custom.newName": "New theme",
  "type.title": "Typography",
  "type.desc": "Interface and code font sizes and families",
  "type.interfaceSize": "Interface font size",
  "type.codeSize": "Code font size",
  "type.sans": "Interface font family",
  "type.code": "Code font family",
  "type.composer": "Composer font family",
  "type.terminal": "Terminal font family",
  "type.default": "Leave empty to keep the default stack",
};
