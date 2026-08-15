use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Process, ProcessesToUpdate, System};

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
    if !cmd.iter().any(|arg| arg.to_string_lossy() == "web") {
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
    fn parses_url_line() {
        assert_eq!(
            parse_dsh_web_url("dsh web: http://127.0.0.1:49321"),
            Some("http://127.0.0.1:49321".into())
        );
        assert_eq!(parse_dsh_web_url("random log line"), None);
    }

    #[test]
    fn classifies_drop_kind() {
        let dir = std::env::temp_dir();
        assert_eq!(classify_drop(std::slice::from_ref(&dir)), "directory");
        assert_eq!(classify_drop(&[]), "file");
    }
}
