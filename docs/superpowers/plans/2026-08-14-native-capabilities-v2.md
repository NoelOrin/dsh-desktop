# 桌面壳原生能力二期实施计划（托盘 / 自启 / 深链 / 快捷键 / 通知 / 进程日志 / 拖放）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

> **Status:** 已完成（2026-08-15）。本计划对应实现已落在当前 `feature/native` 分支（核心提交 `c707826` 及后续桌面设置/桥接契约提交），下方步骤已按最终实现勾选；`yarn typecheck`、`cargo fmt --check`、`cargo test` 当前通过。

**Goal:** 为 dsh-desktop 落地 7 组桌面壳能力：动态托盘、开机自启启动模式、结构化深链队列、快捷键管理、通知点击与权限、进程/日志/健康检查/版本管理、结构化文件拖放。

**Architecture:** 能力按既有边界分三层落地。① **Tauri 壳域**：`apps/shell/src-tauri/src/lib.rs` 负责状态、IPC、托盘、快捷键、深链、进程与拖放事件；新增 `notifications.rs`（通知点击回调）、`process.rs`（URL/拖放分类纯函数）和 `desktop_settings.rs`（自启/启动模式解析）保持 `lib.rs` 可读。② **dsh 插件域**：`packages/plugins/bridge` 注册健康端点与桥接类型；桌面设置由壳侧 `get_desktop_settings` / `set_desktop_settings` 命令持有并同步 OS/持久化，UI 负责事务反馈。③ **共享契约**：native IPC 类型与常量进 `packages/contracts`，桥接能力由 bridge 自持。

**落地差异：** 原 Task 3 设想把自启设置放进 dsh settings 命名空间；最终实现改为 `desktop-settings.json` + 壳侧 `get_desktop_settings` / `set_desktop_settings`，bridge 只暴露 `desktop.get()` / `desktop.set()`，client 在 `DesktopPanel.tsx` 完成设置。其余任务与计划一致。

**Tech Stack:** Tauri 2.11 / Rust 2021、`notify-rust 4.18`、TypeScript strict、dsh cordis 插件生态（`settingsScope` / `slots` / `locale` / `dsh-settings` / `schemastery`）、React 18、node:test、cargo test。

---

## Global Constraints

- 新增 IPC 命令必须同步四处：`apps/shell/src-tauri/build.rs` 的 `AppManifest::commands`、`capabilities/default.json`、`capabilities/bridge.json`、`packages/contracts/src/index.ts`（native 契约）或 bridge 自持类型。
- 新增 IPC 权限：只有本地窗口使用的进 `default.json`；dsh web 也要用的进 `bridge.json`。远程桥接只开放最小命令切片。
- `dist/`、`apps/shell/src-tauri/target/`、`apps/shell/src-tauri/gen/`、插件 `lib/` 均为生成产物，不手改。
- 用户可见文案与代码注释使用中文；字段命名 snake_case。
- 当前工作区已有用户改动（主题、插件 AGENTS、capabilities 等），实现时保留这些改动，不回退。
- 提交遵循 Conventional Commits，每个 Task 一个提交；提交前运行对应验证命令。
- `dsh web` 已支持 `--port 0` 并输出 `dsh web: http://127.0.0.1:<port>` 作为就绪信号（官方 `web-app` 的 URL line），本计划以 stdout URL 作为主就绪信号，`/dsh-desktop/health` 作为健康轮询端点。

---

### Task 1: 契约与命令注册脚手架

**Files:**
- Modify: `packages/contracts/src/index.ts`
- Modify: `apps/shell/src-tauri/build.rs`
- Modify: `apps/shell/src-tauri/capabilities/default.json`
- Modify: `apps/shell/src-tauri/capabilities/bridge.json`
- Modify: `apps/shell/src-tauri/Cargo.toml`

- [x] **Step 1: 扩展共享契约类型与常量**

在 `packages/contracts/src/index.ts` 中新增以下类型，并更新 `RuntimeSnapshot`、`DshConfig`、`COMMANDS`、`EVENTS`：

```ts
export type StartupMode = "normal" | "tray" | "minimized";

export interface DeepLinkPayload {
  id: string;
  url: string;
  raw: string;
  received_at: string;
  source: "deep_link" | "second_instance";
  args: string[];
  cwd: string;
}

export interface FileDropPayload {
  id: string;
  paths: string[];
  kind: "file" | "directory" | "mixed";
  position: { x: number; y: number };
  action: "open" | "import";
}

export interface ShortcutSnapshot {
  shortcut: string;
  registered: boolean;
}

export interface NotificationActionPayload {
  kind: "focus" | "open_session" | "open_update";
  session_id: string | null;
  url: string | null;
  path: string | null;
}

export interface RuntimeSnapshot {
  phase: RuntimePhase;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  dsh_version: string | null;
  log_dir: string | null;
  logs: string[];
}

export interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
  shortcuts: string[];
}

export const COMMANDS = {
  getStatus: "get_status",
  installDsh: "install_dsh",
  restart: "restart",
  openLogDirectory: "open_log_directory",
  getConfig: "get_config",
  setConfig: "set_config",
  openExternal: "open_external",
  getUiTheme: "get_ui_theme",
  windowAction: "window_action",
  getShortcuts: "get_shortcuts",
  unregisterAllShortcuts: "unregister_all_shortcuts",
  getPendingDeepLinks: "get_pending_deeplinks",
  ackDeepLink: "ack_deeplink",
  requestNotificationPermission: "request_notification_permission",
  updateDsh: "update_dsh",
  openPaths: "open_paths",
  importPaths: "import_paths",
} as const;

export const EVENTS = {
  dshStatus: "dsh-status",
  dshLog: "dsh-log",
  dshFileDrop: "dsh-file-drop",
  dshTheme: "dsh-theme",
  dshDeeplink: "dsh-deeplink",
  dshShortcut: "dsh-shortcut",
  dshUpdateAvailable: "dsh-update-available",
  dshUiTheme: "dsh-ui-theme",
  dshWindowState: "dsh-window-state",
  dshNotificationAction: "dsh-notification-action",
} as const;
```

