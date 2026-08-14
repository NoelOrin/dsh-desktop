use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod config;
mod desktop_settings;
mod embedded;
mod notifications;
mod process;
mod theme;

use config::DshConfig;
use theme::{read_ui_theme_section, resolve_ui_theme, UiThemeSnapshot};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, GlobalShortcutExt, Shortcut, ShortcutState,
};
use tauri_plugin_notification::NotificationExt;

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_LOGS: usize = 500;
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// 子进程意外退出后的自动重启上限（看门狗，超过则转 failed）。
const MAX_AUTO_RESTARTS: u32 = 3;
/// 优雅退出：SIGTERM 后等待子进程退出的宽限期。
const GRACE_PERIOD: Duration = Duration::from_secs(2);

static LOG_LOCK: Mutex<()> = Mutex::new(());

// 托盘菜单项 id
const TRAY_STATUS: &str = "tray-status";
const TRAY_COPY_URL: &str = "tray-copy-url";
const TRAY_OPEN_BROWSER: &str = "tray-open-browser";
const TRAY_STOP_DSH: &str = "tray-stop-dsh";
const TRAY_RESTART_DSH: &str = "tray-restart-dsh";
const TRAY_SHOW_MAIN: &str = "tray-show-main";
const TRAY_QUIT: &str = "tray-quit";

/// 注入到 dsh web（loopback 远程页面）的桥接脚本，定义 window.__DSH_DESKTOP__。
/// 仅暴露最小能力切片（见 capabilities/bridge.json 的 remote 白名单）。
const BRIDGE_SCRIPT: &str = r#"(function () {
  "use strict";
  if (window.__DSH_DESKTOP__) return;
  var internals = window.__TAURI_INTERNALS__;
  if (!internals) return;
  function invoke(cmd, args) {
    return internals.invoke(cmd, args || {});
  }
  function listen(event, cb) {
    return internals.invoke("plugin:event|listen", {
      event: event,
      target: { kind: "Any" },
      handler: internals.transformCallback(function (e) { cb(e.payload); }),
    }).then(function (eventId) {
      return function () {
        return internals.invoke("plugin:event|unlisten", {
          event: event,
          eventId: eventId,
        });
      };
    });
  }
  window.__DSH_DESKTOP__ = {
    platform: (navigator.platform || "unknown"),
    notify: function (title, body) { return invoke("plugin:notification|notify", { options: { title: title, body: body } }); },
    clipboard: {
      readText: function () { return invoke("plugin:clipboard-manager|read_text"); },
      writeText: function (text) { return invoke("plugin:clipboard-manager|write_text", { text: text }); },
    },
    dialog: {
      openFile: function (options) { return invoke("plugin:dialog|open", { options: options || {} }); },
      saveFile: function (options) { return invoke("plugin:dialog|save", { options: options || {} }); },
    },
    openExternal: function (target) { return invoke("open_external", { target: target }); },
    windowAction: function (action) { return invoke("window_action", { action: action }); },
    onWindowState: function (cb) { return listen("dsh-window-state", cb); },
    getStatus: function () { return invoke("get_status"); },
    restart: function () { return invoke("restart"); },
    installDsh: function () { return invoke("install_dsh"); },
    updateDsh: function () { return invoke("update_dsh"); },
    openLogDirectory: function () { return invoke("open_log_directory"); },
    openPaths: function (paths) { return invoke("open_paths", { paths: paths }); },
    importPaths: function (paths) { return invoke("import_paths", { paths: paths }); },
    getConfig: function () { return invoke("get_config"); },
    setConfig: function (config) { return invoke("set_config", { config: config }); },
    onStatus: function (cb) { return listen("dsh-status", cb); },
    onLog: function (cb) { return listen("dsh-log", cb); },
    onFileDrop: function (cb) { return listen("dsh-file-drop", cb); },
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
    requestNotificationPermission: function () { return invoke("request_notification_permission"); },
    onNotificationAction: function (cb) { return listen("dsh-notification-action", function (e) { cb(e.payload); }); },
    autostart: {
      get: function () { return invoke("get_autostart"); },
      set: function (enabled) { return invoke("set_autostart", { enabled: enabled }); },
    },
    desktop: {
      get: function () { return invoke("get_desktop_settings"); },
      set: function (settings) { return invoke("set_desktop_settings", { settings: settings }); },
    },
    shortcuts: {
      register: function (s, cb) {
        return invoke("register_shortcut", { shortcut: s }).then(function () {
          return listen("dsh-shortcut", function (e) { if (e.payload === s) cb(); });
        });
      },
      unregister: function (s) { return invoke("unregister_shortcut", { shortcut: s }); },
      list: function () { return invoke("get_shortcuts"); },
      unregisterAll: function () { return invoke("unregister_all_shortcuts"); },
    },
    onShortcut: function (cb) { return listen("dsh-shortcut", cb); },
    update: {
      check: function () { return invoke("check_update"); },
      install: function () { return invoke("install_update"); },
    },
  };
})();"#;

