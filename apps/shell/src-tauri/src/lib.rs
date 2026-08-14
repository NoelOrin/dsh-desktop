use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod config;
use config::DshConfig;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, WindowEvent};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, Code, Modifiers, ShortcutState};

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_LOGS: usize = 500;

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
    install_dir: Option<String>,
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
    install_dir: Option<PathBuf>,
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
            install_dir: self
                .install_dir
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
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
    runtime_dir: PathBuf,
    log_dir: PathBuf,
    config_path: PathBuf,
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
    runtime_dir: PathBuf,
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

    tauri::Builder::default()
        .plugin(shortcut_plugin)
        .setup(|app| {
            let app_handle = app.handle().clone();
            let app_data = app_handle.path().app_data_dir()?;
            let runtime_dir = app_data.join("runtime");
            let log_dir = app_data.join("logs");
            std::fs::create_dir_all(&runtime_dir)?;
            std::fs::create_dir_all(&log_dir)?;
            let log_path = log_dir.join("dsh.log");
            let config_path = app_data.join("config.json");

            let inner = Arc::new(Mutex::new(Inner {
                install_dir: Some(runtime_dir.clone()),
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
                runtime_dir: runtime_dir.clone(),
                log_path,
                config_path: config_path.clone(),
            };
            thread::spawn(move || manager.run());

            app.manage(AppState {
                app: app_handle,
                inner: inner.clone(),
                tx: tx.clone(),
                runtime_dir,
                log_dir,
                config_path,
            });

            let start_tx = tx.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                let _ = start_tx.send(ManagerMessage::Start);
            });

            // 主窗口改为在 setup 中手动构建：macOS 上启用透明窗口 +
            // Overlay 标题栏 + vibrancy 毛玻璃，并在每次页面加载完成后注入
            // 样式，让 Harness Web UI 顶部操作 bar 区域透出毛玻璃；其他平台
            // 保持普通不透明窗口。
            let builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("DSH Desktop")
                    .inner_size(1280.0, 860.0)
                    .min_inner_size(480.0, 600.0)
                    .center()
                    .resizable(true)
                    .visible(true);

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

                let injected = include_str!("../resources/titlebar-transparency.js");
                let window = builder
                    .transparent(true)
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true)
                    .on_page_load(move |window, payload| {
                        if payload.event() == tauri::webview::PageLoadEvent::Finished {
                            let _ = window.eval(injected);
                        }
                    })
                    .build()?;

                apply_vibrancy(&window, NSVisualEffectMaterial::HudWindow, None, Some(20.0))
                    .expect("无法应用 macOS 毛玻璃效果 (apply_vibrancy)");
            }

            #[cfg(not(target_os = "macos"))]
            {
                let _ = builder.build()?;
            }

            let menu = build_menu(app.handle())?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                if event.id().as_ref() == "open-control" {
                    open_control_window(app);
                }
            });

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
            set_config
        ])
        .on_window_event(|window, event| {
            if window.label() == "control" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let app = window.app_handle().clone();
                thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = state.tx.send(ManagerMessage::Stop);
                    thread::sleep(Duration::from_millis(300));
                    app.exit(0);
                });
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

impl DshManager {
    fn run(mut self) {
        loop {
            match self.rx.recv_timeout(POLL_INTERVAL) {
                Ok(ManagerMessage::Start) => self.handle_start(),
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
                        self.handle_start();
                    }
                    Err(error) => self.fail(format!("DSH 安装失败: {error}")),
                },
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(exit) = self.take_exit() {
                        self.fail(format!("DSH 进程已退出 ({exit})"));
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
        let (node, entry) = match resolve_dsh(&config, &self.runtime_dir) {
            Some(pair) => pair,
            None => {
                let node_found = resolve_node(&config).is_some();
                let message = if node_found {
                    "未检测到 DSH"
                } else {
                    "未检测到 Node.js"
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
        self.set_phase(RuntimePhase::Failed, message, None);
    }

    fn cleanup_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            self.generation += 1;
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
    let install_dir = state.runtime_dir.clone();
    let log_path = state.log_dir.join("install.log");
    let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());
    thread::spawn(move || {
        let result = run_install(&config, &install_dir, &log_path, &app, &inner);
        let _ = tx.send(ManagerMessage::InstallFinished { result });
    });
    Ok(())
}

#[tauri::command]
fn open_log_directory(state: State<AppState>) -> Result<(), String> {
    open_external(&state.log_dir.to_string_lossy())
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
    install_dir: &Path,
    log_path: &Path,
    app: &AppHandle,
    inner: &Arc<Mutex<Inner>>,
) -> Result<(), String> {
    if let Some(parent) = install_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let npm = resolve_npm(config);
    let mut cmd = Command::new(&npm);
    cmd.arg("install")
        .arg("--prefix")
        .arg(install_dir)
        .arg("@deepseek-ai/dsh");
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

fn resolve_dsh(config: &DshConfig, runtime_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let node = resolve_node(config)?;
    let entry = config
        .dsh_bin
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let candidate = runtime_dir.join("bin/dsh");
            candidate.is_file().then_some(candidate)
        })
        .or_else(|| {
            let candidate = runtime_dir.join("node_modules/@deepseek-ai/dsh/lib/bin.js");
            candidate.is_file().then_some(candidate)
        })
        .or_else(|| find_in_path("dsh"))
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

fn resolve_npm(config: &DshConfig) -> PathBuf {
    find_in_path("npm")
        .or_else(|| {
            resolve_node(config)
                .and_then(|node| node.parent().map(|dir| dir.join("npm")))
                .filter(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("npm"))
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

fn open_external(target: &str) -> Result<(), String> {
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
