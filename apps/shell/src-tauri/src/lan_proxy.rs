use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// 内嵌 lan-proxy 脚本源码：随二进制编译进应用，启动时写入应用数据目录。
pub const SCRIPT_SOURCE: &str = include_str!("../../../../scripts/lan-proxy.mjs");

const DEFAULT_BIND: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_TARGET: &str = "127.0.0.1:53553";
const MAX_MESSAGES: usize = 200;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

fn default_bind() -> String {
    DEFAULT_BIND.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// 局域网反向代理配置，存于壳侧 `lan-proxy.json`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LanProxySettings {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// 上游 dsh web 地址；留空时自动使用当前 dsh URL，未就绪时回退默认端口。
    #[serde(default)]
    pub target: Option<String>,
}

impl Default for LanProxySettings {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            target: None,
        }
    }
}

/// 桥接设置页展示的代理状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LanProxySnapshot {
    pub settings: LanProxySettings,
    pub running: bool,
    pub url: Option<String>,
    pub urls: Vec<String>,
    pub error: Option<String>,
}

/// 受管 lan-proxy 子进程：负责启动/停止、捕获输出与状态快照。
#[derive(Default)]
pub struct LanProxyManager {
    child: Arc<Mutex<Option<Child>>>,
    messages: Arc<Mutex<Vec<String>>>,
    expected_stop: Arc<AtomicBool>,
}

impl LanProxyManager {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            messages: Arc::new(Mutex::new(Vec::new())),
            expected_stop: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn start(
        &self,
        node: &Path,
        script: &Path,
        settings: &LanProxySettings,
        current_url: Option<&str>,
    ) -> Result<(), String> {
        validate(settings)?;
        self.stop();
        ensure_script(script)?;

        let target = resolve_target(settings, current_url)?;
        let mut command = Command::new(node);
        command
            .arg(script)
            .arg("--bind")
            .arg(&settings.bind)
            .arg("--port")
            .arg(settings.port.to_string())
            .arg("--target")
            .arg(&target)
            .env("PATH", crate::effective_path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动局域网代理: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "无法读取局域网代理输出".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法读取局域网代理输出".to_string())?;

        self.messages.lock().unwrap().clear();
        spawn_reader(stdout, self.messages.clone());
        spawn_reader(stderr, self.messages.clone());

        self.expected_stop.store(false, Ordering::SeqCst);
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    pub fn stop(&self) {
        self.expected_stop.store(true, Ordering::SeqCst);
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            terminate_child(&mut child);
        }
        self.messages.lock().unwrap().clear();
    }

    pub fn is_running(&self) -> bool {
        self.child_is_alive()
    }

    pub fn snapshot(&self, settings: &LanProxySettings) -> LanProxySnapshot {
        let running = self.child_is_alive();
        let messages = self.messages.lock().unwrap().clone();
        let urls = if running {
            extract_urls(&messages)
        } else {
            Vec::new()
        };
        let url = urls.first().cloned().or_else(|| {
            if running && !settings.bind.trim().is_empty() {
                Some(format!("http://{}:{}", settings.bind.trim(), settings.port))
            } else {
                None
            }
        });
        let error = if !running && !self.expected_stop.load(Ordering::SeqCst) {
            latest_error(&messages).or_else(|| {
                if messages.is_empty() {
                    None
                } else {
                    Some("局域网代理进程已退出".to_string())
                }
            })
        } else {
            None
        };
        LanProxySnapshot {
            settings: settings.clone(),
            running,
            url,
            urls,
            error,
        }
    }

    fn child_is_alive(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        let Some(child) = guard.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                false
            }
            Ok(None) => true,
            Err(_) => {
                *guard = None;
                false
            }
        }
    }
}

fn ensure_script(path: &Path) -> Result<(), String> {
    fs::write(path, SCRIPT_SOURCE).map_err(|error| format!("写入 lan-proxy 脚本失败: {error}"))
}

pub fn load(path: &Path) -> LanProxySettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &LanProxySettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

pub fn validate(settings: &LanProxySettings) -> Result<(), String> {
    if settings.bind.trim().is_empty() {
        return Err("监听地址不能为空".to_string());
    }
    if settings.port == 0 {
        return Err("监听端口必须大于 0".to_string());
    }
    if let Some(target) = settings
        .target
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        validate_target(target)?;
    }
    Ok(())
}

fn validate_target(target: &str) -> Result<(), String> {
    let trimmed = target.trim();
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.contains('/')
        || trimmed.chars().any(char::is_whitespace)
    {
        return Err("上游 dsh 地址应为 host 或 host:port".to_string());
    }
    let parsed = url::Url::parse(&format!("http://{trimmed}"))
        .map_err(|_| "上游 dsh 地址无效".to_string())?;
    if parsed.host_str().is_none() {
        return Err("上游 dsh 地址无效".to_string());
    }
    Ok(())
}

fn resolve_target(
    settings: &LanProxySettings,
    current_url: Option<&str>,
) -> Result<String, String> {
    if let Some(target) = settings
        .target
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        validate_target(target)?;
        return Ok(target.trim().to_string());
    }
    if let Some(host) = current_url
        .and_then(|url| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .filter(|host| !host.is_empty())
    {
        return Ok(host.to_string());
    }
    Ok(DEFAULT_TARGET.to_string())
}

fn spawn_reader(stream: impl std::io::Read + Send + 'static, messages: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            let mut all = messages.lock().unwrap();
            all.push(line);
            if all.len() > MAX_MESSAGES {
                all.remove(0);
            }
        }
    });
}

fn extract_urls(messages: &[String]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("局域网访问:")
                .map(|value| value.trim().to_string())
        })
        .collect()
}

fn latest_error(messages: &[String]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("启动失败")
                || lower.contains("端口无效")
                || lower.contains("目标地址无效")
                || lower.contains("error")
        })
        .cloned()
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break;
            }
            thread::sleep(Duration::from_millis(25));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let settings = LanProxySettings::default();
        assert_eq!(settings.bind, "0.0.0.0");
        assert_eq!(settings.port, 8080);
        assert!(settings.target.is_none());
    }

    #[test]
    fn invalid_target_is_rejected() {
        let settings = LanProxySettings {
            target: Some("http://127.0.0.1:8080".into()),
            ..LanProxySettings::default()
        };
        assert!(validate(&settings).is_err());
    }

    #[test]
    fn resolve_target_uses_current_url_first() {
        let settings = LanProxySettings::default();
        let target = resolve_target(&settings, Some("http://127.0.0.1:53210")).unwrap();
        assert_eq!(target, "127.0.0.1:53210");
    }

    #[test]
    fn resolve_target_falls_back_to_default() {
        let settings = LanProxySettings::default();
        let target = resolve_target(&settings, None).unwrap();
        assert_eq!(target, DEFAULT_TARGET);
    }

    #[test]
    fn extract_urls_parses_script_output() {
        let lines = vec![
            "[lan-proxy] 已启动".into(),
            "  监听:      0.0.0.0:8080".into(),
            "  局域网访问: http://192.168.1.8:8080".into(),
        ];
        assert_eq!(extract_urls(&lines), vec!["http://192.168.1.8:8080"]);
    }
}