- [x] **Step 2: 注册新命令并声明权限**

`apps/shell/src-tauri/build.rs` 的 `commands(&[...])` 追加：

```rust
"get_shortcuts",
"unregister_all_shortcuts",
"get_pending_deeplinks",
"ack_deeplink",
"request_notification_permission",
"update_dsh",
"open_paths",
"import_paths",
```

`capabilities/default.json` 的 `permissions` 追加：

```json
"allow-get-shortcuts",
"allow-unregister-all-shortcuts",
"allow-get-pending-deeplinks",
"allow-ack-deeplink",
"allow-request-notification-permission",
"allow-update-dsh",
"allow-open-paths",
"allow-import-paths"
```

`capabilities/bridge.json` 的 `permissions` 追加相同命令（`request_notification_permission` 也开放给桥接）。

- [x] **Step 3: 添加 notify-rust 直接依赖**

`apps/shell/src-tauri/Cargo.toml` 的 `[dependencies]` 追加：

```toml
notify-rust = { version = "4.18", default-features = false, features = ["z", "preview-macos-un"] }
```

原因：`tauri-plugin-notification` 的 desktop builder 是 fire-and-forget，无法拿到点击回调；`notify-rust` 的 `NotificationHandle` 支持 Windows toast、Linux D-Bus、macOS UNUserNotificationCenter 的响应等待。

- [x] **Step 4: 验证**

Run: `yarn typecheck`
Expected: 契约类型通过；Rust 侧尚未使用新字段，本阶段不产生 Rust 编译错误。

- [x] **Step 5: Commit**

```bash
git add packages/contracts/src/index.ts apps/shell/src-tauri/build.rs apps/shell/src-tauri/capabilities/default.json apps/shell/src-tauri/capabilities/bridge.json apps/shell/src-tauri/Cargo.toml
git commit -m "feat: expand native capability contracts"
```

---

### Task 2: 动态托盘菜单与状态跟随

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/AGENTS.md`

- [x] **Step 1: 定义新托盘菜单项与状态容器**

在 `lib.rs` 顶部把托盘菜单 id 扩展为：

```rust
const TRAY_STATUS: &str = "tray-status";
const TRAY_COPY_URL: &str = "tray-copy-url";
const TRAY_OPEN_BROWSER: &str = "tray-open-browser";
const TRAY_STOP_DSH: &str = "tray-stop-dsh";
const TRAY_RESTART_DSH: &str = "tray-restart-dsh";
const TRAY_SHOW_MAIN: &str = "tray-show-main";
const TRAY_QUIT: &str = "tray-quit";
```

把 `struct ManagedTray` 替换为可更新的状态容器：

```rust
struct TrayState {
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    status: tauri::menu::MenuItem<tauri::Wry>,
    copy_url: tauri::menu::MenuItem<tauri::Wry>,
    open_browser: tauri::menu::MenuItem<tauri::Wry>,
    stop_dsh: tauri::menu::MenuItem<tauri::Wry>,
    restart_dsh: tauri::menu::MenuItem<tauri::Wry>,
}
```

- [x] **Step 2: 重写 setup_tray**

`setup_tray` 返回 `tauri::Result<TrayState>`，菜单顺序为：状态、分隔线、复制 Web UI 地址、浏览器打开、停止 dsh、重启 dsh、分隔线、显示主窗口、退出：

```rust
fn setup_tray(app: &tauri::AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<TrayState> {
    let status = MenuItem::with_id(app, TRAY_STATUS, "DSH: 检测中", false, None::<&str>)?;
    let copy_url = MenuItem::with_id(app, TRAY_COPY_URL, "复制 Web UI 地址", false, None::<&str>)?;
    let open_browser = MenuItem::with_id(app, TRAY_OPEN_BROWSER, "用浏览器打开", false, None::<&str>)?;
    let stop_dsh = MenuItem::with_id(app, TRAY_STOP_DSH, "停止 dsh", false, None::<&str>)?;
    let restart_dsh = MenuItem::with_id(app, TRAY_RESTART_DSH, "重启 dsh", false, None::<&str>)?;
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT, "退出", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&status, &sep1, &copy_url, &open_browser, &stop_dsh, &restart_dsh, &sep2, &show_main, &quit],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).ok())
        .expect("缺少托盘图标");

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("DSH Desktop")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TRAY_COPY_URL => {
                let state = app.state::<AppState>();
                let snapshot = state.inner.lock().unwrap().snapshot();
                if let Some(url) = snapshot.url {
                    let _ = app.clipboard().write_text(url);
                }
            }
            TRAY_OPEN_BROWSER => {
                let state = app.state::<AppState>();
                let snapshot = state.inner.lock().unwrap().snapshot();
                if let Some(url) = snapshot.url {
                    let _ = open_with_system(&url);
                }
            }
            TRAY_STOP_DSH => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Stop);
            }
            TRAY_RESTART_DSH => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Start);
            }
            TRAY_SHOW_MAIN => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            TRAY_QUIT => {
                let app = app.clone();
                let exiting = exiting.clone();
                app.dialog()
                    .message("退出后将停止当前 dsh 会话，确认退出？")
                    .title("退出 DSH Desktop")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                    .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
                        "退出".into(),
                        "取消".into(),
                    ))
                    .show(move |confirmed| {
                        if !confirmed {
                            return;
                        }
                        exiting.store(true, Ordering::Relaxed);
                        let state = app.state::<AppState>();
                        let _ = state.tx.send(ManagerMessage::Stop);
                        thread::sleep(Duration::from_millis(300));
                        app.exit(0);
                    });
            }
            _ => {}
        })
        .build(app)?;

    Ok(TrayState {
        tray,
        status,
        copy_url,
        open_browser,
        stop_dsh,
        restart_dsh,
    })
}
```

同时新增 `use tauri_plugin_clipboard_manager::ClipboardExt;`。

- [x] **Step 3: setup 中管理 TrayState**

在 `setup` 中把原来的 `setup_tray(app.handle(), exiting.clone())?;` 改为：

```rust
let tray_state = setup_tray(app.handle(), exiting.clone())?;
app.manage(tray_state);
```

- [x] **Step 4: emit_status 时更新托盘**

```rust
fn emit_status(app: &AppHandle, inner: &Arc<Mutex<Inner>>) {
    let snapshot = inner.lock().unwrap().snapshot();
    let _ = app.emit("dsh-status", &snapshot);
    update_tray(app, &snapshot);
}

