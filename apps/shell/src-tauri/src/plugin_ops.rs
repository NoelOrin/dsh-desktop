use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const BUSY_MESSAGE: &str = "当前只有一个插件操作可运行";
const CANCELLED_MESSAGE: &str = "插件操作已取消";
const MAX_ARG_LENGTH: usize = 512;
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

/// 受管 dsh 插件操作结果，与 packages/contracts 的 PluginOperationResult 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginOperationResult {
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub output: Vec<String>,
}

impl PluginOperationResult {
    fn from_status(status: ExitStatus, output: Vec<String>) -> Self {
        Self {
            ok: status.success(),
            exit_code: status.code(),
            output,
        }
    }
}

#[derive(Debug)]
struct RunningOp {
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
}

struct PluginOpsState {
    active: Mutex<Option<Arc<RunningOp>>>,
    changed: Condvar,
    cancelling: AtomicBool,
    #[cfg(test)]
    cancel_wait_started: AtomicBool,
}

/// 当前唯一的 dsh plugin 操作运行器。
#[derive(Clone)]
pub struct PluginOps {
    logger: Arc<dyn Fn(&str) + Send + Sync>,
    state: Arc<PluginOpsState>,
}

impl Default for PluginOps {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginOps {
    pub fn new() -> Self {
        Self {
            logger: Arc::new(|_| {}),
            state: Arc::new(PluginOpsState {
                active: Mutex::new(None),
                changed: Condvar::new(),
                cancelling: AtomicBool::new(false),
                #[cfg(test)]
                cancel_wait_started: AtomicBool::new(false),
            }),
        }
    }

    pub fn with_logger(self, logger: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            logger: Arc::new(logger),
            state: self.state,
        }
    }

    pub fn install_profile_plugin(
        &self,
        node: &Path,
        dsh: &Path,
        home: &Path,
        profile: &str,
        spec: &str,
    ) -> Result<PluginOperationResult, String> {
        validate_value("spec", spec)?;
        validate_profile(profile)?;
        let cwd = profile_dir(home, profile)?;
        self.run_operation(
            node,
            dsh,
            home,
            cwd,
            &["plugin", "--profile", profile, "add", spec],
        )
    }

    pub fn remove_profile_plugin(
        &self,
        node: &Path,
        dsh: &Path,
        home: &Path,
        profile: &str,
        name: &str,
    ) -> Result<PluginOperationResult, String> {
        validate_value("name", name)?;
        if name == "." || name == ".." {
            return Err("name 无效".to_string());
        }
        validate_profile(profile)?;
        let cwd = profile_dir(home, profile)?;
        self.run_operation(
            node,
            dsh,
            home,
            cwd,
            &["plugin", "--profile", profile, "remove", name],
        )
    }

    pub fn update_profile_plugins(
        &self,
        node: &Path,
        dsh: &Path,
        home: &Path,
        profile: &str,
    ) -> Result<PluginOperationResult, String> {
        validate_profile(profile)?;
        let cwd = profile_dir(home, profile)?;
        self.run_operation(
            node,
            dsh,
            home,
            cwd,
            &["plugin", "--profile", profile, "update"],
        )
    }

    /// 当前是否有运行中的插件操作。
    pub fn is_busy(&self) -> bool {
        self.state.active.lock().unwrap().is_some()
    }

