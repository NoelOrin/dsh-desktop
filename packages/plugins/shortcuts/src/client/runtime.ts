/** shortcuts 插件使用的桥接与快捷键本地类型切片。 */
import { getBridge as readBridge } from "../../../client-kit/inject";

export type { ShortcutsSettings } from "../shared/settings";

export interface ShortcutSnapshot {
  shortcut: string;
  registered: boolean;
}

export interface BridgeLike {
  shortcuts: {
    register(shortcut: string): Promise<() => void>;
    unregister(shortcut: string): Promise<void>;
    list(): Promise<ShortcutSnapshot[]>;
    unregisterAll(): Promise<void>;
  };
  /** 订阅任意系统级全局快捷键按下（payload 为快捷键字符串）。 */
  onShortcut(cb: (shortcut: string) => void): Promise<() => void>;
  /** 无边框窗口控制（minimize / maximize / close / toggle-visible）。 */
  windowAction(action: "minimize" | "maximize" | "close" | "toggle-visible"): Promise<void>;
}

/** 翻译函数形状（ctx.locale.bind 的返回）。 */
export type Translate = (key: string) => string;

/** SettingsScope<T> 的最小形状：getSnapshot / subscribe / set。 */
export interface SettingsScopeLike<T> {
  getSnapshot(): { status: "loading" | "ready" | "unavailable"; value: T | undefined };
  subscribe(listener: () => void): () => void;
  set(field: string, value: unknown): Promise<void>;
}

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
export const getBridge = () => readBridge<BridgeLike>();