fn tray_phase_label(phase: &RuntimePhase) -> &'static str {
    match phase {
        RuntimePhase::Detecting => "检测中",
        RuntimePhase::MissingDsh => "未安装",
        RuntimePhase::Installing => "安装中",
        RuntimePhase::Starting => "启动中",
        RuntimePhase::Ready => "已就绪",
        RuntimePhase::Failed => "失败",
        RuntimePhase::Stopped => "已停止",
    }
}

fn update_tray(app: &AppHandle, snapshot: &RuntimeSnapshot) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let label = tray_phase_label(&snapshot.phase);
    let _ = state.status.set_text(format!("DSH: {label}"));
    let ready = matches!(snapshot.phase, RuntimePhase::Ready) && snapshot.url.is_some();
    let _ = state.copy_url.set_enabled(ready);
    let _ = state.open_browser.set_enabled(ready);
    let running = matches!(
        snapshot.phase,
        RuntimePhase::Starting | RuntimePhase::Ready
    );
    let _ = state.stop_dsh.set_enabled(running);
    let _ = state.restart_dsh.set_enabled(!matches!(snapshot.phase, RuntimePhase::Installing));
    let url_hint = snapshot
        .url
        .as_deref()
        .map(|url| format!(" ({url})"))
        .unwrap_or_default();
    let _ = state.tray.set_tooltip(Some(format!("DSH Desktop - {label}{url_hint}")));
}
```

- [x] **Step 5: 写测试**

在 `lib.rs` 的 `mod tests` 中追加：

```rust
#[test]
fn tray_labels_cover_all_phases() {
    assert_eq!(tray_phase_label(&RuntimePhase::Detecting), "检测中");
    assert_eq!(tray_phase_label(&RuntimePhase::Ready), "已就绪");
    assert_eq!(tray_phase_label(&RuntimePhase::Stopped), "已停止");
}
```

- [x] **Step 6: 验证**

Run: `cargo fmt --check && cargo test tray_labels_cover_all_phases`
Expected: PASS

- [x] **Step 7: Commit**

```bash
git add apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/AGENTS.md
git commit -m "feat: dynamic tray status and dsh controls"
```

---

### Task 3: 开机自启启动模式与 settings/OS 同步

**Files:**
- Create: `apps/shell/src-tauri/src/desktop_settings.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/src/index.ts`
- Modify: `packages/plugins/bridge/src/client.tsx`
- Modify: `packages/plugins/bridge/package.json`

- [x] **Step 1: 新增 desktop settings 解析模块**

`apps/shell/src-tauri/src/desktop_settings.rs`：

```rust
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopSection {
    pub autostart: Option<bool>,
    pub startup_mode: Option<String>,
}

pub fn read_desktop_section(settings_path: &Path) -> DesktopSection {
    let Ok(raw) = std::fs::read_to_string(settings_path) else {
        return DesktopSection::default();
    };
    let doc: Result<serde_yaml::Value, _> = serde_yaml::from_str(&raw);
    let Ok(doc) = doc else {
        return DesktopSection::default();
    };
    let Some(section) = doc.get("desktop") else {
        return DesktopSection::default();
    };
    serde_yaml::from_value(section.clone()).unwrap_or_default()
}

pub fn startup_mode(section: &DesktopSection) -> &'static str {
    match section.startup_mode.as_deref() {
        Some("tray") => "tray",
        Some("minimized") => "minimized",
        _ => "normal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_section() {
        let raw = "desktop:\n  autostart: true\n  startupMode: tray\n";
        let path = std::env::temp_dir().join("dsh-desktop-settings-test.yaml");
        std::fs::write(&path, raw).unwrap();
        let section = read_desktop_section(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(section.autostart, Some(true));
        assert_eq!(startup_mode(&section), "tray");
    }

    #[test]
    fn defaults_to_normal() {
        assert_eq!(startup_mode(&DesktopSection::default()), "normal");
    }
}
```

在 `lib.rs` 增加 `mod desktop_settings;`。

- [x] **Step 2: 消费 --autostart 并支持启动到托盘/最小化**

`run()` 顶部读取启动参数：

```rust
let autostart_requested = std::env::args().any(|arg| arg == "--autostart");
```

`AppState` 增加：

```rust
start_in_tray: Arc<AtomicBool>,
```

在 `setup` 中，`app.manage(AppState { ... })` 之前计算：

```rust
let launch_config = config::load(&config_path).effective(|k| std::env::var(k).ok());
let settings_home = launch_config
    .dsh_home
    .clone()
    .or_else(|| std::env::var("DSH_HOME").ok())
    .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
let startup_mode = settings_home
    .as_deref()
    .map(Path::new)
    .map(|home| home.join("settings.yaml"))
    .map(|path| desktop_settings::startup_mode(&desktop_settings::read_desktop_section(&path)))
    .unwrap_or("normal");
