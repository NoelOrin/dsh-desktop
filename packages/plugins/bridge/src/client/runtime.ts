/** client 面桥接与桌面设置的本地类型切片。 */

/** 桥接对象最小切片（与 src/index.ts 的 DshDesktopBridge 对齐；client 侧内联读取 window，不跨侧 import）。 */
export interface BridgeLike {
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
export interface RuntimeSnapshot {
  phase: string;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  log_dir: string | null;
  logs: string[];
}

/** 壳侧配置（与 packages/contracts 的 DshConfig 对齐）。 */
export interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
}

/** desktop 设置命名空间的快照形状（与 host 侧 z.object 对齐）。 */
export interface DesktopConfig {
  autostart?: boolean;
  startupMode?: "normal" | "tray" | "minimized";
  dsh_bin?: string | null;
  dsh_node?: string | null;
  dsh_home?: string | null;
}

/** SettingsScope<T> 的最小形状：getSnapshot / subscribe / set。 */
export interface SettingsScopeLike<T> {
  getSnapshot(): { status: "loading" | "ready" | "unavailable"; value: T | undefined };
  subscribe(listener: () => void): () => void;
  set(field: string, value: unknown): Promise<void>;
}

/** 翻译函数形状（ctx.locale.bind 的返回）。 */
export type Translate = (key: string) => string;

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
export function getBridge(): BridgeLike | null {
  const bridge = (window as unknown as { __DSH_DESKTOP__?: BridgeLike }).__DSH_DESKTOP__;
  return bridge ?? null;
}