/// 注入 dsh web 的自绘标题栏脚本（参考参考仓库 harness-chrome-inject.js，适配 Tauri data-tauri-drag-region）。
const HARNESS_CHROME_SCRIPT: &str = r##"(function () {
  "use strict";
  if (document.getElementById("dsh-shell-controls")) return;
  var STYLE_ID = "dsh-shell-chrome-style";
  var CONTROLS_ID = "dsh-shell-controls";
  var DRAG_ID = "dsh-shell-drag-strip";
  var EDGE = 8;
  var SIZE = 32;
  var GAP = 0;
  var MAC_TRAFFIC_WIDTH = 80;
  var MAC_DRAG_HEIGHT = 16;
  var MAC_SIDEBAR_INSET = 20;

  function detectPlatform() {
    var ua = navigator.userAgent || "";
    if (/Mac|iPhone|iPad/.test(ua) && !/Windows/.test(ua)) return "macos";
    if (/Win/.test(ua)) return "windows";
    return "linux";
  }

  var PLATFORM = detectPlatform();

  var ICON_MIN = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2" y="5.4" width="8" height="1.2" rx="0.6" fill="currentColor"/></svg>';
  var ICON_MAX = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>';
  var ICON_RESTORE = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="3.4" y="2.2" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/><rect x="2.2" y="3.6" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/></svg>';
  var ICON_CLOSE = '<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3L3 9" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></svg>';

  function reservedEdge() {
    return EDGE + SIZE * 3 + GAP * 2;
  }

  function ensureStyle() {
    var style = document.getElementById(STYLE_ID);
    if (style) return;
    style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = [
      "#" + CONTROLS_ID + " {",
      "  position: fixed;",
      "  top: 4px;",
      "  " + (PLATFORM === "macos" ? "left" : "right") + ": " + EDGE + "px;",
      "  z-index: 2147483647;",
      "  display: flex;",
      "  gap: " + GAP + "px;",
      "  height: " + SIZE + "px;",
      "}",
      "#" + CONTROLS_ID + " button {",
      "  width: " + SIZE + "px;",
      "  height: " + SIZE + "px;",
      "  margin: 0;",
      "  padding: 0;",
      "  display: inline-flex;",
      "  align-items: center;",
      "  justify-content: center;",
      "  border: 0;",
      "  border-radius: 8px;",
      "  background: transparent;",
      "  color: var(--dsh-ctrl-fg, #3f3f46);",
      "  cursor: pointer;",
      "}",
      "#" + CONTROLS_ID + " button svg { width: 12px; height: 12px; display: block; }",
      "#" + CONTROLS_ID + " button:hover { background: var(--dsh-ctrl-hover, rgba(0, 0, 0, 0.08)); }",
      "#" + CONTROLS_ID + " button[data-act=close]:hover { background: #e81123; color: #fff; }",
      "#" + CONTROLS_ID + "[data-platform=macos] { right: auto; }",
      "#" + CONTROLS_ID + "[data-platform=macos] { display: none; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button { width: 12px; height: 12px; border-radius: 50%; color: transparent; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button svg { display: none; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button[data-act=close] { order: 1; background: #ff5f57; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button[data-act=minimize] { order: 2; background: #febc2e; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button[data-act=maximize] { order: 3; background: #28c840; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button:hover { background: inherit; filter: brightness(0.92); color: transparent; }",
      "#" + CONTROLS_ID + "[data-platform=macos] button[data-act=close]:hover { background: #ff5f57; color: transparent; }",
      "#" + DRAG_ID + " {",
      "  position: fixed;",
      "  top: 0;",
      "  " + (PLATFORM === "macos" ? "left: " + MAC_TRAFFIC_WIDTH + "px;" : "left: 0;"),
      "  " + (PLATFORM === "macos" ? "right: 0;" : "right: " + reservedEdge() + "px;"),
      "  height: " + (PLATFORM === "macos" ? MAC_DRAG_HEIGHT : 44) + "px;",
      "  z-index: 2147483644;",
      "}"
    ].join("\n");
    (document.head || document.documentElement).appendChild(style);
  }

  function findSidebarRoot() {
    // 侧栏根节点用稳定的 data-slot 定位，避免依赖 CSS module 哈希 class。
    var slot = document.querySelector('[data-slot="sidebar"]');
    var root = slot && slot.firstElementChild;
    return root instanceof HTMLElement ? root : null;
  }

  function applyMacSidebarInset() {
    if (PLATFORM !== "macos") return true;
    var sidebar = findSidebarRoot();
    if (!sidebar) return false;
    var prevTop = parseFloat(sidebar.style.paddingTop) || 0;
    sidebar.style.paddingTop = Math.max(prevTop, MAC_SIDEBAR_INSET) + "px";
    return true;
  }

  function findTopBar() {
    var buttons = document.querySelectorAll("button");
    for (var i = 0; i < buttons.length; i++) {
      var label = (buttons[i].getAttribute("aria-label") || "") + " " + (buttons[i].textContent || "");
      if (/session\s*log/i.test(label)) {
        var header = buttons[i].closest("header");
        if (header) return header;
      }
    }
    var nodes = document.querySelectorAll("header, [role=banner]");
    for (var j = 0; j < nodes.length; j++) {
      var rect = nodes[j].getBoundingClientRect();
      if (rect.top <= 8 && rect.height >= 32 && rect.height <= 160) return nodes[j];
    }
    return null;
  }

  function ensureControls() {
    var host = document.getElementById(CONTROLS_ID);
    if (host) return host;
    host = document.createElement("div");
    host.id = CONTROLS_ID;
    host.innerHTML = [
      '<button type="button" data-act="minimize" aria-label="最小化">' + ICON_MIN + "</button>",
      '<button type="button" data-act="maximize" aria-label="最大化">' + ICON_MAX + "</button>",
      '<button type="button" data-act="close" aria-label="关闭">' + ICON_CLOSE + "</button>"
    ].join("");
    host.setAttribute("data-platform", PLATFORM);
    host.addEventListener("click", function (event) {
      var button = event.target.closest("[data-act]");
      if (!button || !window.__DSH_DESKTOP__) return;
      window.__DSH_DESKTOP__.windowAction(button.dataset.act);
    });
    (document.body || document.documentElement).appendChild(host);
    return host;
  }

  function ensureDragStrip() {
    var strip = document.getElementById(DRAG_ID);
    if (!strip) {
      strip = document.createElement("div");
      strip.id = DRAG_ID;
      strip.setAttribute("data-tauri-drag-region", "deep");
      (document.body || document.documentElement).appendChild(strip);
    }
    return strip;
  }

  function applyControlTheme(host) {
    var fg = getComputedStyle(document.body).getPropertyValue("--dsw-alias-label-primary").trim();
    if (!fg) {
      fg = getComputedStyle(document.body).getPropertyValue("color") || "#3f3f46";
    }
    host.style.setProperty("--dsh-ctrl-fg", fg);
    host.style.setProperty("--dsh-ctrl-hover", "rgba(128, 128, 128, 0.18)");
  }

  function install() {
    ensureStyle();
    var host = PLATFORM === "macos" ? null : ensureControls();
    var bar = findTopBar();
    var strip = ensureDragStrip();
    if (!applyMacSidebarInset()) {
      var observer = new MutationObserver(function () {
        if (applyMacSidebarInset()) observer.disconnect();
      });
      observer.observe(document.documentElement || document.body, { childList: true, subtree: true });
    }
    if (bar && bar instanceof HTMLElement) {
      if (PLATFORM !== "macos") {
        // 非 mac 顶部栏自身作为拖拽区，注入条不拦截原按钮交互。
        strip.style.pointerEvents = "none";
        bar.setAttribute("data-tauri-drag-region", "deep");
      }
      var reserved = PLATFORM === "macos" ? MAC_TRAFFIC_WIDTH : reservedEdge();
      var prev = parseFloat(PLATFORM === "macos" ? bar.style.paddingLeft : bar.style.paddingRight) || 0;
      var nextPadding = Math.max(prev, reserved) + "px";
      if (PLATFORM === "macos") {
        bar.style.paddingLeft = nextPadding;
      } else {
        bar.style.paddingRight = nextPadding;
      }
    }
    if (host) applyControlTheme(host);
    if (host && window.__DSH_DESKTOP__ && typeof window.__DSH_DESKTOP__.onWindowState === "function") {
      window.__DSH_DESKTOP__.onWindowState(function (state) {
        var maximized = !!(state && state.maximized);
        var maxBtn = host.querySelector("[data-act=maximize]");
        if (maxBtn) {
          maxBtn.innerHTML = maximized ? ICON_RESTORE : ICON_MAX;
          maxBtn.setAttribute("aria-label", maximized ? "还原" : "最大化");
        }
      });
    }
  }

  install();
})();"##;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
enum RuntimePhase {
    #[default]
    Detecting,
    #[serde(rename = "missing")]
    MissingDsh,
    Installing,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Clone, Serialize)]