let start_in_tray = Arc::new(AtomicBool::new(
    autostart_requested && startup_mode == "tray",
));
if autostart_requested && startup_mode == "minimized" {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
}
if start_in_tray.load(Ordering::Relaxed) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}
```

`AppState` 传入 `start_in_tray: start_in_tray.clone()`。

`open_window` 改为：

```rust
fn open_window(&self, url: String) {
    if let Some(window) = self.app.get_webview_window("main") {
        if let Ok(url) = tauri::Url::parse(&url) {
            let _ = window.navigate(url);
        }
        if !self.start_in_tray.load(Ordering::Relaxed) {
            let _ = window.show();
            let _ = window.set_focus();
        }
        emit_window_state(&self.app);
    }
}
```

`DshManager` 增加字段 `start_in_tray: Arc<AtomicBool>`，并在 setup 构造时传入。

托盘“显示主窗口”处理器中，在 `show()` 前关闭启动隐藏：

```rust
TRAY_SHOW_MAIN => {
    let state = app.state::<AppState>();
    state.start_in_tray.store(false, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
```

- [x] **Step 3: bridge host 注册 desktop 命名空间**

`packages/plugins/bridge/src/index.ts` 改为：

```ts
import { installSettingsSection, settingsNamespace } from "@deepseek-ai/dsh-settings";
import { z } from "@deepseek-ai/schemastery";
import type { Context } from "@deepseek-ai/cordis";
import type { DshConfig, RuntimeSnapshot } from "@dsh-desktop/contracts";

export const name = "bridge";

const desktopSchema = z.object({
  autostart: z.boolean().default(false),
  startupMode: z.enum(["normal", "tray", "minimized"]).default("normal"),
});

export function apply(ctx: Context): void {
  installSettingsSection(
    ctx,
    settingsNamespace("desktop"),
    desktopSchema,
    { autostart: false, startupMode: "normal" },
    {
      setSource() {},
      onChange() {},
    },
  );
}
```

`packages/plugins/bridge/package.json` 的 `dependencies` 追加：

```json
"@deepseek-ai/dsh-host-webserver": "^0.1.0-rc.6"
```

（`installSettingsSection` 已由 `@deepseek-ai/dsh-settings` 导出，仓库内该依赖已存在。）

- [x] **Step 4: client 面启动模式 UI 与事务同步**

`packages/plugins/bridge/src/client.tsx` 的 `DesktopConfig` 增加 `startupMode?: "normal" | "tray" | "minimized"`。把 `AutostartSwitch` 扩展为 `StartupSettings`：

```tsx
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
    <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: "8px 0" }}>
      <label style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontWeight: 600 }}>{props.t("autostart.title")}</div>
          <div style={{ fontSize: 12, opacity: 0.6 }}>{props.t("autostart.desc")}</div>
        </div>
        <input
          type="checkbox"
          checked={autostart}
          onChange={(e) => void apply(e.currentTarget.checked, startupMode)}
          aria-label={props.t("autostart.title")}
        />
      </label>
      <label style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <div style={{ flex: 1, fontSize: 12, opacity: 0.7 }}>{props.t("autostart.mode")}</div>
        <select
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
      {error && <p style={{ fontSize: 12, color: "#d92d20", margin: 0 }}>{error}</p>}
    </div>
  );
}
```

在 `DesktopPanel` 中把 `<AutostartSwitch ... />` 替换为 `<StartupSettings ... />`。中英文案新增：

```ts
"autostart.mode": "自启后窗口状态",
"autostart.mode.normal": "正常显示",
"autostart.mode.tray": "启动到托盘",
"autostart.mode.minimized": "启动时最小化",
```

- [x] **Step 5: 验证**

Run: `cargo test parses_desktop_section && yarn typecheck`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add apps/shell/src-tauri/src/desktop_settings.rs apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/src/index.ts packages/plugins/bridge/src/client.tsx packages/plugins/bridge/package.json
git commit -m "feat: consume autostart mode and sync desktop settings"
```

---

### Task 4: 结构化深链与 pending 队列

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/src/index.ts`
- Modify: `packages/contracts/src/index.ts`

- [x] **Step 1: 定义 DeepLinkPayload 并替换原始 URL 队列**

`Inner` 中的 `pending_deeplinks` 改为 `VecDeque<DeepLinkPayload>`，并增加 `next_deep_link_id: u64`：

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DeepLinkPayload {
    id: String,
    url: String,
    raw: String,
    received_at: String,
    source: String,
    args: Vec<String>,
    cwd: String,
}

struct Inner {
    // ...
    pending_deeplinks: VecDeque<DeepLinkPayload>,
    next_deep_link_id: u64,
}
```

新增入队函数：

```rust
fn enqueue_launch_payload(
    app: &AppHandle,
    source: String,
    url: String,
    raw: String,
    args: Vec<String>,
    cwd: String,
) {
    let payload = {
        let mut inner = app.state::<AppState>().inner.lock().unwrap();
        inner.next_deep_link_id += 1;
        let id = format!("dl-{}", inner.next_deep_link_id);
        let received_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default();
        let payload = DeepLinkPayload {
            id,
            url: url.clone(),
            raw,
            received_at,
            source,
            args,
            cwd,
        };
        inner.pending_deeplinks.push_back(payload.clone());
        payload
    };
    let ready = matches!(
        app.state::<AppState>().inner.lock().unwrap().phase,
        RuntimePhase::Ready
    );
    if ready {
        let _ = app.emit("dsh-deeplink", payload);
    }
}
```

- [x] **Step 2: 深链与二次启动入口统一入队**

把 deep-link 回调替换为：

```rust
deep_link_app.deep_link().on_open_url(move |event| {
    for url in event.urls() {
        let url = url.to_string();
        enqueue_launch_payload(&deep_app, "deep_link".into(), url.clone(), url, Vec::new(), String::new());
    }
});
```

single-instance 回调替换为：

```rust
.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Some(url) = args.iter().find(|arg| arg.starts_with("dsh-desktop://")) {
            enqueue_launch_payload(app, "deep_link".into(), url.clone(), url.clone(), args.clone(), cwd.clone());
        } else {
            enqueue_launch_payload(app, "second_instance".into(), String::new(), String::new(), args.clone(), cwd.clone());
        }
    }
}))
```

`Ready` 分支中删除“while let Some(link) = inner.pending_deeplinks.pop_front() { emit }”这段补发逻辑。

- [x] **Step 3: 新增查询与确认命令**

```rust
#[tauri::command]
fn get_pending_deeplinks(state: State<AppState>) -> Vec<DeepLinkPayload> {
    state.inner.lock().unwrap().pending_deeplinks.iter().cloned().collect()
}

#[tauri::command]
fn ack_deeplink(state: State<AppState>, id: String) -> Result<(), String> {
    state.inner.lock().unwrap().pending_deeplinks.retain(|item| item.id != id);
    Ok(())
}
```

注册到 `invoke_handler`，命令名与 Task 1 权限一致。

- [x] **Step 4: BRIDGE_SCRIPT 消费并确认**

