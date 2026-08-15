/** client 面桥接与桌面设置的本地类型切片。 */
import { getBridge as readBridge } from "../../../client-kit/inject";

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
  desktop: {
    get(): Promise<DesktopSettings>;
    set(settings: DesktopSettings): Promise<void>;
  };
  profiles: {
    list(): Promise<DshProfileSummary[]>;
    active(): Promise<DesktopProfileState>;
    select(name: string): Promise<ProfileSelectionResult>;
  };
  plugins: {
    install(spec: string): Promise<PluginOperationResult>;
    remove(name: string): Promise<PluginOperationResult>;
    update(): Promise<PluginOperationResult>;
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
  shortcuts: string[];
}

export type StartupMode = "normal" | "tray" | "minimized";

/** 壳侧开机自启设置（与 packages/contracts 的 DesktopSettings 对齐）。 */
export interface DesktopSettings {
  autostart: boolean;
  startup_mode: StartupMode;
}

/** dsh profile 摘要（与 packages/contracts 的 DshProfileSummary 对齐）。 */
export interface DshProfileSummary {
  name: string;
  dir: string;
  exists: boolean;
  web_capable: boolean;
  problem: string | null;
}

/** profile 状态机快照（与 packages/contracts 的 DesktopProfileState 对齐）。 */
export interface DesktopProfileState {
  version: 1;
  active: string;
  pending: string | null;
  last_known_good: string;
}

/** profile 选择结果（与 packages/contracts 的 ProfileSelectionResult 对齐）。 */
export interface ProfileSelectionResult {
  profile: string;
  restart_required: boolean;
}

/** 受管插件操作结果（与 packages/contracts 的 PluginOperationResult 对齐）。 */
export interface PluginOperationResult {
  ok: boolean;
  exit_code: number | null;
  output: string[];
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
export const getBridge = () => readBridge<BridgeLike>();
