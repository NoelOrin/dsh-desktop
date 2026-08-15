use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Process, ProcessesToUpdate, System};
use url::Url;

/// 应用自管理 dsh web 进程的环境标记，用于崩溃/异常退出后识别残留实例。
const MANAGED_ENV: &str = "DSH_DESKTOP_MANAGED=1";
const STALE_CLEANUP_GRACE: Duration = Duration::from_secs(2);
const STALE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 清理启动本应用管理的旧 dsh web 实例，返回识别到的进程数。
///
/// 只清理命令中带 `web`，且携带应用数据目录 overlay 或 `DSH_DESKTOP_MANAGED`
/// 标记的进程，避免误杀用户单独启动的 `dsh web`。
pub fn cleanup_stale_dsh_web(app_data_dir: &Path) -> usize {
    let marker = app_data_dir.to_string_lossy();
    let mut system = System::new_all();
    let mut pids: Vec<_> = system
        .processes()
        .values()
        .filter(|process| is_managed_dsh_web(process, &marker))
        .map(|process| process.pid())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    if pids.is_empty() {
        return 0;
    }

    let deadline = Instant::now() + STALE_CLEANUP_GRACE;
    let mut signalled = false;
    loop {
        let alive = pids
            .iter()
            .filter_map(|pid| system.process(*pid))
            .filter(|process| is_managed_dsh_web(process, &marker))
            .collect::<Vec<_>>();
        if alive.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            for process in alive {
                force_terminate_process(process);
            }
            break;
        }
        if !signalled {
            for process in alive {
                terminate_process(process);
            }
            signalled = true;
        }
        thread::sleep(STALE_POLL_INTERVAL);
        system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
    }

    pids.len()
}

fn is_managed_dsh_web(process: &Process, marker: &str) -> bool {
    is_managed_dsh_cmd(process.cmd(), process.environ(), marker)
}

fn is_managed_dsh_cmd(cmd: &[OsString], environ: &[OsString], marker: &str) -> bool {
    // 命令以字面量 `web`，或以成对的 `--profile <值>`（值不以 - 开头）识别自管理启动。
    let has_web = cmd.iter().any(|arg| arg.to_string_lossy() == "web");
    let has_profile = cmd.iter().enumerate().any(|(idx, arg)| {
        arg.to_string_lossy() == "--profile"
            && cmd
                .get(idx + 1)
                .is_some_and(|value| !value.to_string_lossy().starts_with('-'))
    });
    if !(has_web || has_profile) {
        return false;
    }
    let has_overlay = cmd.iter().any(|arg| arg.to_string_lossy().contains(marker));
    let has_managed_env = environ
        .iter()
        .any(|value| value.to_string_lossy() == MANAGED_ENV);
    has_overlay || has_managed_env
}

#[cfg(unix)]
/// 直接取进程组 id；sysinfo 的 `group_id()` 返回真实组 id（RGID），不能用于组信号。
fn process_group_id(process: &Process) -> Option<i32> {
    let pid = process.pid().as_u32() as i32;
    if pid <= 0 {
        return None;
    }
    let pgid = unsafe { libc::getpgid(pid) };
    (pgid > 0).then_some(pgid)
}

#[cfg(unix)]
fn terminate_process(process: &Process) {
    if let Some(group) = process_group_id(process) {
        let ok = unsafe { libc::kill(-group, libc::SIGTERM) } == 0;
        if ok {
            return;
        }
    }
    let _ = process.kill_with(sysinfo::Signal::Term);
}

#[cfg(not(unix))]
fn terminate_process(process: &Process) {
    process.kill();
}

#[cfg(unix)]
fn force_terminate_process(process: &Process) {
    if let Some(group) = process_group_id(process) {
        let ok = unsafe { libc::kill(-group, libc::SIGKILL) } == 0;
        if ok {
            return;
        }
    }
    let _ = process.kill_with(sysinfo::Signal::Kill);
}

#[cfg(not(unix))]
fn force_terminate_process(process: &Process) {
    process.kill();
}

const READINESS_PREFIX: &str = "dsh web: ";

