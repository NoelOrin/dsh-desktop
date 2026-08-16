use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const BUSY_MESSAGE: &str = "当前只有一个插件操作可运行";
const CANCELLED_MESSAGE: &str = "插件操作已取消";
const MAX_ARG_LENGTH: usize = 512;
const POLL_INTERVAL: Duration = Duration::from_millis(25);

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

/// 当前唯一的 dsh plugin 操作运行器。
#[derive(Clone)]
pub struct PluginOps {
    active: Arc<Mutex<Option<RunningOp>>>,
}

impl Default for PluginOps {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginOps {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
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
        self.active.lock().unwrap().is_some()
    }

    /// 取消当前操作；没有运行中的操作时直接返回成功。
    pub fn cancel_current(&self) -> Result<(), String> {
        let Some(running) = self.active.lock().unwrap().take() else {
            return Ok(());
        };
        running.cancelled.store(true, Ordering::SeqCst);
        let child = running.child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
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

        let (child_state, cancelled) = {
            let mut active = self.active.lock().unwrap();
            if active.is_some() {
                return Err(BUSY_MESSAGE.to_string());
            }
            let running = RunningOp {
                child: Arc::new(Mutex::new(None)),
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            let child_state = running.child.clone();
            let cancelled = running.cancelled.clone();
            active.replace(running);
            (child_state, cancelled)
        };

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.active.lock().unwrap().take();
                return Err(format!("无法启动插件操作: {error}"));
            }
        };

        let stdout = child.stdout.take().expect("stdout 已开启管道");
        let stderr = child.stderr.take().expect("stderr 已开启管道");
        {
            let mut state = child_state.lock().unwrap();
            if cancelled.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                drop(state);
                self.active.lock().unwrap().take();
                return Err(CANCELLED_MESSAGE.to_string());
            }
            *state = Some(child);
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let stdout_handle = thread::spawn({
            let output = output.clone();
            move || collect_stream(stdout, output)
        });
        let stderr_handle = thread::spawn({
            let output = output.clone();
            move || collect_stream(stderr, output)
        });

        let status = loop {
            let mut state = child_state.lock().unwrap();
            match state
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
            {
                Some(status) => {
                    state.take();
                    break Some(status);
                }
                None if state.is_none() => break None,
                None => {}
            }
            drop(state);
            thread::sleep(POLL_INTERVAL);
        };
        self.active.lock().unwrap().take();

        let _ = stdout_handle.join();
        let _ = stderr_handle.join();
        let output = output.lock().unwrap().clone();

        match status {
            Some(status) => Ok(PluginOperationResult::from_status(status, output)),
            None => Err(CANCELLED_MESSAGE.to_string()),
        }
    }
}

fn collect_stream(stream: impl std::io::Read + Send + 'static, output: Arc<Mutex<Vec<String>>>) {
    let reader = BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim_end().to_string();
        if !line.is_empty() {
            output.lock().unwrap().push(line);
        }
    }
}

fn validate_value(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} 不能为空"));
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
    if profile == "." || profile == ".." || profile.contains('/') || profile.contains('\\') {
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
    }
}
