import { injectPluginCss } from "../../client-kit/inject";
import { AppearanceSection } from "./client/AppearanceSection";
import { DesktopPanel } from "./client/DesktopPanel";
import { applyDesktopShell } from "./client/desktop-shell";
import { PluginPanel } from "./client/PluginPanel";
import type { DesktopModeSettings, SettingsScopeLike, Translate } from "./client/runtime";
import { resolveAdvancedParams } from "./client/advanced/theme-presenter";
import { applySettingsNavIcons } from "./client/settings-nav-icons";
import { applyThemeSection, ensurePageStyle } from "./client/theme-apply";
import { createThemeStore } from "./client/theme-store";
import { THEME_SETTINGS_NAMESPACE, type ThemeSettings } from "./shared/theme";

injectPluginCss("@dsh-desktop/plugin-bridge", "@dsh-desktop/plugin-bridge/ui");

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
    bind<T>(spec: { namespace: string }): SettingsScopeLike<T>;
  };
  slots: {
    inject(key: string, callback: () => unknown): unknown;
    register(options: unknown, component: unknown): unknown;
  };
}

/** 所需服务（cordis fiber inject）；settingsScope 的 bind 内部需要 connection / remote。 */
export const inject = ["slots", "locale", "connection", "remote", "settingsScope"];