struct RuntimeSnapshot {
    phase: RuntimePhase,
    message: String,
    url: Option<String>,
    dsh_installed: bool,
    node_found: bool,
    dsh_version: Option<String>,
    log_dir: Option<String>,
    logs: Vec<String>,
}

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

#[derive(Default)]
struct Inner {
    phase: RuntimePhase,
    message: String,
    url: Option<String>,
    dsh_installed: bool,
    node_found: bool,
    dsh_version: Option<String>,
    log_dir: Option<PathBuf>,
    logs: VecDeque<String>,
    /// 尚未被 dsh web 消费的深链 payload，由 bridge 主动查询并确认。
    pending_deeplinks: VecDeque<DeepLinkPayload>,
    next_deep_link_id: u64,
}

impl Inner {
    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            phase: self.phase.clone(),
            message: self.message.clone(),
            url: self.url.clone(),
            dsh_installed: self.dsh_installed,
            node_found: self.node_found,
            dsh_version: self.dsh_version.clone(),
            log_dir: self
                .log_dir
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            logs: self.logs.iter().rev().take(200).cloned().collect(),
        }
    }
}

struct AppState {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    tx: Sender<ManagerMessage>,
    log_dir: PathBuf,
    config_path: PathBuf,
    desktop_settings_path: PathBuf,
    /// 应用是否正在退出（托盘"退出"置 true，用于关闭到托盘时区分真正退出）。
    exiting: Arc<AtomicBool>,
    /// --autostart + settings startupMode=tray 时隐藏主窗口，直到用户从托盘唤起。
    start_in_tray: Arc<AtomicBool>,
    /// 已注册的自定义全局快捷键注册表（快捷键字符串 → Shortcut），供注销时查表。
    shortcuts: Arc<Mutex<HashMap<String, Shortcut>>>,
}

