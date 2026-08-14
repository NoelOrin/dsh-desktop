use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod config;
use config::DshConfig;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, WindowEvent};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, Code, Modifiers, ShortcutState};
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
const TRAY_OPEN_CONTROL: &str = "tray-open-control";
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
    getStatus: function () { return invoke("get_status"); },
    restart: function () { return invoke("restart"); },
    installDsh: function () { return invoke("install_dsh"); },
    openLogDirectory: function () { return invoke("open_log_directory"); },
    onStatus: function (cb) { return listen("dsh-status", cb); },
    onLog: function (cb) { return listen("dsh-log", cb); },
    onFileDrop: function (cb) { return listen("dsh-file-drop", cb); },
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
    // 全局快捷键：macOS 用 Cmd+Shift+C，其他平台用 Ctrl+Shift+C
    let shortcut = {
        #[cfg(target_os = "macos")]
        {
            tauri_plugin_global_shortcut::Shortcut::new(
                Some(Modifiers::META | Modifiers::SHIFT),
                Code::KeyC,
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            tauri_plugin_global_shortcut::Shortcut::new(
                Some(Modifiers::CONTROL | Modifiers::SHIFT),
                Code::KeyC,
            )
        }
    };

    let shortcut_plugin = ShortcutBuilder::new()
        .with_shortcuts([shortcut])
        .expect("invalid global shortcut")
        .with_handler(move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                open_control_window(app);
            }
        })
        .build();

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
        // 窗口状态记忆：重启后恢复 main/control 窗口大小/位置/最大化；不保存可见性，
        // 避免 control 窗口（visible:false）在重启后被插件恢复为可见
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
        // 自动更新：pubkey/endpoints 由 tauri.conf.json 的 plugins.updater 段提供
        .plugin(tauri_plugin_updater::Builder::new().build())
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

            app.manage(AppState {
                app: app_handle,
                inner: inner.clone(),
                tx: tx.clone(),
                log_dir,
                config_path,
                exiting: exiting.clone(),
            });

            let start_tx = tx.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                let _ = start_tx.send(ManagerMessage::Start);
            });

            let menu = build_menu(app.handle())?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                if event.id().as_ref() == "open-control" {
                    open_control_window(app);
                }
            });

            setup_tray(app.handle(), exiting.clone())?;

            #[cfg(debug_assertions)]
            if let Some(control) = app.get_webview_window("control") {
                if let Ok(url) = tauri::Url::parse("http://localhost:5174/control-center/") {
                    let _ = control.navigate(url);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            restart,
            install_dsh,
            open_log_directory,
            get_config,
            set_config,
            open_external
        ])
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            // 文件拖放：把真实路径通过 dsh-file-drop 事件转给前端（dsh web 经桥接订阅）
            if label == "main" {
                if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                    let paths: Vec<String> = paths
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    let _ = window.emit("dsh-file-drop", paths);
                    return;
                }
                // 系统主题变化：透传给前端（启动页 / 控制中心监听 dsh-theme）
                if let WindowEvent::ThemeChanged(theme) = event {
                    let theme = match theme {
                        tauri::Theme::Dark => "dark",
                        _ => "light",
                    };
                    let _ = window.emit("dsh-theme", theme);
                    return;
                }
            }
            if label == "control" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
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

fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    let item = MenuItem::with_id(
        app,
        "open-control",
        "控制中心",
        true,
        Some("CmdOrCtrl+Shift+C"),
    )?;
    let submenu = Submenu::with_items(app, "DSH", true, &[&item])?;
    menu.append(&submenu)?;
    Ok(menu)
}

fn open_control_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_focus();
    }
}

/// 创建系统托盘：常驻后台，菜单含 显示主窗口 / 控制中心 / 重启 dsh / 退出。
fn setup_tray(app: &tauri::AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<()> {
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let open_control = MenuItem::with_id(app, TRAY_OPEN_CONTROL, "控制中心", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, TRAY_RESTART, "重启 dsh", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT, "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&show_main, &open_control, &restart, &separator, &quit],
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
            TRAY_SHOW_MAIN => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            TRAY_OPEN_CONTROL => open_control_window(app),
            TRAY_RESTART => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Start);
            }
            TRAY_QUIT => {
                exiting.store(true, Ordering::Relaxed);
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Stop);
                thread::sleep(Duration::from_millis(300));
                app.exit(0);
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

        let mut cmd = Command::new(&node);
        cmd.arg(&entry)
            .args(["web", "--host", "127.0.0.1", "--port", &port.to_string()])
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
        self.append_log(&format!(
            "[desktop] 启动 dsh: {} {} web --host 127.0.0.1 --port {port}",
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