`BRIDGE_SCRIPT` 中 `onDeepLink` 替换为：

```js
getPendingDeepLinks: function () { return invoke("get_pending_deeplinks"); },
ackDeepLink: function (id) { return invoke("ack_deeplink", { id: id }); },
onDeepLink: function (cb) {
  return listen("dsh-deeplink", function (e) {
    cb(e.payload);
    invoke("ack_deeplink", { id: e.payload.id }).catch(function () {});
  }).then(function (unlisten) {
    return invoke("get_pending_deeplinks").then(function (pending) {
      pending.forEach(function (p) {
        cb(p);
        invoke("ack_deeplink", { id: p.id }).catch(function () {});
      });
      return unlisten;
    }).catch(function () { return unlisten; });
  });
},
```

`packages/plugins/bridge/src/index.ts` 的 `DshDesktopBridge` 增加：

```ts
getPendingDeepLinks(): Promise<DeepLinkPayload[]>;
ackDeepLink(id: string): Promise<void>;
```

并补一个 bridge 本地类型：

```ts
export interface DeepLinkPayload {
  id: string;
  url: string;
  raw: string;
  received_at: string;
  source: "deep_link" | "second_instance";
  args: string[];
  cwd: string;
}
```

- [x] **Step 5: 验证**

Run: `yarn typecheck && cargo build`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/src/index.ts packages/contracts/src/index.ts
git commit -m "feat: reliable structured deep links with ack queue"
```

---

### Task 5: 快捷键查询、清理、冲突提示与持久化

**Files:**
- Modify: `apps/shell/src-tauri/src/config.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/src/index.ts`
- Modify: `packages/contracts/src/index.ts`

- [x] **Step 1: DshConfig 增加快捷键持久化**

`config.rs`：

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DshConfig {
    pub dsh_bin: Option<String>,
    pub dsh_node: Option<String>,
    pub dsh_home: Option<String>,
    #[serde(default)]
    pub shortcuts: Vec<String>,
}
```

同步更新 `effective()`、测试构造与 `packages/contracts` 的 `DshConfig`（Task 1 已加）。

- [x] **Step 2: 新增快捷键快照与内部注册函数**

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ShortcutSnapshot {
    shortcut: String,
    registered: bool,
}

fn persist_shortcuts(config_path: &Path, shortcuts: &[String]) {
    let mut config = config::load(config_path);
    config.shortcuts = shortcuts.to_vec();
    let _ = config::save(config_path, &config);
}

fn register_shortcut_internal(state: &AppState, shortcut: String) -> Result<ShortcutSnapshot, String> {
    let s = Shortcut::from_str(&shortcut).map_err(|e| e.to_string())?;
    if state.shortcuts.lock().unwrap().contains_key(&shortcut) {
        return Ok(ShortcutSnapshot { shortcut, registered: true });
    }
    let app = state.app.clone();
    let trigger = shortcut.clone();
    app.global_shortcut()
        .on_shortcut(s, move |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app.emit("dsh-shortcut", trigger.clone());
            }
        })
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("already registered") || message.contains("already in use") {
                format!("快捷键 {shortcut} 已被其他应用占用")
            } else {
                format!("注册快捷键 {shortcut} 失败: {message}")
            }
        })?;
    let shortcuts = {
        let mut map = state.shortcuts.lock().unwrap();
        map.insert(shortcut.clone(), s);
        map.keys().cloned().collect::<Vec<_>>()
    };
    persist_shortcuts(&state.config_path, &shortcuts);
    Ok(ShortcutSnapshot { shortcut, registered: true })
}
```

命令改为调用内部函数：

```rust
#[tauri::command]
fn register_shortcut(state: State<AppState>, shortcut: String) -> Result<ShortcutSnapshot, String> {
    register_shortcut_internal(&state, shortcut)
}
```

- [x] **Step 3: 查询与全量清理**

```rust
#[tauri::command]
fn get_shortcuts(state: State<AppState>) -> Vec<ShortcutSnapshot> {
    state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .map(|shortcut| ShortcutSnapshot { shortcut, registered: true })
        .collect()
}

#[tauri::command]
fn unregister_all_shortcuts(state: State<AppState>) -> Result<(), String> {
    let shortcuts = state.shortcuts.lock().unwrap().keys().cloned().collect::<Vec<_>>();
    if !shortcuts.is_empty() {
        state.app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;
    }
    state.shortcuts.lock().unwrap().clear();
    persist_shortcuts(&state.config_path, &[]);
    Ok(())
}
```

`unregister_shortcut` 注销后同步调用 `persist_shortcuts`。

- [x] **Step 4: 重启恢复**

在 `setup` 中 `app.manage(AppState { ... })` 后调用：

```rust
if let Some(state) = app.try_state::<AppState>() {
    let config = config::load(&state.config_path);
    for shortcut in config.shortcuts {
        let _ = register_shortcut_internal(&state, shortcut);
    }
}
```

- [x] **Step 5: bridge 类型**

`packages/plugins/bridge/src/index.ts` 的 `shortcuts` 增加：

```ts
list(): Promise<Array<{ shortcut: string; registered: boolean }>>;
unregisterAll(): Promise<void>;
```

并在 `BRIDGE_SCRIPT` 中加：

```js
list: function () { return invoke("get_shortcuts"); },
unregisterAll: function () { return invoke("unregister_all_shortcuts"); },
```

- [x] **Step 6: 验证**

Run: `cargo test && yarn typecheck`
Expected: PASS

- [x] **Step 7: Commit**

```bash
git add apps/shell/src-tauri/src/config.rs apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/src/index.ts packages/contracts/src/index.ts
git commit -m "feat: query, clear, conflict-check and persist shortcuts"
```

---

### Task 6: 通知点击、权限与更新完成通知

**Files:**
- Create: `apps/shell/src-tauri/src/notifications.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/src/index.ts`
- Modify: `packages/contracts/src/index.ts`

- [x] **Step 1: 新增通知模块**

`apps/shell/src-tauri/src/notifications.rs`：

```rust
use serde::Serialize;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationAction {
    pub kind: String,
    pub session_id: Option<String>,
    pub url: Option<String>,
    pub path: Option<String>,
}