/** client 面入口：注册“桌面”与“外观”设置节。 */
export function apply(ctx: ClientContextLike): void {
  const NS = "settings.desktop";
  const t = ctx.locale.bind(NS);
  const PLUGIN_NS = "settings.desktop.plugins";
  const pluginT = ctx.locale.bind(PLUGIN_NS);

  ctx.effect(() => ensurePageStyle(), "bridge: 全局页面样式");
  const shellParams = resolveAdvancedParams(window.location.search);
  ctx.effect(
    () => applyDesktopShell(shellParams.mode, shellParams.platform),
    "bridge: 桌面壳形态",
  );
  ctx.effect(applySettingsNavIcons, "bridge: 设置菜单插件图标");

  // ── 主题与背景（ui-theme 命名空间由上游 dsh-client-ui-theme host 注册，这里只 bind）──
  const THEME_NS = "settings.appearance";
  const themeScope = ctx.settingsScope.bind<ThemeSettings>({
    namespace: THEME_SETTINGS_NAMESPACE,
  });
  const themeStore = createThemeStore(themeScope);
  const modeScope = ctx.settingsScope.bind<DesktopModeSettings>({
    namespace: "dsh-desktop",
  });

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
        "nav.status.desc": "当前 dsh 运行环境、端口与最近日志",
        "nav.config": "配置",
        "nav.config.desc": "自定义可执行文件和 Harness 数据目录",
        "nav.tools": "工具",
        "nav.tools.desc": "打开日志目录或检查桌面端更新",
        "nav.autostart": "开机自启",
        "nav.autostart.desc": "控制登录时是否启动及窗口状态",
        "status.detecting": "正在检测运行环境...",
        "status.detail": "运行环境详情",
        "status.phase": "阶段",
        "status.phase.detecting": "检测中",
        "status.phase.missing": "未安装",
        "status.phase.installing": "安装中",
        "status.phase.starting": "启动中",
        "status.phase.ready": "就绪",
        "status.phase.failed": "失败",
        "status.node": "Node",
        "status.nodeFound": "可用",
        "status.nodeMissing": "缺失",
        "status.logs": "日志行数",
        "status.noLogs": "暂无日志",
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
        "mode.title": "界面模式",
        "mode.compatibility": "兼容模式",
        "mode.advanced": "高级模式",
        "mode.restart": "已保存，重启 dsh 后生效",
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
        "nav.status.desc": "Current dsh runtime, port and recent logs",
        "nav.config": "Config",
        "nav.config.desc": "Custom executable paths and Harness data directory",
        "nav.tools": "Tools",
        "nav.tools.desc": "Open logs or check for desktop updates",
        "nav.autostart": "Launch at login",
        "nav.autostart.desc": "Control startup behavior and window state",
        "status.detecting": "Detecting the runtime environment...",
        "status.detail": "Runtime details",
        "status.phase": "Phase",
        "status.phase.detecting": "Detecting",
        "status.phase.missing": "Missing",
        "status.phase.installing": "Installing",
        "status.phase.starting": "Starting",
        "status.phase.ready": "Ready",
        "status.phase.failed": "Failed",
        "status.node": "Node",
        "status.nodeFound": "Found",
        "status.nodeMissing": "Missing",
        "status.logs": "Log lines",
        "status.noLogs": "No logs yet",
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
        "mode.title": "Interface mode",
        "mode.compatibility": "Compatibility mode",
        "mode.advanced": "Advanced mode",
        "mode.restart": "Saved. Restart dsh to apply.",
        "autostart.title": "Launch at login",
        "autostart.desc": "Start the desktop app automatically when you log in",
        "autostart.mode": "Window state after autostart",
        "autostart.mode.normal": "Show normally",
        "autostart.mode.tray": "Start in tray",
        "autostart.mode.minimized": "Start minimized",
      }),
    "bridge: English dictionary",
  );

  ctx.effect(
    () =>
      ctx.locale.register(PLUGIN_NS, "zh", {
        nav: "桌面插件",
        "install.title": "插件管理",
        "install.desc": "安装到当前 active profile，重启 dsh 后生效",
        "install.profile": "当前 profile",
        "install.placeholder": "包名或 git URL",
        "install.button": "安装插件",
        "install.success": "安装成功，重启后生效",
        "operation.running": "操作中",
        "operation.failed": "操作失败",
        "operation.busy": "当前只有一个插件操作可运行",
        "operation.unavailable": "桌面壳桥接不可用",
        restart: "重启 dsh",
        "presets.title": "远程插件预设",
        "presets.desc": "添加插件 URL 或 git 地址，启动时自动安装到当前 profile",
        "presets.sync": "立即同步",
        "presets.syncGroup": "同步本组",
        "presets.add": "添加预设",
        "presets.urlRequired": "请输入插件 URL 或 git 地址",
        "presets.duplicate": "该远程插件 URL 已存在",
        "presets.groupPlaceholder": "分组，默认 default",
        "presets.groupEnabled": "整组启用",
        "presets.removeGroup": "移除本组",
        "presets.external": "外部固定",
        "presets.enabled": "启动时自动下载",
        "presets.disabled": "已停用",
        "presets.remove": "移除",
        "presets.empty": "尚未添加远程插件",
        "presets.saved": "远程插件预设已保存，重启 dsh 后自动下载",
        "presets.synced": "远程插件同步完成",
        "presets.syncedGroup": "当前分组同步完成",
        "installed.title": "已安装插件",
        "installed.desc": "当前 profile 的直装依赖；安装、更新或移除后需重启 dsh",
        "installed.update": "更新全部",
        "installed.remove": "移除",
        "installed.updated": "插件更新完成，重启 dsh 后生效",
        "installed.removed": "插件已移除，重启 dsh 后生效",
        "installed.bundle": "bundle",
        "installed.dependency": "依赖",
        "installed.empty": "当前 profile 没有额外安装的插件",
        "output.title": "操作输出",
        "output.desc": "最近一次 dsh plugin 命令的原始输出",
      }),
    "bridge: 插件中文字典",
  );
  ctx.effect(
    () =>
      ctx.locale.register(PLUGIN_NS, "en", {
        nav: "Desktop plugins",
        "install.title": "Plugin management",
        "install.desc": "Install into the active profile; restart dsh to apply",
        "install.profile": "Active profile",
        "install.placeholder": "Package name or git URL",
        "install.button": "Install plugin",
        "install.success": "Plugin installed. Restart dsh to apply.",
        "operation.running": "Running...",
        "operation.failed": "Operation failed",
        "operation.busy": "Only one plugin operation can run at a time",
        "operation.unavailable": "Desktop shell bridge unavailable",
        restart: "Restart dsh",
        "presets.title": "Remote plugin presets",
        "presets.desc":
          "Add plugin URLs or git addresses to install into the active profile on startup",
        "presets.sync": "Sync now",
        "presets.syncGroup": "Sync group",
        "presets.add": "Add preset",
        "presets.urlRequired": "Enter a plugin URL or git address",
        "presets.duplicate": "This remote plugin URL already exists",
        "presets.groupPlaceholder": "Group, default is default",
        "presets.groupEnabled": "Enable group",
        "presets.removeGroup": "Remove group",
        "presets.external": "External",
        "presets.enabled": "Download on startup",
        "presets.disabled": "Disabled",
        "presets.remove": "Remove",
        "presets.empty": "No remote plugins configured",
        "presets.saved": "Preset saved. It will download on the next dsh restart.",
        "presets.synced": "Remote plugins synced",
        "presets.syncedGroup": "Current group synced",
        "installed.title": "Installed plugins",
        "installed.desc":
          "Direct dependencies of the active profile; restart dsh after install, update or remove",
        "installed.update": "Update all",
        "installed.remove": "Remove",
        "installed.updated": "Plugins updated. Restart dsh to apply.",
        "installed.removed": "Plugin removed. Restart dsh to apply.",
        "installed.bundle": "bundle",
        "installed.dependency": "dependency",
        "installed.empty": "No extra plugins installed in the active profile",
        "output.title": "Operation output",
        "output.desc": "Raw output from the latest dsh plugin command",
      }),
    "bridge: Plugins English dictionary",
  );

  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "desktop-plugins",
        // 官方插件设置节已占用 plugins id；菜单/标签图标由 settings-nav-icons 补齐。
        order: 85,
        label: () => pluginT("nav"),
        locale: PLUGIN_NS,
        children: {},
      },
      () => <PluginPanel t={pluginT} />,
    ),
  );

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
      () => <DesktopPanel t={t} modeScope={modeScope} />,
    ),
  );
}