    /// 取消当前操作；active 槽位保留到 run_operation 完成输出收集后再清理。
    pub fn cancel_current(&self) -> Result<(), String> {
        self.state.cancelling.store(true, Ordering::SeqCst);
        let running = {
            let active = self.state.active.lock().unwrap();
            active.as_ref().cloned()
        };
        let Some(running) = running else {
            self.state.cancelling.store(false, Ordering::SeqCst);
            return Ok(());
        };
        running.cancelled.store(true, Ordering::SeqCst);
        let child = running.child.lock().unwrap().take();
        if let Some(mut child) = child {
            terminate_child(&mut child);
        }
        self.wait_for_cleanup(&running);
        self.state.cancelling.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn clear_active(&self, running: &Arc<RunningOp>) {
        let mut active = self.state.active.lock().unwrap();
        let is_current = active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, running));
        if is_current {
            active.take();
            drop(active);
            self.state.changed.notify_all();
        }
    }

    fn wait_for_cleanup(&self, running: &Arc<RunningOp>) {
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        let mut active = self.state.active.lock().unwrap();
        while active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, running))
        {
            #[cfg(test)]
            self.state.cancel_wait_started.store(true, Ordering::SeqCst);
            let now = Instant::now();
            if now >= deadline {
                drop(active);
                self.clear_active(running);
                return;
            }
            let (guard, _) = self
                .state
                .changed
                .wait_timeout(active, deadline - now)
                .unwrap();
            active = guard;
        }
    }

    fn run_operation(
        &self,
        node: &Path,
        dsh: &Path,
        home: &Path,
        cwd: PathBuf,
        args: &[&str],
    ) -> Result<PluginOperationResult, String> {
        let mut command = Command::new(node);
        command
            .arg(dsh)
            .args(args)
            .current_dir(&cwd)
            .env("PATH", crate::effective_path())
            .env("DSH_HOME", home)
            .env("DSH_DESKTOP_MANAGED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let (child_state, cancelled, running) = {
            let mut active = self.state.active.lock().unwrap();
            if self.state.cancelling.load(Ordering::SeqCst) {
                return Err(CANCELLED_MESSAGE.to_string());
            }
            if active.is_some() {
                return Err(BUSY_MESSAGE.to_string());
            }
            let running = Arc::new(RunningOp {
                child: Arc::new(Mutex::new(None)),
                cancelled: Arc::new(AtomicBool::new(false)),
            });
            let child_state = running.child.clone();
            let cancelled = running.cancelled.clone();
            active.replace(running.clone());
            (child_state, cancelled, running)
        };

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.clear_active(&running);
                return Err(format!("无法启动插件操作: {error}"));
            }
        };

        let stdout = child.stdout.take().expect("stdout 已开启管道");
        let stderr = child.stderr.take().expect("stderr 已开启管道");
        {
            let mut state = child_state.lock().unwrap();
            if cancelled.load(Ordering::SeqCst) {
                drop(state);
                terminate_child(&mut child);
                self.clear_active(&running);
                return Err(CANCELLED_MESSAGE.to_string());
            }
            *state = Some(child);
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let logger = self.logger.clone();
        let stdout_handle = thread::spawn({
            let output = output.clone();
            let logger = logger.clone();
            move || collect_stream(stdout, output, logger)
        });
        let stderr_handle = thread::spawn({
            let output = output.clone();
            let logger = logger.clone();
            move || collect_stream(stderr, output, logger)
        });

        let mut wait_error = None;
        let status = loop {
            let mut state = child_state.lock().unwrap();
            let Some(child) = state.as_mut() else {
                break None;
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    state.take();
                    break Some(status);
                }
                Ok(None) => {}
                Err(error) => {
                    let mut child = state.take().expect("child 仍存在");
                    drop(state);
                    terminate_child(&mut child);
                    wait_error = Some(format!("等待插件操作失败: {error}"));
                    break None;
                }
            }
            drop(state);
            thread::sleep(POLL_INTERVAL);
        };

        join_with_timeout(stdout_handle);
        join_with_timeout(stderr_handle);
        self.clear_active(&running);

        if let Some(error) = wait_error {
            return Err(error);
        }

        let output = output.lock().unwrap().clone();

        match status {
            Some(status) => Ok(PluginOperationResult::from_status(status, output)),
            None => Err(CANCELLED_MESSAGE.to_string()),
        }
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn collect_stream(
    stream: impl std::io::Read + Send + 'static,
    output: Arc<Mutex<Vec<String>>>,
    logger: Arc<dyn Fn(&str) + Send + Sync>,
) {
    let reader = BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim_end().to_string();
        if !line.is_empty() {
            {
                let mut lines = output.lock().unwrap();
                lines.push(line.clone());
            }
            logger(&line);
        }
    }
}