pub fn show(app: &AppHandle, title: &str, body: &str, action: Option<NotificationAction>) {
    let app = app.clone();
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("DSH Desktop")
        .summary(title)
        .body(body);
    #[cfg(target_os = "windows")]
    notification.app_id(&app.package_info().identifier);
    if action.is_some() {
        notification.action("default", "打开");
    }
    let Ok(handle) = notification.show() else {
        return;
    };
    thread::spawn(move || {
        handle.wait_for_response(move |response| {
            let clicked = matches!(
                response,
                notify_rust::NotificationResponse::Default
                    | notify_rust::NotificationResponse::Action(_)
            );
            if !clicked {
                return;
            }
            if let Some(action) = action {
                let _ = app.emit("dsh-notification-action", action);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
    });
}
```

在 `lib.rs` 增加 `mod notifications;`。

- [x] **Step 2: DshManager::notify 改用带回调的通知**

```rust
fn notify(&self, title: &str, body: &str) {
    notifications::show(&self.app, title, body, None);
}
```

就绪通知带点击动作：

```rust
notifications::show(
    &self.app,
    "DSH 已就绪",
    &format!("DeepSeek Harness 已启动：{url}"),
    Some(notifications::NotificationAction {
        kind: "focus".into(),
        session_id: None,
        url: Some(url.clone()),
        path: None,
    }),
);
```

- [x] **Step 3: 更新下载完成通知**

`install_update` 在 `file.sync_all()` 后、返回前追加：

```rust
let dest_string = dest.to_string_lossy().into_owned();
notifications::show(
    &app,
    "更新下载完成",
    &format!("安装包已保存到 {dest_string}"),
    Some(notifications::NotificationAction {
        kind: "open_update".into(),
        session_id: None,
        url: None,
        path: Some(dest_string.clone()),
    }),
);
Ok(dest_string)
```

- [x] **Step 4: 权限命令**

```rust
#[tauri::command]
fn request_notification_permission(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_notification::PermissionState;
    let state = app.notification().request_permission().map_err(|e| e.to_string())?;
    Ok(match state {
        PermissionState::Granted => "granted".to_string(),
        PermissionState::Prompt => "prompt".to_string(),
        PermissionState::Denied => "denied".to_string(),
    })
}
```

- [x] **Step 5: bridge 类型与脚本**

`packages/plugins/bridge/src/index.ts` 增加：

```ts
requestNotificationPermission(): Promise<"granted" | "prompt" | "denied">;
onNotificationAction(cb: (payload: NotificationActionPayload) => void): Promise<() => void>;
```

`BRIDGE_SCRIPT` 增加：

```js
requestNotificationPermission: function () { return invoke("request_notification_permission"); },
onNotificationAction: function (cb) { return listen("dsh-notification-action", function (e) { cb(e.payload); }); },
```

- [x] **Step 6: 验证**

Run: `cargo test && yarn typecheck`
Expected: PASS

- [x] **Step 7: Commit**

```bash
git add apps/shell/src-tauri/src/notifications.rs apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/src/index.ts packages/contracts/src/index.ts
git commit -m "feat: actionable notifications and permission command"
```

---

### Task 7: 端口、日志轮转、健康端点与 dsh 版本

**Files:**
- Create: `apps/shell/src-tauri/src/process.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `packages/plugins/bridge/src/index.ts`

- [x] **Step 1: 新增 process.rs 纯函数**

```rust
pub fn parse_dsh_web_url(line: &str) -> Option<String> {
    const PREFIX: &str = "dsh web: http://127.0.0.1:";
    let start = line.find(PREFIX)?;
    let rest = &line[start + PREFIX.len()..];
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    let port: u16 = rest[..end].parse().ok()?;
    Some(format!("http://127.0.0.1:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_line() {
        assert_eq!(
            parse_dsh_web_url("dsh web: http://127.0.0.1:49321"),
            Some("http://127.0.0.1:49321".into())
        );
        assert_eq!(parse_dsh_web_url("random log line"), None);
    }
}
```

`lib.rs` 增加 `mod process;`。

- [x] **Step 2: 使用 --port 0 并从 stdout 获取真实 URL**

`DshManager::start` 中删除 `let port = reserve_port()?;`，参数改为：

```rust
args.extend(["--host".into(), "127.0.0.1".into(), "--port".into(), "0".into()]);
```

删除原来的 `is_server_ready` 轮询线程，改为 stdout 检测：

```rust
let tx = self.tx.clone();
let generation = self.generation;
let stdout = child.stdout.take().expect("stdout 已开启管道");
let stderr = child.stderr.take().expect("stderr 已开启管道");
self.spawn_url_reader(stdout, generation, "stdout");
self.spawn_reader(stderr, "stderr");
```

新增：

```rust
fn spawn_url_reader(
    &self,
    stream: impl std::io::Read + Send + 'static,
    generation: u64,
    label: &'static str,
) {
    let app = self.app.clone();
    let inner = self.inner.clone();
    let log_path = self.log_path.clone();
    let tx = self.tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim_end();
            if let Some(url) = process::parse_dsh_web_url(line) {
                let _ = tx.send(ManagerMessage::Ready { generation, url });
            }
            append_line(&app, &inner, &log_path, &format!("[{label}] {line}"));
        }
    });
}
```

`reserve_port` 与 `is_server_ready` 可删除；`READY_TIMEOUT` 仍用于 `ReadyTimeout` 分支但改为“未收到 URL line 超时”。需要把 `ManagerMessage::ReadyTimeout` 的语义改成 `url_timeout` 文案，`ReadyTimeout` 仍携带 generation。

`ManagerMessage` 同时新增健康失败变体：

```rust
enum ManagerMessage {
    Start,
    Stop,
    Shutdown,
    Ready { generation: u64, url: String },
    ReadyTimeout { generation: u64 },
    InstallFinished { result: Result<(), String> },
    Unhealthy { generation: u64 },
}
```

- [x] **Step 3: 固定健康端点**

`packages/plugins/bridge/src/index.ts` 的 `apply(ctx)` 增加：

```ts
ctx.inject(["webServer"] as never, (sctx: {
  webServer: {
    register(route: {
      kind: "exact";
      path: string;
      handler: (
        req: unknown,
        res: { writeHead(code: number, headers?: Record<string, string>): void; end(body?: string): void },
      ) => void | Promise<void>;
    }): () => void;
  };
}) => {
  sctx.webServer.register({
    kind: "exact",
    path: "/dsh-desktop/health",
    handler(_req, res) {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ ok: true }));
    },
  });
});
```

`lib.rs` 增加健康轮询：

```rust
fn is_health_ready(url: &str) -> bool {
    let url = format!("{url}/dsh-desktop/health");
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], parse_port(&url))),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let request = format!("GET /dsh-desktop/health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).is_ok()
        && (line.starts_with("HTTP/1.0 200") || line.starts_with("HTTP/1.1 200"))
}

fn parse_port(url: &str) -> u16 {
    url.rsplit(':')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3080)
}
```

在 `start` 末尾启动健康线程，3 次失败发 `ManagerMessage::Unhealthy { generation }`，`run` 新增分支：

```rust
Ok(ManagerMessage::Unhealthy { generation }) => {
    if generation == self.generation {
        self.fail("DSH 健康检查连续失败".to_string());
    }
}
```

- [x] **Step 4: 日志按大小轮转**

`append_line` 调用前先轮转：

```rust
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
static LOG_LOCK: Mutex<()> = Mutex::new(());

fn rotate_log_if_needed(log_path: &Path) {
    let _guard = LOG_LOCK.lock().unwrap();
    let Ok(meta) = std::fs::metadata(log_path) else {
        return;
    };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let old = log_path.with_extension("log.1");
    let older = log_path.with_extension("log.2");
    let _ = std::fs::remove_file(&older);
    let _ = std::fs::rename(&old, &older);
    let _ = std::fs::rename(log_path, &old);
}

fn append_line(app: &AppHandle, inner: &Arc<Mutex<Inner>>, log_path: &Path, line: &str) {
    rotate_log_if_needed(log_path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{line}");
    }
    // ...原 inner.logs 与 emit 逻辑不变
}
```

- [x] **Step 5: dsh 版本与更新命令**

`Inner` 与 `RuntimeSnapshot` 增加 `dsh_version: Option<String>`。`handle_start` 中：

```rust
let version = Command::new(&node)
    .arg(&entry)
    .arg("--version")
    .output()
    .ok()
    .and_then(|output| {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            String::from_utf8_lossy(&output.stderr).trim().to_string().into()
        } else {
            text.into()
        }
    })
    .filter(|value| !value.is_empty());
self.inner.lock().unwrap().dsh_version = version;
```

新增命令：

```rust
#[tauri::command]
fn update_dsh(state: State<AppState>) -> Result<(), String> {
    if matches!(state.inner.lock().unwrap().phase, RuntimePhase::Installing) {
        return Ok(());
    }
    let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());
    let npm = resolve_npm(&config).ok_or_else(|| "未检测到 npm，无法更新 DSH".to_string())?;
    {
        let mut inner = state.inner.lock().unwrap();
        inner.phase = RuntimePhase::Installing;
        inner.message = "正在更新 DSH...".to_string();
    }
    emit_status(&state.app, &state.inner);
    let app = state.app.clone();
    let inner = state.inner.clone();
    let tx = state.tx.clone();
    let log_path = state.log_dir.join("update.log");
    thread::spawn(move || {
        append_line(&app, &inner, &log_path, "[desktop] 执行: npm install -g @deepseek-ai/dsh@latest");
        let mut child = Command::new(&npm)
            .args(["install", "-g", "@deepseek-ai/dsh@latest"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法启动 npm: {error}"));
        let result = child
            .and_then(|mut child| {
                if let Some(stdout) = child.stdout.take() {
                    append_stream(stdout, &app, &inner, &log_path, "npm");
                }
                if let Some(stderr) = child.stderr.take() {
                    append_stream(stderr, &app, &inner, &log_path, "npm");
                }
                child
                    .wait()
                    .map_err(|error| format!("npm 更新中断: {error}"))
                    .and_then(|status| {
                        if status.success() {
                            Ok(())
                        } else {
                            Err(format!("npm 更新失败 (exit {:?})", status.code()))
                        }
                    })
            });
        let _ = tx.send(ManagerMessage::InstallFinished { result });
    });
    Ok(())
}
```

bridge 增加 `updateDsh(): Promise<void>` 与 `getStatus()` 返回 `dsh_version`。

- [x] **Step 6: 验证**

Run: `cargo test && yarn typecheck`
Expected: PASS

- [x] **Step 7: Commit**

```bash
git add apps/shell/src-tauri/src/process.rs apps/shell/src-tauri/src/lib.rs packages/plugins/bridge/src/index.ts
git commit -m "feat: dynamic port, log rotation, health and dsh updates"
```

---

### Task 8: 文件拖放结构化 payload 与明确动作

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `apps/shell/src-tauri/src/process.rs`
- Modify: `packages/plugins/bridge/src/index.ts`
- Modify: `packages/contracts/src/index.ts`

- [x] **Step 1: 拖放分类纯函数**

`process.rs` 增加：

```rust
pub fn classify_drop(paths: &[PathBuf]) -> &'static str {
    if paths.is_empty() {
        return "file";
    }
    let mut has_file = false;
    let mut has_dir = false;
    for path in paths {
        if path.is_dir() {
            has_dir = true;
        } else {
            has_file = true;
        }
    }
    if has_file && has_dir {
        "mixed"
    } else if has_dir {
        "directory"
    } else {
        "file"
    }
}

#[test]
fn classifies_drop_kind() {
    let dir = std::env::temp_dir();
    assert_eq!(classify_drop(&[dir.clone()]), "directory");
    assert_eq!(classify_drop(&[]), "file");
}
```

文件顶部补充 `use std::path::PathBuf;`。

- [x] **Step 2: Rust 事件与命令**

在 `lib.rs` 定义：

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DropPosition {
    x: f64,
    y: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct FileDropPayload {
    id: String,
    paths: Vec<String>,
    kind: String,
    position: DropPosition,
    action: String,
}

static NEXT_DROP_ID: AtomicU64 = AtomicU64::new(0);

fn emit_file_drop(window: &tauri::WebviewWindow, paths: Vec<PathBuf>, action: &str) {
    let id = format!("drop-{}", NEXT_DROP_ID.fetch_add(1, Ordering::Relaxed));
    let kind = process::classify_drop(&paths);
    let payload = FileDropPayload {
        id,
        paths: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        kind: kind.to_string(),
        position: DropPosition { x: 0.0, y: 0.0 },
        action: action.to_string(),
    };
    let _ = window.emit("dsh-file-drop", payload);
}

#[tauri::command]
fn open_paths(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("未提供文件路径".to_string());
    }
    emit_file_drop(&window, paths.into_iter().map(PathBuf::from).collect(), "open");
    Ok(())
}

#[tauri::command]
fn import_paths(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("未提供目录路径".to_string());
    }
    emit_file_drop(&window, paths.into_iter().map(PathBuf::from).collect(), "import");
    Ok(())
}
```

原 `WindowEvent::DragDrop` 处理器改为：

```rust
if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position, .. }) = event {
    let payload = FileDropPayload {
        id: format!("drop-{}", NEXT_DROP_ID.fetch_add(1, Ordering::Relaxed)),
        paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        kind: process::classify_drop(&paths).to_string(),
        position: DropPosition { x: position.x, y: position.y },
        action: "open".to_string(),
    };
    let _ = window.emit("dsh-file-drop", payload);
    return;
}
```

注册 `open_paths` / `import_paths` 到 `invoke_handler`。

- [x] **Step 3: bridge 类型与脚本**

`packages/plugins/bridge/src/index.ts` 增加：

```ts
openPaths(paths: string[]): Promise<void>;
importPaths(paths: string[]): Promise<void>;
```

`BRIDGE_SCRIPT` 增加：

```js
openPaths: function (paths) { return invoke("open_paths", { paths: paths }); },
importPaths: function (paths) { return invoke("import_paths", { paths: paths }); },
```

`onFileDrop` 回调类型改为 `FileDropPayload`。

- [x] **Step 4: 验证**

Run: `cargo test classifies_drop_kind && yarn typecheck`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/src/process.rs packages/plugins/bridge/src/index.ts packages/contracts/src/index.ts
git commit -m "feat: structured file drop with explicit actions"
```

