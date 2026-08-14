use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod config;
mod embedded;
mod theme;

use config::DshConfig;
use theme::{read_ui_theme_section, resolve_ui_theme, UiThemeSnapshot};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, GlobalShortcutExt, Shortcut, ShortcutState,
};
use tauri_plugin_notification::NotificationExt;

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_LOGS: usize = 500;
/// 子进程意外退出后的自动重启上限（看门狗，超过则转 failed）。
const MAX_AUTO_RESTARTS: u32 = 3;
/// 优雅退出：SIGTERM 后等待子进程退出的宽限期。
const GRACE_PERIOD: Duration = Duration::from_secs(2);

// 托盘菜单项 id
const TRAY_SHOW_MAIN: &str = "tray-show-main";
const TRAY_RESTART: &str = "tray-restart";
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
    return internals.listen(event, function (e) { cb(e.payload); });
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
    openLogDirectory: function () { return invoke("open_log_directory"); },
    getConfig: function () { return invoke("get_config"); },
    setConfig: function (config) { return invoke("set_config", { config: config }); },
    onStatus: function (cb) { return listen("dsh-status", cb); },
    onLog: function (cb) { return listen("dsh-log", cb); },
    onFileDrop: function (cb) { return listen("dsh-file-drop", cb); },
    onDeepLink: function (cb) { return listen("dsh-deeplink", cb); },
    autostart: {
      get: function () { return invoke("get_autostart"); },
      set: function (enabled) { return invoke("set_autostart", { enabled: enabled }); },
    },
    shortcuts: {
      register: function (s, cb) {
        return invoke("register_shortcut", { shortcut: s }).then(function () {
          return listen("dsh-shortcut", function (e) { if (e.payload === s) cb(); });
        });
      },
      unregister: function (s) { return invoke("unregister_shortcut", { shortcut: s }); },
    },
    onShortcut: function (cb) { return listen("dsh-shortcut", cb); },
    update: {
      check: function () { return invoke("check_update"); },
      install: function () { return invoke("install_update"); },
    },
  };
})();"#;

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
    log_dir: Option<String>,
    logs: Vec<String>,
}

#[derive(Default)]
struct Inner {
    phase: RuntimePhase,
    message: String,
    url: Option<String>,
    dsh_installed: bool,
    node_found: bool,
    log_dir: Option<PathBuf>,
    logs: VecDeque<String>,
    /// 尚未被 dsh web 消费的深链（dsh-desktop://）原始 URL，就绪后补发。
    pending_deeplinks: VecDeque<String>,
}

impl Inner {
    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            phase: self.phase.clone(),
            message: self.message.clone(),
            url: self.url.clone(),
            dsh_installed: self.dsh_installed,
            node_found: self.node_found,
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
    /// 应用是否正在退出（托盘"退出"置 true，用于关闭到托盘时区分真正退出）。
    exiting: Arc<AtomicBool>,
    /// 已注册的自定义全局快捷键注册表（快捷键字符串 → Shortcut），供注销时查表。
    shortcuts: Arc<Mutex<HashMap<String, Shortcut>>>,
}

enum ManagerMessage {
    Start,
    Stop,
    Shutdown,
    Ready { generation: u64, url: String },
    ReadyTimeout { generation: u64, port: u16 },
    InstallFinished { result: Result<(), String> },
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
}

pub fn run() {
    // 全局快捷键插件：预置快捷键（CmdOrCtrl+Shift+C 打开控制中心）已随控制中心移除；
    // 自定义快捷键由 dsh 插件经 register_shortcut / unregister_shortcut 桥接命令注册。
    let shortcut_plugin = ShortcutBuilder::new().build();

    let exiting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        // 单实例锁必须最先注册（插件按注册顺序执行）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动时聚焦已存在的主窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
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
            }
        })
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let app_data = app_handle.path().app_data_dir()?;
            let log_dir = app_data.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            let log_path = log_dir.join("dsh.log");
            let config_path = app_data.join("config.json");

            let inner = Arc::new(Mutex::new(Inner {
                log_dir: Some(log_dir.clone()),
                ..Inner::default()
            }));

            let (tx, rx) = mpsc::channel();
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
                exiting: exiting.clone(),
                shortcuts: Arc::new(Mutex::new(HashMap::new())),
            });

            // 深链 dsh-desktop://：收到 URL 后暂存（未就绪时由 Ready 分支补发）并立即转发给 dsh web
            let deep_app = app.handle().clone();
            let deep_link_app = deep_app.clone();
            deep_link_app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let url = url.to_string();
                    let state = deep_app.state::<AppState>();
                    {
                        let mut inner = state.inner.lock().unwrap();
                        inner.pending_deeplinks.push_back(url.clone());
                    }
                    let _ = deep_app.emit("dsh-deeplink", url);
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

            setup_tray(app.handle(), exiting.clone())?;

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
            restart,
            install_dsh,
            open_log_directory,
            get_config,
            set_config,
            open_external,
            get_autostart,
            set_autostart,
            register_shortcut,
            unregister_shortcut,
            check_update,
            install_update,
            get_ui_theme,
            window_action
        ])
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            // 文件拖放：把真实路径通过 dsh-file-drop 事件转给前端（dsh web 经桥接订阅）
            if label == "main" {
                if let WindowEvent::Resized(_) = event {
                    emit_window_state(window.app_handle());
                    return;
                }
                if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                    let paths: Vec<String> = paths
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    let _ = window.emit("dsh-file-drop", paths);
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
            if let RunEvent::Exit = event {
                let _ = app.state::<AppState>().tx.send(ManagerMessage::Shutdown);
            }
        });
}