fn enqueue_launch_payload(
    app: &AppHandle,
    source: String,
    url: String,
    raw: String,
    args: Vec<String>,
    cwd: String,
) {
    let state = app.state::<AppState>();
    let payload = {
        let mut inner = state.inner.lock().unwrap();
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
    let ready = matches!(state.inner.lock().unwrap().phase, RuntimePhase::Ready);
    if ready {
        let _ = app.emit("dsh-deeplink", payload);
    }
}

enum ManagerMessage {
    Start,
    Stop,
    Shutdown,
    Ready { generation: u64, url: String },
    ReadyTimeout { generation: u64 },
    InstallFinished { result: Result<(), String> },
    Unhealthy { generation: u64 },
}

struct DshManager {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    tx: Sender<ManagerMessage>,
    rx: Receiver<ManagerMessage>,
    child: Option<Child>,
    generation: u64,
    /// 连续自动重启计数（看门狗），手动 Start 时清零。
    auto_restarts: u32,
    log_path: PathBuf,
    config_path: PathBuf,
    start_in_tray: Arc<AtomicBool>,
}

pub fn run() {
    let autostart_requested = std::env::args().any(|arg| arg == "--autostart");
    // 全局快捷键插件：预置快捷键（CmdOrCtrl+Shift+C 打开控制中心）已随控制中心移除；
    // 自定义快捷键由 dsh 插件经 register_shortcut / unregister_shortcut 桥接命令注册。
    let shortcut_plugin = ShortcutBuilder::new().build();

    let exiting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        // 单实例锁必须最先注册（插件按注册顺序执行）
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            if app.try_state::<AppState>().is_some() {
                if let Some(url) = args.iter().find(|arg| arg.starts_with("dsh-desktop://")) {
                    enqueue_launch_payload(
                        app,
                        "deep_link".into(),
                        url.clone(),
                        url.clone(),
                        args.clone(),
                        cwd.clone(),
                    );
                } else {
                    enqueue_launch_payload(
                        app,
                        "second_instance".into(),
                        String::new(),
                        String::new(),
                        args.clone(),
                        cwd.clone(),
                    );
                }
            }
        }))
        .plugin(shortcut_plugin)
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        // 窗口状态记忆：重启后恢复 main 窗口大小/位置/最大化
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        // 深链 dsh-desktop://（macOS 经 RunEvent::Opened；Windows/Linux 由 single-instance 转发）
        .plugin(tauri_plugin_deep_link::init())
        // 开机自启（默认关闭，由 dsh 插件设置面板控制）
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        // 向 dsh web（loopback 远程页面）注入受控桥接 window.__DSH_DESKTOP__
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished && is_dsh_web_url(payload.url()) {
                let _ = webview.eval(BRIDGE_SCRIPT);
                let _ = webview.eval(HARNESS_CHROME_SCRIPT);
            }
        })
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let app_data = app_handle.path().app_data_dir()?;
            let log_dir = app_data.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            let log_path = log_dir.join("dsh.log");
            let config_path = app_data.join("config.json");
            let desktop_settings_path = app_data.join("desktop-settings.json");

            let inner = Arc::new(Mutex::new(Inner {
                log_dir: Some(log_dir.clone()),
                ..Inner::default()
            }));

            let (tx, rx) = mpsc::channel();

            let launch_config = config::load(&config_path).effective(|k| std::env::var(k).ok());
            let settings_home = launch_config
                .dsh_home
                .clone()
                .or_else(|| std::env::var("DSH_HOME").ok())
                .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
            let startup_mode = desktop_settings::read_startup_mode(&desktop_settings_path)
                .unwrap_or_else(|| {
                    settings_home
                        .as_deref()
                        .map(Path::new)
                        .map(|home| home.join("settings.yaml"))
                        .map(|path| {
                            desktop_settings::startup_mode(&desktop_settings::read_desktop_section(
                                &path,
                            ))
                        })
                        .unwrap_or(desktop_settings::StartupMode::Normal)
                });
            let start_in_tray = Arc::new(AtomicBool::new(
                autostart_requested && startup_mode == desktop_settings::StartupMode::Tray,
            ));
            if autostart_requested && startup_mode == desktop_settings::StartupMode::Minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.minimize();
                }
            }
            if start_in_tray.load(Ordering::Relaxed) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            let manager = DshManager {
                app: app_handle.clone(),
                inner: inner.clone(),
                tx: tx.clone(),
                rx,
                child: None,
                generation: 0,
                auto_restarts: 0,
                log_path,
                config_path: config_path.clone(),
                start_in_tray: start_in_tray.clone(),
            };
            thread::spawn(move || manager.run());

            let theme_app = app_handle.clone();
            let config_path_for_theme = config_path.clone();
            app.manage(AppState {
                app: app_handle,
                inner: inner.clone(),
                tx: tx.clone(),
                log_dir,
                config_path,
                desktop_settings_path,
                exiting: exiting.clone(),
                start_in_tray: start_in_tray.clone(),
                shortcuts: Arc::new(Mutex::new(HashMap::new())),
            });

            // 重启后恢复上次持久化的全局快捷键（注册冲突仅跳过，不阻塞启动）
            if let Some(state) = app.try_state::<AppState>() {
                let config = config::load(&state.config_path);
                for shortcut in config.shortcuts {
                    let _ = register_shortcut_internal(&state, shortcut);
                }
            }

            // 深链 dsh-desktop://：统一入队，就绪时转发给 dsh web，由 bridge 查询并确认
            let deep_app = app.handle().clone();
            let deep_link_app = deep_app.clone();
            deep_link_app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let url = url.to_string();
                    enqueue_launch_payload(
                        &deep_app,
                        "deep_link".into(),
                        url.clone(),
                        url,
                        Vec::new(),
                        String::new(),
                    );
                }
            });

            let start_tx = tx.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                let _ = start_tx.send(ManagerMessage::Start);
            });

            // 主题跟随：轮询 settings.yaml 的 ui-theme 分节，变化时发 dsh-ui-theme 并更新窗口背景
            thread::spawn(move || {
                let mut last: Option<String> = None;
                loop {
                    thread::sleep(Duration::from_secs(2));
                    let config =
                        config::load(&config_path_for_theme).effective(|k| std::env::var(k).ok());
                    let home = config
                        .dsh_home
                        .clone()
                        .or_else(|| std::env::var("DSH_HOME").ok())
                        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
                    let Some(home) = home else {
                        continue;
                    };
                    let settings_path = Path::new(&home).join("settings.yaml");
                    let system_dark = theme_app
                        .get_webview_window("main")
                        .and_then(|w| w.theme().ok())
                        .map(|t| t == tauri::Theme::Dark)
                        .unwrap_or(false);
                    let section = read_ui_theme_section(&settings_path);
                    let snapshot = resolve_ui_theme(&section, system_dark);
                    let key = format!(
                        "{}:{}:{}",
                        snapshot.preference, snapshot.mode, snapshot.tokens.bg
                    );
                    if last.as_deref() == Some(key.as_str()) {
                        continue;
                    }
                    last = Some(key);
                    let _ = theme_app.emit("dsh-ui-theme", &snapshot);
                    if let Some(window) = theme_app.get_webview_window("main") {
                        if let Some(color) = parse_window_color(&snapshot.tokens.bg) {
                            let _ = window.set_background_color(Some(color));
                        }
                    }
                }
            });

            let tray_state = setup_tray(app.handle(), exiting.clone())?;
            app.manage(tray_state);

            // 自动更新：后台检查 GitHub Release，发现新版本 emit dsh-update-available（payload 为新版本号），失败仅记日志
            let updater_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match latest_release_tag().await {
                    Ok(tag) => {
                        let is_new = parse_release_tag(&tag)
                            .map(|v| v > updater_app.package_info().version)
                            .unwrap_or(false);
                        if is_new {
                            let _ = updater_app.emit("dsh-update-available", tag);
                        }
                    }
                    Err(e) => {
                        eprintln!("update check failed: {e}");
                    }
                }
            });

            emit_window_state(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_pending_deeplinks,
            ack_deeplink,
            restart,
            install_dsh,
            update_dsh,
            open_log_directory,
            get_config,
            set_config,
            open_external,
            get_autostart,
            set_autostart,
            get_desktop_settings,
            set_desktop_settings,
            register_shortcut,
            unregister_shortcut,
            get_shortcuts,
            unregister_all_shortcuts,
            check_update,
            install_update,
            request_notification_permission,
            get_ui_theme,
            window_action,
            open_paths,
            import_paths
        ])
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            // 文件拖放：把真实路径通过 dsh-file-drop 事件转给前端（dsh web 经桥接订阅）
            if label == "main" {
                if let WindowEvent::Resized(_) = event {
                    emit_window_state(window.app_handle());
                    return;
                }
                if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop {
                    paths, position, ..
                }) = event
                {
                    let payload = FileDropPayload {
                        id: format!("drop-{}", NEXT_DROP_ID.fetch_add(1, Ordering::Relaxed)),
                        paths: paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect(),
                        kind: process::classify_drop(paths).to_string(),
                        position: DropPosition {
                            x: position.x,
                            y: position.y,
                        },
                        action: "open".to_string(),
                    };
                    let _ = window.emit("dsh-file-drop", payload);
                    return;
                }
                // 系统主题变化：透传给前端（启动页监听 dsh-theme）
                if let WindowEvent::ThemeChanged(theme) = event {
                    let theme = match theme {
                        tauri::Theme::Dark => "dark",
                        _ => "light",
                    };
                    let _ = window.emit("dsh-theme", theme);
                    return;
                }
            }
            // main 窗口：关闭到托盘（非真正退出时仅隐藏）
            if label == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let exiting = window.app_handle().state::<AppState>().exiting.clone();
                    if exiting.load(Ordering::Relaxed) {
                        return; // 真正退出，放行关闭
                    }
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building DSH Desktop")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                // Command+W 关闭到托盘后，点击 Dock 图标重新唤起主窗口。
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                return;
            }

            if let RunEvent::Exit = event {
                let _ = app.state::<AppState>().tx.send(ManagerMessage::Shutdown);
            }
        });
}