---

### Task 9: 文档、边界说明与最终验证

**Files:**
- Modify: `docs/plugin-tauri-boundary.md`
- Modify: `packages/plugins/AGENTS.md`
- Modify: `packages/plugins/bridge/AGENTS.md`
- Modify: `packages/contracts/AGENTS.md`

- [x] **Step 1: 同步桥接能力表**

在 `packages/plugins/bridge/AGENTS.md` 的桥接对象表补一行：

```markdown
| `shortcuts.list()` / `shortcuts.unregisterAll()` | `get_shortcuts` / `unregister_all_shortcuts` | 查询 / 全量清理全局快捷键 |
| `getPendingDeepLinks()` / `ackDeepLink(id)` | `get_pending_deeplinks` / `ack_deeplink` | 深链 pending 队列读取 / 确认消费 |
| `requestNotificationPermission()` / `onNotificationAction(cb)` | `request_notification_permission` / 事件 `dsh-notification-action` | 通知权限申请 / 点击动作 |
| `openPaths(paths)` / `importPaths(paths)` | `open_paths` / `import_paths` | 打开文件 / 导入目录动作 |
```

`docs/plugin-tauri-boundary.md` 增加一段“二期新增能力边界”：Tauri 壳负责托盘/自启/深链/快捷键/通知/日志/拖放；bridge host 只注册 desktop settings 与健康端点，不实现 dsh 业务。