const themeSectionZh: Record<string, string> = {
  nav: "外观",
  "pref.title": "主题偏好",
  "pref.desc": "切换界面明暗与系统跟随",
  "pref.label": "主题模式",
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
  "wallpaper.empty": "尚未设置背景图",
  "wallpaper.preview": "背景图预览",
  "wallpaper.blur": "毛玻璃",
  "wallpaper.pixelate": "像素化",
  "wallpaper.reset": "重置效果",
  "wallpaper.effects": "背景效果",
  "glass.title": "玻璃透明度",
  "glass.desc": "数值越低，侧栏、对话框和输入框越通透",
  "glass.opacity": "透明度",
  "glass.reset": "恢复默认",
  "glass.preview": "玻璃表面预览",
  "glass.surface": "面板玻璃",
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
  saveError: "主题设置保存失败",
  "type.title": "排版",
  "type.desc": "界面与代码字号、字体族",
  "type.interfaceSize": "界面字号",
  "type.reset": "恢复默认",
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
  "pref.label": "Theme mode",
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
  "wallpaper.empty": "No wallpaper selected",
  "wallpaper.preview": "Wallpaper preview",
  "wallpaper.blur": "Frosted glass",
  "wallpaper.pixelate": "Pixelation",
  "wallpaper.reset": "Reset effects",
  "wallpaper.effects": "Background effects",
  "glass.title": "Glass opacity",
  "glass.desc": "Lower values make sidebar, dialogs and composer more translucent",
  "glass.opacity": "Opacity",
  "glass.reset": "Reset glass",
  "glass.preview": "Glass surface preview",
  "glass.surface": "Panel glass",
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
  saveError: "Failed to save theme settings",
  "type.title": "Typography",
  "type.desc": "Interface and code font sizes and families",
  "type.interfaceSize": "Interface font size",
  "type.reset": "Reset typography",
  "type.codeSize": "Code font size",
  "type.sans": "Interface font family",
  "type.code": "Code font family",
  "type.composer": "Composer font family",
  "type.terminal": "Terminal font family",
  "type.default": "Leave empty to keep the default stack",
};
