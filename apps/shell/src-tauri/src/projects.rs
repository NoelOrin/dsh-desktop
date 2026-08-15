use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// 壳侧维护的本地项目条目（项目列表与右键菜单操作的唯一状态源）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub pinned: bool,
    /// 该项目下的会话聊天是否已归档。
    #[serde(default)]
    pub archived_chats: bool,
    #[serde(default)]
    pub unread_chats: u32,
    /// 最近一次“全部标为已读”的时间（unix 毫秒）。
    #[serde(default)]
    pub read_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// 创建永久工作树的结果：工作树目录已作为新项目加入列表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProjectWorktreeResult {
    pub entry: ProjectEntry,
    pub target: String,
    pub branch: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
struct ProjectsFile {
    projects: Vec<ProjectEntry>,
}

pub fn load(path: &Path) -> Vec<ProjectEntry> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ProjectsFile>(&raw).ok())
        .map(|file| file.projects)
        .unwrap_or_default()
}

pub fn save(path: &Path, projects: &[ProjectEntry]) -> Result<(), String> {
    let file = ProjectsFile {
        projects: projects.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 以路径哈希 + 纳秒时间生成稳定且唯一的项目 id。
pub fn make_id(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos}{:016x}", hasher.finish())
}

/// 把任意字符串整理成可用于目录名 / 分支名的组件（保留中文等字母数字，其余转连字符）。
fn sanitize_component(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches(['-', '.']).to_string();
    if trimmed.is_empty() {
        "worktree".to_string()
    } else {
        trimmed
    }
}

fn unique_dir(base: &Path, name: &str) -> PathBuf {
    let mut candidate = base.join(name);
    let mut index = 2;
    while candidate.exists() {
        candidate = base.join(format!("{name}-{index}"));
        index += 1;
    }
    candidate
}

fn unique_branch(repo: &Path, name: &str) -> String {
    let base = format!("wt/{name}");
    let mut branch = base.clone();
    let mut index = 2;
    loop {
        let exists = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !exists {
            return branch;
        }
        branch = format!("{base}-{index}");
        index += 1;
    }
}

/// 在项目目录旁创建永久 git 工作树，返回（目标目录，分支名）。
pub fn create_worktree(
    project: &ProjectEntry,
    requested_name: Option<&str>,
) -> Result<(PathBuf, String), String> {
    let repo = PathBuf::from(&project.path);
    if !repo.is_dir() {
        return Err(format!("项目目录不存在: {}", project.path));
    }
    let check = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("无法执行 git: {e}"))?;
    if !check.status.success() {
        return Err("项目不是 Git 仓库，无法创建永久工作树".to_string());
    }

    let base_name = requested_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}-worktree", sanitize_component(&project.name)));
    let safe_name = sanitize_component(&base_name);
    let parent = repo.parent().unwrap_or(Path::new("."));
    let target = unique_dir(parent, &safe_name);
    let branch = unique_branch(&repo, &safe_name);

    let run = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "add", "-b", &branch])
        .arg(&target)
        .output()
        .map_err(|e| format!("无法执行 git: {e}"))?;
    if !run.status.success() {
        return Err(format!(
            "创建永久工作树失败: {}",
            String::from_utf8_lossy(&run.stderr).trim()
        ));
    }
    Ok((target, branch))
}

/// 在系统文件管理器中定位项目目录（macOS Finder 高亮、Windows 资源管理器选中、Linux 打开父目录）。
pub fn reveal_in_finder(path: &str) -> Result<(), String> {
    let target = PathBuf::from(path);
    if !target.exists() {
        return Err(format!("路径不存在: {path}"));
    }
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg("-R").arg(&target).status();
    #[cfg(target_os = "linux")]
    let status = {
        let parent = target.parent().unwrap_or(Path::new("/"));
        Command::new("xdg-open").arg(parent).status()
    };
    #[cfg(target_os = "windows")]
    let status = Command::new("explorer")
        .arg(format!("/select,{}", target.display()))
        .status();
    status
        .map(|_| ())
        .map_err(|e| format!("无法在文件管理器中显示 {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("dsh-projects-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.json");
        let entry = ProjectEntry {
            id: "abc".into(),
            name: "demo".into(),
            path: "/tmp/demo".into(),
            pinned: true,
            archived_chats: true,
            unread_chats: 3,
            read_at: Some(42),
            created_at: 1,
            updated_at: 2,
        };
        save(&path, &[entry.clone()]).unwrap();
        assert_eq!(load(&path), vec![entry]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_loads_empty() {
        let path = Path::new("/nonexistent/dsh-projects-missing.json");
        assert!(load(path).is_empty());
    }

    #[test]
    fn sanitize_component_keeps_cjk() {
        assert_eq!(sanitize_component("My 项目/测试!"), "My-项目-测试");
        assert_eq!(sanitize_component("..."), "worktree");
    }

    #[test]
    fn unique_dir_appends_suffix() {
        let dir = std::env::temp_dir().join(format!("dsh-worktree-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("demo-worktree")).unwrap();
        let target = unique_dir(&dir, "demo-worktree");
        assert_eq!(
            target.file_name().unwrap().to_string_lossy(),
            "demo-worktree-2"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