/// 创建系统托盘：常驻后台，菜单按运行阶段动态更新状态与可用性。
fn setup_tray(app: &tauri::AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<TrayState> {
    let status = MenuItem::with_id(app, TRAY_STATUS, "DSH: 检测中", false, None::<&str>)?;
    let copy_url = MenuItem::with_id(app, TRAY_COPY_URL, "复制 Web UI 地址", false, None::<&str>)?;
    let open_browser =
        MenuItem::with_id(app, TRAY_OPEN_BROWSER, "用浏览器打开", false, None::<&str>)?;
    let stop_dsh = MenuItem::with_id(app, TRAY_STOP_DSH, "停止 dsh", false, None::<&str>)?;
    let restart_dsh = MenuItem::with_id(app, TRAY_RESTART_DSH, "重启 dsh", false, None::<&str>)?;
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT, "退出", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &sep1,
            &copy_url,
            &open_browser,
            &stop_dsh,
            &restart_dsh,
            &sep2,
            &show_main,
            &quit,
        ],
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
                let state = app.state::<AppState>();
                state.start_in_tray.store(false, Ordering::Relaxed);
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

/// 托盘句柄与菜单项需要保活，供状态更新时修改文案与可用性。
struct TrayState {
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    status: tauri::menu::MenuItem<tauri::Wry>,
    copy_url: tauri::menu::MenuItem<tauri::Wry>,
    open_browser: tauri::menu::MenuItem<tauri::Wry>,
    stop_dsh: tauri::menu::MenuItem<tauri::Wry>,
    restart_dsh: tauri::menu::MenuItem<tauri::Wry>,
}

/// 判断是否为 dsh web 的 loopback 页面（用于桥接注入）。
fn is_dsh_web_url(url: &tauri::Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

impl DshManager {
    fn run(mut self) {
        loop {
            match self.rx.recv_timeout(POLL_INTERVAL) {
                Ok(ManagerMessage::Start) => {
                    self.auto_restarts = 0;
                    self.handle_start();
                }
                Ok(ManagerMessage::Stop) => {
                    self.cleanup_child();
                    self.set_phase(RuntimePhase::Stopped, "已停止".to_string(), None);
                }
                Ok(ManagerMessage::Shutdown) => {
                    self.cleanup_child();
                    break;
                }
                Ok(ManagerMessage::Ready { generation, url }) => {
                    if generation == self.generation {
                        self.set_phase(
                            RuntimePhase::Ready,
                            format!("DSH 已就绪: {url}"),
                            Some(url.clone()),
                        );
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
                        self.open_window(url);
                    }
                }
                Ok(ManagerMessage::ReadyTimeout { generation }) => {
                    if generation == self.generation {
                        self.fail("DSH 未输出 URL line，等待超时".to_string());
                    }
                }
                Ok(ManagerMessage::InstallFinished { result }) => match result {
                    Ok(()) => {
                        self.append_log("[desktop] DSH 安装完成");
                        self.notify("DSH 安装完成", "DeepSeek Harness 安装成功，正在启动…");
                        self.auto_restarts = 0;
                        self.handle_start();
                    }
                    Err(error) => self.fail(format!("DSH 安装失败: {error}")),
                },
                Ok(ManagerMessage::Unhealthy { generation }) => {
                    if generation == self.generation {
                        self.fail("DSH 健康检查连续失败".to_string());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(exit) = self.take_exit() {
                        // 看门狗：运行中（starting/ready）意外退出时自动重启，超过上限才转 failed
                        let running = matches!(
                            self.inner.lock().unwrap().phase,
                            RuntimePhase::Starting | RuntimePhase::Ready
                        );
                        if running && self.auto_restarts < MAX_AUTO_RESTARTS {
                            self.auto_restarts += 1;
                            self.append_log(&format!(
                                "[desktop] DSH 进程意外退出 ({exit})，{}/{} 自动重启",
                                self.auto_restarts, MAX_AUTO_RESTARTS
                            ));
                            self.handle_start();
                        } else {
                            self.fail(format!("DSH 进程已退出 ({exit})"));
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.cleanup_child();
                    break;
                }
            }
        }
    }

    fn handle_start(&mut self) {
        self.cleanup_child();
        self.cleanup_stale_dsh_web();

        let config = config::load(&self.config_path).effective(|k| std::env::var(k).ok());
        let (node, entry) = match resolve_dsh(&config) {
            Some(pair) => pair,
            None => {
                let node_found = resolve_node(&config).is_some();
                let message = if node_found {
                    "未检测到 DSH"
                } else {
                    "未检测到 Node.js，请先安装 Node.js"
                };
                self.update_detection(node_found, false);
                self.set_phase(RuntimePhase::MissingDsh, message.to_string(), None);
                return;
            }
        };

        self.update_detection(true, true);
        let version = Command::new(&node)
            .arg(&entry)
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if text.is_empty() {
                    String::from_utf8_lossy(&output.stderr)
                        .trim()
                        .to_string()
                        .into()
                } else {
                    text.into()
                }
            })
            .filter(|value| !value.is_empty());
        self.inner.lock().unwrap().dsh_version = version;
        self.set_phase(RuntimePhase::Starting, "正在启动 DSH...".to_string(), None);
        if let Err(error) = self.start(node, entry, config.dsh_home) {
            self.fail(error);
        }
    }

    fn start(&mut self, node: PathBuf, entry: PathBuf, home: Option<String>) -> Result<(), String> {
        self.cleanup_child();

        let workspace = std::env::var("HOME").unwrap_or_else(|_| ".".into());

        self.generation += 1;
        let generation = self.generation;

        // 装配内嵌插件（best-effort）并生成 --patch overlay：任何失败只记日志，不影响 dsh 启动
        let overlay = {
            let plugins_resource =
                embedded::plugins_resource_dir(self.app.path().resource_dir().ok().as_deref());
            let home_path = home
                .as_ref()
                .map(|h| PathBuf::from(h.as_str()))
                .or_else(|| dirs::home_dir().map(|h| h.join(".dsh")));
            let mut log = |line: &str| self.append_log(line);
            let mounted = match (plugins_resource, home_path) {
                (Some(res), Some(home)) => {
                    if cfg!(debug_assertions) {
                        embedded::assemble_dev(&res, &home, &mut log)
                    } else {
                        embedded::assemble(&res, &home, &mut log)
                    }
                }
                _ => Vec::new(),
            };
            self.app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|data_dir| embedded::write_overlay(&data_dir, &mounted))
        };

        let mut cmd = Command::new(&node);
        let mut args = vec!["web".to_string()];
        if let Some(overlay_path) = &overlay {
            args.push("--patch".into());
            args.push(overlay_path.to_string_lossy().into_owned());
        }
        args.extend([
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            "0".into(),
        ]);
        cmd.arg(&entry)
            .args(&args)
            .env("NO_COLOR", "1")
            .current_dir(&workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // 独立进程组：便于优雅退出时对整棵进程树发信号
            cmd.process_group(0);
        }
        if let Some(home) = &home {
            cmd.env("DSH_HOME", home);
        }
        cmd.env("DSH_DESKTOP_MANAGED", "1");

        let mut child = cmd
            .spawn()
            .map_err(|error| format!("无法启动 dsh: {error}"))?;
        let overlay_log = overlay
            .as_ref()
            .map(|path| format!(" --patch {}", path.to_string_lossy()))
            .unwrap_or_default();
        self.append_log(&format!(
            "[desktop] 启动 dsh: {} {} web{overlay_log} --host 127.0.0.1 --port 0",
            node.display(),
            entry.display()
        ));

        let stdout = child.stdout.take().expect("stdout 已开启管道");
        let stderr = child.stderr.take().expect("stderr 已开启管道");
        self.spawn_url_reader(stdout, generation, "stdout");
        self.spawn_reader(stderr, "stderr");
        self.child = Some(child);

        self.spawn_health_checker();

        Ok(())
    }

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

    fn fail(&mut self, message: String) {
        self.cleanup_child();
        self.append_log(&format!("[desktop] 失败: {message}"));
        self.notify("DSH 启动失败", &message);
        self.set_phase(RuntimePhase::Failed, message, None);
    }

    /// 优雅停止子进程：unix 下先 SIGTERM 等宽限期，再 SIGKILL；Windows 直接 TerminateProcess。
    fn cleanup_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            self.generation += 1;
            let pid = child.id() as i32;
            #[cfg(unix)]
            {
                // 子进程以进程组启动（start 中 process_group(0)），对负 pid 发 SIGTERM 可整组清理
                unsafe { libc::kill(-pid, libc::SIGTERM) };
                let deadline = Instant::now() + GRACE_PERIOD;
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    if Instant::now() >= deadline {
                        unsafe { libc::kill(-pid, libc::SIGKILL) };
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 清理崩溃/异常退出后残留的本应用 dsh web 实例。
    fn cleanup_stale_dsh_web(&self) {
        let Ok(app_data_dir) = self.app.path().app_data_dir() else {
            return;
        };
        let count = process::cleanup_stale_dsh_web(&app_data_dir);
        if count > 0 {
            self.append_log(&format!("[desktop] 已清理 {} 个旧 dsh web 实例", count));
        }
    }

    fn take_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(exit_summary(&status))
    }

    fn spawn_reader(&self, stream: impl std::io::Read + Send + 'static, label: &'static str) {
        let app = self.app.clone();
        let inner = self.inner.clone();
        let log_path = self.log_path.clone();
        thread::spawn(move || {
            append_stream(stream, &app, &inner, &log_path, label);
        });
    }

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

    fn spawn_health_checker(&self) {
        let inner = self.inner.clone();
        let tx = self.tx.clone();
        let generation = self.generation;
        thread::spawn(move || {
            let started = Instant::now();
            let url = loop {
                if let Some(url) = inner.lock().unwrap().url.clone() {
                    break url;
                }
                if started.elapsed() >= READY_TIMEOUT {
                    let _ = tx.send(ManagerMessage::ReadyTimeout { generation });
                    return;
                }
                thread::sleep(POLL_INTERVAL);
            };

            let mut failures = 0u32;
            loop {
                if is_health_ready(&url) {
                    failures = 0;
                } else {
                    failures += 1;
                    if failures >= 3 {
                        let _ = tx.send(ManagerMessage::Unhealthy { generation });
                        return;
                    }
                }
                thread::sleep(POLL_INTERVAL);
            }
        });
    }

    fn update_detection(&self, node_found: bool, dsh_installed: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.node_found = node_found;
        inner.dsh_installed = dsh_installed;
    }

    fn set_phase(&self, phase: RuntimePhase, message: String, url: Option<String>) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.phase = phase;
            inner.message = message;
            inner.url = url;
        }
        emit_status(&self.app, &self.inner);
    }

    fn append_log(&self, line: &str) {
        append_line(&self.app, &self.inner, &self.log_path, line);
    }

    /// 发送系统原生通知（就绪 / 失败 / 安装完成等关键节点）。
    fn notify(&self, title: &str, body: &str) {
        notifications::show(&self.app, title, body, None);
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> RuntimeSnapshot {
    state.inner.lock().unwrap().snapshot()
}

#[tauri::command]
fn get_pending_deeplinks(state: State<AppState>) -> Vec<DeepLinkPayload> {
    state
        .inner
        .lock()
        .unwrap()
        .pending_deeplinks
        .iter()
        .cloned()
        .collect()
}

#[tauri::command]
fn ack_deeplink(state: State<AppState>, id: String) -> Result<(), String> {
    state
        .inner
        .lock()
        .unwrap()
        .pending_deeplinks
        .retain(|item| item.id != id);
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WindowStateSnapshot {
    maximized: bool,
}

fn emit_window_state(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = app.emit(
            "dsh-window-state",
            WindowStateSnapshot {
                maximized: window.is_maximized().unwrap_or(false),
            },
        );
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ShortcutSnapshot {
    shortcut: String,
    registered: bool,
}

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
    emit_file_drop(
        &window,
        paths.into_iter().map(PathBuf::from).collect(),
        "open",
    );
    Ok(())
}

#[tauri::command]
fn import_paths(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("未提供目录路径".to_string());
    }
    emit_file_drop(
        &window,
        paths.into_iter().map(PathBuf::from).collect(),
        "import",
    );
    Ok(())
}

/// 无边框窗口控制：minimize / maximize（切换）/ close。
#[tauri::command]
fn window_action(window: tauri::WebviewWindow, action: String) -> Result<(), String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => window.close().map_err(|e| e.to_string()),
        other => Err(format!("未知窗口动作: {other}")),
    }
}

/// #rrggbb → tauri::window::Color
fn parse_window_color(hex: &str) -> Option<tauri::window::Color> {
    let value = hex.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(tauri::window::Color(r, g, b, 255))
}

#[tauri::command]
fn get_ui_theme(app: AppHandle) -> UiThemeSnapshot {
    let system_dark = app
        .get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .map(|t| t == tauri::Theme::Dark)
        .unwrap_or(false);
    let home = std::env::var("DSH_HOME")
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
    let section = match home {
        Some(home) => read_ui_theme_section(&Path::new(&home).join("settings.yaml")),
        None => theme::UiThemeSection::default(),
    };
    resolve_ui_theme(&section, system_dark)
}

#[tauri::command]
fn restart(state: State<AppState>) -> Result<(), String> {
    state
        .tx
        .send(ManagerMessage::Start)
        .map_err(|_| "进程管理器已退出".to_string())
}

#[tauri::command]
fn install_dsh(state: State<AppState>) -> Result<(), String> {
    let phase = state.inner.lock().unwrap().phase.clone();
    if matches!(phase, RuntimePhase::Installing) {
        return Ok(());
    }

    let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());
    // 无 Node/npm 环境无法执行全局安装：前端在缺 Node 时不显示安装按钮，这里兜底返回清晰错误
    if resolve_npm(&config).is_none() {
        return Err("未检测到 npm，无法安装 DSH（请先安装 Node.js）".to_string());
    }

    {
        let mut inner = state.inner.lock().unwrap();
        inner.phase = RuntimePhase::Installing;
        inner.message = "正在安装 DSH...".to_string();
        inner.logs.clear();
    }
    emit_status(&state.app, &state.inner);

    let app = state.app.clone();
    let inner = state.inner.clone();
    let tx = state.tx.clone();
    let log_path = state.log_dir.join("install.log");
    thread::spawn(move || {
        let result = run_install(&config, &log_path, &app, &inner);
        let _ = tx.send(ManagerMessage::InstallFinished { result });
    });
    Ok(())
}

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
        append_line(
            &app,
            &inner,
            &log_path,
            "[desktop] 执行: npm install -g @deepseek-ai/dsh@latest",
        );
        let child = Command::new(&npm)
            .args(["install", "-g", "@deepseek-ai/dsh@latest"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法启动 npm: {error}"));
        let result = child.and_then(|mut child| {
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

#[tauri::command]
fn open_log_directory(state: State<AppState>) -> Result<(), String> {
    open_with_system(&state.log_dir.to_string_lossy())
}

#[tauri::command]
fn get_config(state: State<AppState>) -> DshConfig {
    let stored = config::load(&state.config_path);
    stored.effective(|k| std::env::var(k).ok())
}

#[tauri::command]
fn set_config(state: State<AppState>, config: DshConfig) -> Result<(), String> {
    config::save(&state.config_path, &config)
}

/// 用系统默认应用打开目标（URL / 文件 / 目录）。供本地窗口与桥接脚本共用。
#[tauri::command]
fn open_external(target: String) -> Result<(), String> {
    open_with_system(&target)
}

/// 查询开机自启状态（autostart 插件，macOS LaunchAgent / 其他平台系统自启）。
#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 设置开机自启开关（供 dsh 插件设置面板经桥接调用）。
#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

/// 查询开机自启与自启后窗口模式（OS 启停 + 壳侧持久化的启动模式）。
#[tauri::command]
fn get_desktop_settings(
    state: State<AppState>,
    app: AppHandle,
) -> Result<desktop_settings::DesktopSettings, String> {
    let autostart = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    let startup_mode = desktop_settings::read_startup_mode(&state.desktop_settings_path)
        .unwrap_or(desktop_settings::StartupMode::Normal);
    Ok(desktop_settings::DesktopSettings {
        autostart,
        startup_mode,
    })
}

/// 设置开机自启开关与自启后窗口模式（供 dsh 插件设置面板经桥接调用）。
#[tauri::command]
fn set_desktop_settings(
    state: State<AppState>,
    app: AppHandle,
    settings: desktop_settings::DesktopSettings,
) -> Result<(), String> {
    let enabled = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    if enabled != settings.autostart {
        if settings.autostart {
            app.autolaunch().enable().map_err(|e| e.to_string())?;
        } else {
            app.autolaunch().disable().map_err(|e| e.to_string())?;
        }
    }
    desktop_settings::save_desktop_settings(&state.desktop_settings_path, &settings)
}

/// 保存快捷键注册表到 config.json（保留其余配置字段）。
fn persist_shortcuts(config_path: &Path, shortcuts: &[String]) {
    let mut config = config::load(config_path);
    config.shortcuts = shortcuts.to_vec();
    let _ = config::save(config_path, &config);
}

/// 注册系统级全局快捷键（如 CmdOrCtrl+Shift+D），按下时 emit `dsh-shortcut`。
fn register_shortcut_internal(
    state: &AppState,
    shortcut: String,
) -> Result<ShortcutSnapshot, String> {
    let s = Shortcut::from_str(&shortcut).map_err(|e| e.to_string())?;
    if state.shortcuts.lock().unwrap().contains_key(&shortcut) {
        return Ok(ShortcutSnapshot {
            shortcut,
            registered: true,
        });
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
    Ok(ShortcutSnapshot {
        shortcut,
        registered: true,
    })
}

#[tauri::command]
fn register_shortcut(state: State<AppState>, shortcut: String) -> Result<ShortcutSnapshot, String> {
    register_shortcut_internal(&state, shortcut)
}

/// 注销已注册的全局快捷键。
#[tauri::command]
fn unregister_shortcut(state: State<AppState>, shortcut: String) -> Result<(), String> {
    let app = state.app.clone();
    if let Some(s) = state.shortcuts.lock().unwrap().remove(&shortcut) {
        app.global_shortcut()
            .unregister(s)
            .map_err(|e| e.to_string())?;
    }
    let shortcuts = state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    persist_shortcuts(&state.config_path, &shortcuts);
    Ok(())
}

/// 查询当前已注册的全局快捷键。
#[tauri::command]
fn get_shortcuts(state: State<AppState>) -> Vec<ShortcutSnapshot> {
    state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .map(|shortcut| ShortcutSnapshot {
            shortcut,
            registered: true,
        })
        .collect()
}

/// 注销全部已注册的全局快捷键并清空持久化记录。
#[tauri::command]
fn unregister_all_shortcuts(state: State<AppState>) -> Result<(), String> {
    let shortcuts = state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    if !shortcuts.is_empty() {
        state
            .app
            .global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())?;
    }
    state.shortcuts.lock().unwrap().clear();
    persist_shortcuts(&state.config_path, &[]);
    Ok(())
}

/// 查询 GitHub 最新 release 的 tag（如 v0.1.0）。失败返回 Err（网络/限流/解析）。
async fn latest_release_tag() -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/NoelOrin/dsh-desktop/releases/latest")
        .header("User-Agent", "dsh-desktop")
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("检查更新失败: HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析发布信息失败: {e}"))?;
    json.get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "检查更新失败: 响应缺少 tag_name".to_string())
}

/// 解析 GitHub release tag（如 v0.1.0）为 semver 版本。
fn parse_release_tag(tag: &str) -> Result<semver::Version, String> {
    semver::Version::parse(tag.trim_start_matches('v'))
        .map_err(|e| format!("解析远程版本 {tag} 失败: {e}"))
}

/// 检查 GitHub Release 是否有新版本，返回新版本号（无则 None）。
/// 仅供 dsh 插件"检查更新"经桥接调用。
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    let tag = latest_release_tag().await?;
    let remote = parse_release_tag(&tag)?;
    if remote > app.package_info().version {
        Ok(Some(tag))
    } else {
        Ok(None)
    }
}