/// 严格解析一条 dsh 就绪输出。
///
/// 只有完整的 `dsh web: http://127.0.0.1:<port>`（或 `localhost`）才算就绪；
/// 路径、查询、hash、默认端口、非 loopback host 一律拒绝。返回的 URL 统一规范为
/// `http://127.0.0.1:<port>`，供健康检查与窗口导航使用。
pub fn parse_readiness_line(line: &str) -> Result<Option<String>, String> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let Some(token) = trimmed.strip_prefix(READINESS_PREFIX) else {
        return Ok(None);
    };
    let token = token.split_whitespace().next().unwrap_or_default();
    let url = Url::parse(token).map_err(|_| format!("就绪 URL 无效: {token}"))?;
    let port = url
        .port()
        .ok_or_else(|| format!("就绪 URL 缺少显式端口: {token}"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    if url.scheme() != "http"
        || !loopback
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || port == 0
    {
        return Err(format!(
            "就绪 URL 必须是 loopback HTTP 且带显式端口: {token}"
        ));
    }
    Ok(Some(format!("http://127.0.0.1:{port}")))
}

/// 兼容入口：普通日志行返回 None，真正无效的 `dsh web:` 行也返回 None。
#[cfg(test)]
pub fn parse_dsh_web_url(line: &str) -> Option<String> {
    parse_readiness_line(line).ok().flatten()
}

/// 增量就绪解析器。支持任意分块、未换行结尾和冲突 URL 检测。
#[derive(Debug, Default)]
pub struct ReadinessParser {
    pending: String,
    ready: Option<String>,
}

impl ReadinessParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 消费一段可能跨行的 stdout chunk。
    #[cfg(test)]
    pub fn push_chunk(&mut self, chunk: &str) -> Result<Option<String>, String> {
        self.pending.push_str(chunk);
        loop {
            let Some(newline) = self.pending.find('\n') else {
                return Ok(self.ready.clone());
            };
            let line = self.pending[..newline].to_string();
            self.pending = self.pending[newline + 1..].to_string();
            if let Some(url) = self.push_line(&line)? {
                return Ok(Some(url));
            }
        }
    }

    /// 消费一行（调用方已按 `\n` 切分）。
    pub fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        let Some(url) = parse_readiness_line(line)? else {
            return Ok(None);
        };
        if let Some(previous) = &self.ready {
            if previous != &url {
                return Err(format!("dsh 输出了冲突的就绪 URL: {previous} 和 {url}"));
            }
            return Ok(Some(url));
        }
        self.ready = Some(url.clone());
        Ok(Some(url))
    }

    /// 流结束后要求已经得到就绪 URL；末尾未换行的完整行也能解析。
    pub fn finalize(&mut self) -> Result<Option<String>, String> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.push_line(&line)?;
        }
        self.ready
            .clone()
            .ok_or_else(|| "dsh 退出前未输出就绪 URL".to_string())
            .map(Some)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn identifies_managed_dsh_web_by_overlay() {
        let cmd = [
            OsString::from("/usr/local/bin/node"),
            OsString::from("/opt/homebrew/bin/dsh"),
            OsString::from("web"),
            OsString::from("--patch"),
            OsString::from("/tmp/dsh-desktop/embedded-plugins.patch.yml"),
        ];
        assert!(is_managed_dsh_cmd(&cmd, &[], "/tmp/dsh-desktop"));
        assert!(!is_managed_dsh_cmd(&cmd[..3], &[], "/tmp/dsh-desktop"));
        assert!(!is_managed_dsh_cmd(&cmd, &[], "/other/dsh-desktop"));
    }

    #[test]
    fn identifies_managed_dsh_web_by_env() {
        let cmd = [
            OsString::from("node"),
            OsString::from("dsh"),
            OsString::from("web"),
        ];
        let environ = [OsString::from(MANAGED_ENV)];
        assert!(is_managed_dsh_cmd(&cmd, &environ, "/tmp"));
        assert!(!is_managed_dsh_cmd(&cmd, &[], "/tmp"));
    }

    #[test]
    fn identifies_managed_dsh_by_profile_overlay() {
        let cmd = [
            OsString::from("/usr/local/bin/node"),
            OsString::from("/opt/homebrew/bin/dsh"),
            OsString::from("--profile"),
            OsString::from("custom"),
            OsString::from("--patch"),
            OsString::from("/tmp/dsh-desktop/embedded-plugins.patch.yml"),
        ];
        assert!(is_managed_dsh_cmd(&cmd, &[], "/tmp/dsh-desktop"));
        assert!(!is_managed_dsh_cmd(&cmd[..3], &[], "/tmp/dsh-desktop"));
        assert!(!is_managed_dsh_cmd(&cmd, &[], "/other/dsh-desktop"));
    }

    #[test]
    fn identifies_managed_dsh_by_profile_env() {
        let cmd = [
            OsString::from("node"),
            OsString::from("dsh"),
            OsString::from("--profile"),
            OsString::from("custom"),
        ];
        let environ = [OsString::from(MANAGED_ENV)];
        assert!(is_managed_dsh_cmd(&cmd, &environ, "/tmp"));
        assert!(!is_managed_dsh_cmd(&cmd, &[], "/tmp"));
    }

    #[test]
    fn does_not_match_flag_like_profile_value() {
        let cmd = [
            OsString::from("node"),
            OsString::from("dsh"),
            OsString::from("--profile"),
            OsString::from("--help"),
        ];
        let environ = [OsString::from(MANAGED_ENV)];
        assert!(!is_managed_dsh_cmd(&cmd, &environ, "/tmp"));
    }

    #[test]
    fn parses_url_line() {
        assert_eq!(
            parse_dsh_web_url("dsh web: http://127.0.0.1:49321"),
            Some("http://127.0.0.1:49321".into())
        );
        assert_eq!(
            parse_dsh_web_url("dsh web: http://localhost:49321"),
            Some("http://127.0.0.1:49321".into())
        );
        assert_eq!(parse_dsh_web_url("random log line"), None);
    }

    #[test]
    fn rejects_invalid_readiness_urls() {
        for line in [
            "dsh web: https://127.0.0.1:4173",
            "dsh web: http://0.0.0.0:4173",
            "dsh web: http://127.0.0.1:0",
            "dsh web: http://127.0.0.1:65536",
            "dsh web: http://127.0.0.1:4173/path",
            "dsh web: http://127.0.0.1:4173/?q=1",
            "dsh web: http://127.0.0.1:4173/#frag",
        ] {
            assert!(parse_readiness_line(line).is_err(), "应拒绝 {line}");
        }
    }

    #[test]
    fn parses_arbitrary_chunking() {
        let mut parser = ReadinessParser::new();
        assert_eq!(
            parser
                .push_chunk("Node warning: https://nodejs.org\n")
                .unwrap(),
            None
        );
        assert_eq!(parser.push_chunk("dsh we").unwrap(), None);
        assert_eq!(parser.push_chunk("b: http://127.0.").unwrap(), None);
        assert_eq!(
            parser.push_chunk("0.1:4173\nstartup complete\n").unwrap(),
            Some("http://127.0.0.1:4173".into())
        );
        assert_eq!(
            parser.finalize().unwrap(),
            Some("http://127.0.0.1:4173".into())
        );
    }

    #[test]
    fn finalizes_unterminated_readiness_line() {
        let mut parser = ReadinessParser::new();
        assert_eq!(
            parser
                .push_chunk("dsh web: http://127.0.0.1:51234")
                .unwrap(),
            None
        );
        assert_eq!(
            parser.finalize().unwrap(),
            Some("http://127.0.0.1:51234".into())
        );
    }

    #[test]
    fn rejects_conflicting_readiness_urls() {
        let mut parser = ReadinessParser::new();
        assert!(parser.push_line("dsh web: http://127.0.0.1:4173").is_ok());
        assert!(parser.push_line("dsh web: http://127.0.0.1:4174").is_err());
    }

    #[test]
    fn fails_when_stream_ends_without_readiness() {
        let mut parser = ReadinessParser::new();
        parser.push_chunk("ordinary startup output\n").unwrap();
        assert!(parser.finalize().is_err());
    }

    #[test]
    fn classifies_drop_kind() {
        let dir = std::env::temp_dir();
        assert_eq!(classify_drop(std::slice::from_ref(&dir)), "directory");
        assert_eq!(classify_drop(&[]), "file");
    }
}