- [x] **Step 2: 更新 contracts 文档**

`packages/contracts/AGENTS.md` 的内容清单补 `DeepLinkPayload` / `FileDropPayload` / `ShortcutSnapshot` / `NotificationActionPayload` / `StartupMode` 与新增命令/事件名。

- [x] **Step 3: 全量验证**

Run:

```bash
yarn typecheck
yarn lint
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: 全部通过。

- [x] **Step 4: 手工冒烟**

Run: `yarn dev`

Expected:

1. 托盘菜单随 phase 变化；Ready 时复制/浏览器打开/停止可用。
2. `dsh web --port 0` 从 stdout URL 进入 Ready，`/dsh-desktop/health` 返回 200。
3. 启动参数 `--autostart` 且 settings `startupMode: tray` 时主窗口隐藏到托盘。
4. 深链在页面未订阅时入队，订阅后经 `get_pending_deeplinks` 补发并 ack。
5. 快捷键重启后自动恢复，冲突返回明确错误。
6. 通知点击聚焦主窗口，更新完成通知携带安装包路径。
7. 拖入文件/目录时 payload 含 `kind`、坐标与 action。

- [x] **Step 5: Commit**

```bash
git add docs/plugin-tauri-boundary.md packages/plugins/AGENTS.md packages/plugins/bridge/AGENTS.md packages/contracts/AGENTS.md
git commit -m "docs: document native capability phase two"
```

---

## Self-Review

- 托盘：Task 2 覆盖状态、复制、浏览器打开、停止/重启、tooltip。
- 自启：Task 3 覆盖 `--autostart` 消费、tray/minimized、settings/OS 事务同步。
- 深链：Task 4 覆盖结构化 payload、pending 队列、ack、二次启动 args/cwd 透传。
- 快捷键：Task 5 覆盖查询、全量清理、冲突提示、config 持久化与重启恢复。
- 通知：Task 6 覆盖点击回调、聚焦、事件 payload、权限命令、更新完成通知。
- 进程日志：Task 7 覆盖 `--port 0` 防竞态、固定健康端点、日志轮转、dsh 版本与更新。
- 拖放：Task 8 覆盖类型、坐标、打开/导入动作。
- 文档：Task 9 同步桥接与边界说明。