/// 静默下载最新版安装包到应用缓存目录，返回本地路径（由用户手动运行安装）。
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<String, String> {
    let tag = latest_release_tag().await?;
    let remote = parse_release_tag(&tag)?;
    if remote <= app.package_info().version {
        return Err("没有可用更新".to_string());
    }

    // 当前平台的安装包扩展名
    let ext = if cfg!(target_os = "macos") {
        ".dmg"
    } else if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ".AppImage"
    };

    // 取最新 release 的资产下载地址
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/NoelOrin/dsh-desktop/releases/latest")
        .header("User-Agent", "dsh-desktop")
        .send()
        .await
        .map_err(|e| format!("获取发布信息失败: {e}"))?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析发布信息失败: {e}"))?;
    let asset = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .map(|n| n.ends_with(ext))
                    .unwrap_or(false)
            })
        })
        .ok_or_else(|| format!("发布中未找到 {ext} 安装包"))?;
    let name = asset["name"]
        .as_str()
        .ok_or_else(|| "资产缺少 name".to_string())?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| "资产缺少下载地址".to_string())?;

    // 下载到 app_cache_dir/updates/<name>
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let updates_dir = cache_dir.join("updates");
    std::fs::create_dir_all(&updates_dir).map_err(|e| format!("创建更新目录失败: {e}"))?;
    let dest = updates_dir.join(name);

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status()));
    }
    let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败: {e}"))?;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下载中断: {e}"))? {
        file.write_all(&chunk)
            .map_err(|e| format!("写入失败: {e}"))?;
    }
    file.sync_all().map_err(|e| format!("同步失败: {e}"))?;

    // Linux AppImage 需要可执行权限
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置执行权限失败: {e}"))?;
    }

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
}

