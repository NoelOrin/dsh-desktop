import { AppearanceSection } from "./client/AppearanceSection";
import { DesktopPanel } from "./client/DesktopPanel";
import type { SettingsScopeLike, Translate } from "./client/runtime";
import "./client/style-inject";
import { applyThemeSection, ensurePageStyle } from "./client/theme-apply";
import { createThemeStore } from "./client/theme-store";
import { THEME_SETTINGS_NAMESPACE, type ThemeSettings } from "./shared/theme";

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

  ctx.effect(() => ensurePageStyle(), "bridge: 全局页面样式");

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
        "autostart.title": "Launch at login",
        "autostart.desc": "Start the desktop app automatically when you log in",
        "autostart.mode": "Window state after autostart",
        "autostart.mode.normal": "Show normally",
        "autostart.mode.tray": "Start in tray",
        "autostart.mode.minimized": "Start minimized",
      }),
    "bridge: English dictionary",
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
      () => <DesktopPanel t={t} />,
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
