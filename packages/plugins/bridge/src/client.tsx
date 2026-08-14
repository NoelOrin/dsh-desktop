import { useSyncExternalStore } from "react";

/** 桥接对象最小切片（与 src/index.ts 的 DshDesktopBridge 对齐；client 侧内联读取 window，不跨侧 import）。 */
interface BridgeLike {
  autostart: {
    get(): Promise<boolean>;
    set(enabled: boolean): Promise<void>;
  };
}

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
function getBridge(): BridgeLike | null {
  const bridge = (window as unknown as { __DSH_DESKTOP__?: BridgeLike }).__DSH_DESKTOP__;
  return bridge ?? null;
}

/** desktop 设置命名空间的快照形状（与 host 侧 z.object({ autostart }) 对齐）。 */
interface DesktopConfig {
  autostart?: boolean;
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
        "autostart.title": "开机自启",
        "autostart.desc": "登录系统时自动启动桌面应用",
      }),
    "bridge: 中文字典",
  );
  ctx.effect(
    () =>
      ctx.locale.register(NS, "en", {
        nav: "Desktop",
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
      () => <AutostartSwitch scope={settingsScope} t={t} />,
    ),
  );
}