/// 请求系统通知权限，返回 granted / prompt / denied。
#[tauri::command]
fn request_notification_permission(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_notification::PermissionState;
    let state = app
        .notification()
        .request_permission()
        .map_err(|e| e.to_string())?;
    Ok(match state {
        PermissionState::Granted => "granted".to_string(),
        PermissionState::Prompt => "prompt".to_string(),
        PermissionState::PromptWithRationale => "prompt".to_string(),
        PermissionState::Denied => "denied".to_string(),
    })
}

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
    let running = matches!(snapshot.phase, RuntimePhase::Starting | RuntimePhase::Ready);
    let _ = state.stop_dsh.set_enabled(running);
    let _ = state
        .restart_dsh
        .set_enabled(!matches!(snapshot.phase, RuntimePhase::Installing));
    let url_hint = snapshot
        .url
        .as_deref()
        .map(|url| format!(" ({url})"))
        .unwrap_or_default();
    let _ = state
        .tray
        .set_tooltip(Some(format!("DSH Desktop - {label}{url_hint}")));
}

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
    {
        let mut inner = inner.lock().unwrap();
        inner.logs.push_back(line.to_string());
        while inner.logs.len() > MAX_LOGS {
            inner.logs.pop_front();
        }
    }
    let _ = app.emit("dsh-log", line);
}