fn join_with_timeout(handle: thread::JoinHandle<()>) {
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
    let _ = handle.join();
}

fn validate_value(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} 不能为空"));
    }
    if value.starts_with('-') {
        return Err(format!("{label} 不能以 - 开头"));
    }
    if value.contains('\0') {
        return Err(format!("{label} 不能包含 NUL"));
    }
    if value.chars().count() > MAX_ARG_LENGTH {
        return Err(format!("{label} 长度不能超过 {MAX_ARG_LENGTH}"));
    }
    Ok(())
}

fn validate_profile(profile: &str) -> Result<(), String> {
    validate_value("profile", profile)?;
    if profile == "."
        || profile == ".."
        || profile == "node_modules"
        || profile.contains('/')
        || profile.contains('\\')
    {
        return Err("profile 名称无效".to_string());
    }
    Ok(())
}

fn profile_dir(home: &Path, profile: &str) -> Result<PathBuf, String> {
    validate_profile(profile)?;
    Ok(home.join("profiles").join(profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    const LONG_RUNNING_SCRIPT: &str =
        r#"process.stdout.write("started\n"); setTimeout(() => process.exit(0), 60_000);"#;

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "dsh-plugin-ops-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_script(root: &Path, name: &str, body: &str) -> PathBuf {
        fs::create_dir_all(root).unwrap();
        let path = root.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    fn setup_profile(root: &Path, profile: &str) -> PathBuf {
        let home = root.join("home");
        fs::create_dir_all(home.join("profiles").join(profile)).unwrap();
        home
    }

    fn fake_dsh_install_script(result_path: &Path) -> String {
        let literal = serde_json::to_string(&result_path.to_string_lossy()).unwrap();
        format!(
            r#"
const fs = require("fs");
const payload = {{
  argv: process.argv.slice(2),
  cwd: process.cwd(),
  env: {{
    PATH: process.env.PATH || "",
    DSH_HOME: process.env.DSH_HOME || "",
    DSH_DESKTOP_MANAGED: process.env.DSH_DESKTOP_MANAGED || "",
  }},
}};
fs.writeFileSync({literal}, JSON.stringify(payload));
process.stdout.write("stdout from fake dsh\n");
process.stderr.write("stderr from fake dsh\n");
"#
        )
    }

    fn node_path() -> PathBuf {
        std::env::var_os("NODE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("node"))
    }

    fn wait_until_busy(ops: &PluginOps) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ops.is_busy() {
            assert!(Instant::now() < deadline, "插件操作未进入忙碌状态");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn install_profile_plugin_passes_argv_cwd_and_env() {
        let root = temp_root("install-argv");
        let home = setup_profile(&root, "web");
        let result_path = root.join("result.json");
        let fake_dsh = write_script(&root, "fake-dsh.js", &fake_dsh_install_script(&result_path));
        let ops = PluginOps::new();

        let result = ops
            .install_profile_plugin(&node_path(), &fake_dsh, &home, "web", "@scope/pkg@1.2.3")
            .unwrap();

        assert!(result.ok);
        assert_eq!(result.exit_code, Some(0));
        assert!(result
            .output
            .iter()
            .any(|line| line == "stdout from fake dsh"));
        assert!(result
            .output
            .iter()
            .any(|line| line == "stderr from fake dsh"));

        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&result_path).unwrap()).unwrap();
        assert_eq!(
            payload["argv"],
            serde_json::json!(["plugin", "--profile", "web", "add", "@scope/pkg@1.2.3"])
        );
        assert_eq!(
            Path::new(payload["cwd"].as_str().unwrap()),
            fs::canonicalize(home.join("profiles").join("web")).unwrap()
        );
        assert_eq!(
            payload["env"]["DSH_HOME"],
            serde_json::json!(home.to_string_lossy())
        );
        assert_eq!(payload["env"]["DSH_DESKTOP_MANAGED"], "1");
        assert!(!payload["env"]["PATH"]
            .as_str()
            .unwrap_or_default()
            .is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn update_profile_plugins_reports_nonzero_exit_code() {
        let root = temp_root("exit-code");
        let home = setup_profile(&root, "web");
        let fake_dsh = write_script(
            &root,
            "exit.js",
            r#"process.stderr.write("boom\n"); process.exit(7);"#,
        );
        let ops = PluginOps::new();

        let result = ops
            .update_profile_plugins(&node_path(), &fake_dsh, &home, "web")
            .unwrap();

        assert!(!result.ok);
        assert_eq!(result.exit_code, Some(7));
        assert!(result.output.iter().any(|line| line == "boom"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn second_operation_while_running_returns_busy() {
        let root = temp_root("busy");
        let home = setup_profile(&root, "web");
        let fake_dsh = write_script(&root, "slow.js", LONG_RUNNING_SCRIPT);
        let ops = PluginOps::new();
        let run_ops = ops.clone();
        let run_fake_dsh = fake_dsh.clone();
        let run_home = home.clone();
        let handle = thread::spawn(move || {
            run_ops.install_profile_plugin(&node_path(), &run_fake_dsh, &run_home, "web", "pkg")
        });

        wait_until_busy(&ops);
        let error = ops
            .remove_profile_plugin(&node_path(), &fake_dsh, &home, "web", "other")
            .unwrap_err();
        assert_eq!(error, "当前只有一个插件操作可运行");

        ops.cancel_current().unwrap();
        let error = handle.join().unwrap().unwrap_err();
        assert!(error.contains("已取消"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cancel_current_stops_running_operation_and_run_returns_cancelled() {
        let root = temp_root("cancel");
        let home = setup_profile(&root, "web");
        let fake_dsh = write_script(&root, "slow.js", LONG_RUNNING_SCRIPT);
        let ops = PluginOps::new();
        let run_ops = ops.clone();
        let run_fake_dsh = fake_dsh.clone();
        let run_home = home.clone();
        let handle = thread::spawn(move || {
            run_ops.install_profile_plugin(&node_path(), &run_fake_dsh, &run_home, "web", "pkg")
        });

        wait_until_busy(&ops);
        ops.cancel_current().unwrap();

        let error = handle.join().unwrap().unwrap_err();
        assert!(error.contains("已取消"));
        assert!(!ops.is_busy());
        assert!(ops.cancel_current().is_ok());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cancel_current_waits_for_cleanup_and_new_operation_starts() {
        let root = temp_root("cancel-active");
        let home = setup_profile(&root, "web");
        let fake_dsh = write_script(&root, "slow.js", LONG_RUNNING_SCRIPT);
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let blocked = Arc::new(Mutex::new(Some(release_rx)));
        let logger_blocked = blocked.clone();
        let ops = PluginOps::new().with_logger(move |_line| {
            let release = logger_blocked
                .lock()
                .unwrap()
                .take()
                .expect("logger 只应阻塞一次");
            let _ = release.recv();
        });
        let run_ops = ops.clone();
        let run_fake_dsh = fake_dsh.clone();
        let run_home = home.clone();
        let handle = thread::spawn(move || {
            run_ops.install_profile_plugin(&node_path(), &run_fake_dsh, &run_home, "web", "pkg")
        });

        wait_until_busy(&ops);
        let deadline = Instant::now() + Duration::from_secs(5);
        while blocked.lock().unwrap().is_some() {
            assert!(Instant::now() < deadline, "logger 未开始阻塞");
            thread::sleep(Duration::from_millis(2));
        }

        let cancel_ops = ops.clone();
        let cancel_handle = thread::spawn(move || cancel_ops.cancel_current());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ops
            .state
            .cancel_wait_started
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            assert!(
                Instant::now() < deadline,
                "cancel_current 未进入等待清理状态"
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert!(ops.is_busy(), "取消后旧操作清理完成前应保持忙碌");

        let rejected = ops
            .install_profile_plugin(&node_path(), &fake_dsh, &home, "web", "pkg")
            .unwrap_err();
        assert!(
            rejected.contains("已取消"),
            "取消期间的新操作应被拒绝: {rejected}"
        );

        release_tx.send(()).unwrap();
        assert!(cancel_handle.join().unwrap().is_ok());
        let error = handle.join().unwrap().unwrap_err();
        assert!(error.contains("已取消"));
        assert!(!ops.is_busy(), "cancel_current 返回后 active 槽位应已清空");

        let fast_fake_dsh = write_script(
            &root,
            "after-cancel.js",
            &fake_dsh_install_script(&root.join("after-cancel.json")),
        );
        let result = ops
            .install_profile_plugin(&node_path(), &fast_fake_dsh, &home, "web", "pkg")
            .unwrap();
        assert!(result.ok, "取消清理完成后应立即允许新操作");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn invalid_args_and_profiles_are_rejected() {
        let ops = PluginOps::new();
        let node = node_path();
        let dsh = Path::new("fake-dsh.js");
        let home = Path::new("/tmp");

        assert!(ops
            .install_profile_plugin(&node, dsh, home, "web", "")
            .is_err());
        assert!(ops
            .install_profile_plugin(&node, dsh, home, "web", "\0")
            .is_err());
        let long_spec = "x".repeat(513);
        assert!(ops
            .install_profile_plugin(&node, dsh, home, "web", &long_spec)
            .is_err());

        assert!(ops
            .remove_profile_plugin(&node, dsh, home, "", "pkg")
            .is_err());
        assert!(ops
            .remove_profile_plugin(&node, dsh, home, "../web", "pkg")
            .is_err());
        assert!(ops
            .remove_profile_plugin(&node, dsh, home, "web\\nested", "pkg")
            .is_err());
        assert!(ops
            .remove_profile_plugin(&node, dsh, home, "web", "..")
            .is_err());
        assert_eq!(
            ops.install_profile_plugin(&node, dsh, home, "web", "-pkg")
                .unwrap_err(),
            "spec 不能以 - 开头"
        );
        assert_eq!(
            ops.remove_profile_plugin(&node, dsh, home, "web", "-name")
                .unwrap_err(),
            "name 不能以 - 开头"
        );
        assert_eq!(
            ops.install_profile_plugin(&node, dsh, home, "-web", "pkg")
                .unwrap_err(),
            "profile 不能以 - 开头"
        );
    }

    #[test]
    fn with_logger_receives_plugin_output_lines() {
        let root = temp_root("logger");
        let home = setup_profile(&root, "web");
        let fake_dsh = write_script(
            &root,
            "fake-dsh.js",
            &fake_dsh_install_script(&root.join("result.json")),
        );
        let logged = Arc::new(Mutex::new(Vec::new()));
        let ops_logger = logged.clone();
        let ops = PluginOps::new().with_logger(move |line| {
            ops_logger.lock().unwrap().push(line.to_string());
        });

        let result = ops
            .update_profile_plugins(&node_path(), &fake_dsh, &home, "web")
            .unwrap();

        assert!(result.ok);
        let logged = logged.lock().unwrap();
        assert!(logged.iter().any(|line| line == "stdout from fake dsh"));
        assert!(logged.iter().any(|line| line == "stderr from fake dsh"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn profile_named_node_modules_is_rejected() {
        let ops = PluginOps::new();
        let node = node_path();
        let dsh = Path::new("fake-dsh.js");
        let home = Path::new("/tmp");

        assert!(ops
            .install_profile_plugin(&node, dsh, home, "node_modules", "pkg")
            .is_err());
    }
}