/// 创建系统托盘：常驻后台，菜单含 显示主窗口 / 重启 dsh / 退出。
fn setup_tray(app: &tauri::AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<()> {
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, TRAY_RESTART, "重启 dsh", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT, "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show_main, &restart, &separator, &quit])?;

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
            TRAY_SHOW_MAIN => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            TRAY_RESTART => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Start);
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

    // 托盘句柄需要保活，否则图标会被销毁
    app.manage(ManagedTray(tray));
    Ok(())
}

/// 需要持有 TrayIcon 使其保活（Tauri State 要求 Send + Sync）。
struct ManagedTray(#[allow(dead_code)] tauri::tray::TrayIcon<tauri::Wry>);

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
                        // 就绪后补发就绪前收到的深链（页面 ready 后由桥接 onDeepLink 消费）
                        {
                            let mut inner = self.inner.lock().unwrap();
                            while let Some(link) = inner.pending_deeplinks.pop_front() {
                                let _ = self.app.emit("dsh-deeplink", link);
                            }
                        }
                        self.notify("DSH 已就绪", &format!("DeepSeek Harness 已启动：{url}"));
                        self.open_window(url);
                    }
                }
                Ok(ManagerMessage::ReadyTimeout { generation, port }) => {
                    if generation == self.generation {
                        self.fail(format!("DSH 在端口 {port} 上等待超时"));
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
        self.set_phase(RuntimePhase::Starting, "正在启动 DSH...".to_string(), None);
        if let Err(error) = self.start(node, entry, config.dsh_home) {
            self.fail(error);
        }
    }

    fn start(&mut self, node: PathBuf, entry: PathBuf, home: Option<String>) -> Result<(), String> {
        self.cleanup_child();

        let port = reserve_port()?;
        let url = format!("http://127.0.0.1:{port}");
        let workspace = std::env::var("HOME").unwrap_or_else(|_| ".".into());

        self.generation += 1;
        let generation = self.generation;

        // 装配内嵌插件（best-effort）并生成 --patch overlay：任何失败只记日志，不影响 dsh 启动
        let overlay = {
            let resource_dir = self.app.path().resource_dir().ok();
            let home_path = home
                .as_ref()
                .map(|h| PathBuf::from(h.as_str()))
                .or_else(|| dirs::home_dir().map(|h| h.join(".dsh")));
            let mut log = |line: &str| self.append_log(line);
            let mounted = match (resource_dir, home_path) {
                (Some(res), Some(home)) => embedded::assemble(&res, &home, &mut log),
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
            port.to_string(),
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

        let mut child = cmd
            .spawn()
            .map_err(|error| format!("无法启动 dsh: {error}"))?;
        let overlay_log = overlay
            .as_ref()
            .map(|path| format!(" --patch {}", path.to_string_lossy()))
            .unwrap_or_default();
        self.append_log(&format!(
            "[desktop] 启动 dsh: {} {} web{overlay_log} --host 127.0.0.1 --port {port}",
            node.display(),
            entry.display()
        ));

        let stdout = child.stdout.take().expect("stdout 已开启管道");
        let stderr = child.stderr.take().expect("stderr 已开启管道");
        self.spawn_reader(stdout, "stdout");
        self.spawn_reader(stderr, "stderr");
        self.child = Some(child);

        let tx = self.tx.clone();
        thread::spawn(move || {
            let started = Instant::now();
            loop {
                if is_server_ready(port) {
                    let _ = tx.send(ManagerMessage::Ready { generation, url });
                    return;
                }
                if started.elapsed() >= READY_TIMEOUT {
                    let _ = tx.send(ManagerMessage::ReadyTimeout { generation, port });
                    return;
                }
                thread::sleep(POLL_INTERVAL);
            }
        });

        Ok(())
    }

    fn open_window(&self, url: String) {
        if let Some(window) = self.app.get_webview_window("main") {
            if let Ok(url) = tauri::Url::parse(&url) {
                let _ = window.navigate(url);
            }
            let _ = window.show();
            let _ = window.set_focus();
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
        let _ = self
            .app
            .notification()
            .builder()
            .title(title.to_string())
            .body(body.to_string())
            .show();
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> RuntimeSnapshot {
    state.inner.lock().unwrap().snapshot()
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

/// 注册系统级全局快捷键（如 CmdOrCtrl+Shift+D），按下时 emit `dsh-shortcut`。
#[tauri::command]
fn register_shortcut(state: State<AppState>, shortcut: String) -> Result<(), String> {
    let s = Shortcut::from_str(&shortcut).map_err(|e| e.to_string())?;
    let app = state.app.clone();
    let trigger = shortcut.clone();
    app.global_shortcut()
        .on_shortcut(s, move |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app.emit("dsh-shortcut", trigger.clone());
            }
        })
        .map_err(|e| e.to_string())?;
    state.shortcuts.lock().unwrap().insert(shortcut, s);
    Ok(())
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

    Ok(dest.to_string_lossy().into_owned())
}

fn emit_status(app: &AppHandle, inner: &Arc<Mutex<Inner>>) {
    let snapshot = inner.lock().unwrap().snapshot();
    let _ = app.emit("dsh-status", snapshot);
}

fn append_line(app: &AppHandle, inner: &Arc<Mutex<Inner>>, log_path: &Path, line: &str) {
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

fn reserve_port() -> Result<u16, String> {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    drop(listener);
    Ok(port)
}

fn is_server_ready(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let request = format!("GET / HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n");
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
}