fn append_stream(
    stream: impl std::io::Read + Send + 'static,
    app: &AppHandle,
    inner: &Arc<Mutex<Inner>>,
    log_path: &Path,
    label: &'static str,
) {
    let reader = BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        append_line(app, inner, log_path, &format!("[{label}] {line}"));
    }
}

fn run_install(
    config: &DshConfig,
    log_path: &Path,
    app: &AppHandle,
    inner: &Arc<Mutex<Inner>>,
) -> Result<(), String> {
    let npm = resolve_npm(config)
        .ok_or_else(|| "未检测到 npm，无法安装 DSH（请先安装 Node.js）".to_string())?;
    append_line(
        app,
        inner,
        log_path,
        "[desktop] 执行: npm install -g @deepseek-ai/dsh",
    );
    let mut cmd = Command::new(&npm);
    cmd.arg("install").arg("-g").arg("@deepseek-ai/dsh");
    let mut child = cmd
        .spawn()
        .map_err(|error| format!("无法启动 npm ({}): {error}", npm.display()))?;

    if let Some(stdout) = child.stdout.take() {
        append_stream(stdout, app, inner, log_path, "npm");
    }
    if let Some(stderr) = child.stderr.take() {
        append_stream(stderr, app, inner, log_path, "npm");
    }

    let status = child
        .wait()
        .map_err(|error| format!("npm 安装中断: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("npm 安装失败 (exit {:?})", status.code()))
    }
}

fn resolve_dsh(config: &DshConfig) -> Option<(PathBuf, PathBuf)> {
    let node = resolve_node(config)?;
    let entry = config
        .dsh_bin
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_in_path("dsh"))
        .or_else(|| npm_global_dsh(config))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/dsh"))
                .filter(|path| path.is_file())
        })?;
    Some((node, entry))
}

fn resolve_node(config: &DshConfig) -> Option<PathBuf> {
    config
        .dsh_node
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_in_path("node"))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/node"))
                .filter(|path| path.is_file())
        })
}

/// 解析可用的 npm：PATH 优先，其次 node 同目录；找不到返回 None（不再兜底为字面量 "npm"）。
fn resolve_npm(config: &DshConfig) -> Option<PathBuf> {
    find_in_path("npm").or_else(|| {
        resolve_node(config)
            .and_then(|node| node.parent().map(|dir| dir.join("npm")))
            .filter(|path| path.is_file())
    })
}

/// 定位 npm 全局安装的 dsh：先找真实入口 lib/bin.js，再回退到 bin 软链。
fn npm_global_dsh(config: &DshConfig) -> Option<PathBuf> {
    let prefix = npm_global_prefix(config)?;
    let modules = if cfg!(windows) {
        prefix.join("node_modules")
    } else {
        prefix.join("lib").join("node_modules")
    };
    let entry = modules.join("@deepseek-ai/dsh/lib/bin.js");
    if entry.is_file() {
        return Some(entry);
    }
    let bin = if cfg!(windows) {
        prefix.join("dsh")
    } else {
        prefix.join("bin").join("dsh")
    };
    bin.is_file().then_some(bin)
}

/// 通过 npm prefix -g 查询 npm 全局安装前缀（GUI 启动时 PATH 可能不含 npm bin）。
fn npm_global_prefix(config: &DshConfig) -> Option<PathBuf> {
    let npm = resolve_npm(config)?;
    let output = Command::new(&npm).args(["prefix", "-g"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8(output.stdout).ok()?;
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }
    Some(PathBuf::from(prefix))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn is_health_ready(url: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], parse_port(url))),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let request = "GET /dsh-desktop/health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n";
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    line.starts_with("HTTP/1.0 200") || line.starts_with("HTTP/1.1 200")
}

fn parse_port(url: &str) -> u16 {
    let authority = url.split('/').nth(2).unwrap_or(url);
    authority
        .rsplit(':')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3080)
}

fn exit_summary(status: &std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "unknown".into())
}

fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(target).status();
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(target).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(target)
        .status();
    status
        .map(|_| ())
        .map_err(|error| format!("无法打开 {target}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dsh_serializes_as_missing() {
        // 前端契约（RuntimePhase）使用 "missing"，后端序列化必须一致
        assert_eq!(
            serde_json::to_string(&RuntimePhase::MissingDsh).unwrap(),
            "\"missing\""
        );
    }

    #[test]
    fn other_phases_serialize_in_snake_case() {
        for (phase, expected) in [
            (RuntimePhase::Detecting, "detecting"),
            (RuntimePhase::Installing, "installing"),
            (RuntimePhase::Starting, "starting"),
            (RuntimePhase::Ready, "ready"),
            (RuntimePhase::Failed, "failed"),
            (RuntimePhase::Stopped, "stopped"),
        ] {
            assert_eq!(
                serde_json::to_string(&phase).unwrap(),
                format!("\"{expected}\"")
            );
        }
    }

    #[test]
    fn tray_labels_cover_all_phases() {
        assert_eq!(tray_phase_label(&RuntimePhase::Detecting), "检测中");
        assert_eq!(tray_phase_label(&RuntimePhase::Ready), "已就绪");
        assert_eq!(tray_phase_label(&RuntimePhase::Stopped), "已停止");
    }

    #[test]
    fn parse_port_handles_health_url_path() {
        assert_eq!(parse_port("http://127.0.0.1:62359"), 62359);
        assert_eq!(
            parse_port("http://127.0.0.1:62359/dsh-desktop/health"),
            62359
        );
        assert_eq!(parse_port("http://[::1]:62359/dsh-desktop/health"), 62359);
    }
}
