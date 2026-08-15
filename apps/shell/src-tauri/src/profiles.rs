use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const STATE_VERSION: u8 = 1;
const DEFAULT_PROFILE: &str = "web";
const BASE_BUNDLE: &str = "@deepseek-ai/dsh-base";
const WEB_APP_BUNDLE: &str = "@deepseek-ai/dsh-web-app";

/// 临时状态文件序号，避免同进程并发写入时复用同一路径。
static STATE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// dsh profile 摘要（profile 发现结果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DshProfileSummary {
    pub name: String,
    pub dir: String,
    pub exists: bool,
    pub web_capable: bool,
    pub problem: Option<String>,
}

/// profile 切换状态机快照（版本化，便于损坏恢复）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProfileState {
    #[serde(default = "default_state_version")]
    pub version: u8,
    #[serde(default = "default_profile")]
    pub active: String,
    #[serde(default)]
    pub pending: Option<String>,
    #[serde(default = "default_profile")]
    pub last_known_good: String,
}

impl Default for ProfileState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            active: DEFAULT_PROFILE.to_string(),
            pending: None,
            last_known_good: DEFAULT_PROFILE.to_string(),
        }
    }
}

/// 本次启动实际使用的 profile 上下文，供 Task 1.2 在健康检查后提交或回滚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupContext {
    pub active: String,
    pub last_known_good: String,
}

fn default_state_version() -> u8 {
    STATE_VERSION
}

fn default_profile() -> String {
    DEFAULT_PROFILE.to_string()
}

fn invalid_summary(name: String, dir: String, problem: impl Into<String>) -> DshProfileSummary {
    DshProfileSummary {
        name,
        dir,
        exists: true,
        web_capable: false,
        problem: Some(problem.into()),
    }
}

fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name != "node_modules"
        && !name.contains('/')
        && !name.contains('\\')
}

fn profile_summary(dir: &Path, package_path: &Path) -> DshProfileSummary {
    let name = dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let dir_string = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();

    let raw = match fs::read_to_string(package_path) {
        Ok(raw) => raw,
        Err(error) => {
            return invalid_summary(name, dir_string, format!("读取 package.json 失败: {error}"))
        }
    };
    let root: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(root) => root,
        Err(error) => {
            return invalid_summary(name, dir_string, format!("package.json 解析失败: {error}"))
        }
    };
    let dsh = match root.get("dsh").and_then(|value| value.as_object()) {
        Some(dsh) => dsh,
        None => return invalid_summary(name, dir_string, "缺少 dsh 配置"),
    };
    let profile = match dsh.get("profile").and_then(|value| value.as_object()) {
        Some(profile) => profile,
        None => return invalid_summary(name, dir_string, "缺少 dsh.profile 配置"),
    };
    let bundles = match profile.get("bundles").and_then(|value| value.as_array()) {
        Some(bundles) => bundles,
        None => return invalid_summary(name, dir_string, "缺少 dsh.profile.bundles"),
    };

    let mut bundle_names = Vec::with_capacity(bundles.len());
    for (index, bundle) in bundles.iter().enumerate() {
        match bundle.as_str() {
            Some(value) => bundle_names.push(value.to_string()),
            None => {
                return invalid_summary(
                    name,
                    dir_string,
                    format!("dsh.profile.bundles[{index}] 必须是字符串"),
                )
            }
        }
    }

    let base_position = bundle_names.iter().position(|bundle| bundle == BASE_BUNDLE);
    let web_app_position = bundle_names
        .iter()
        .position(|bundle| bundle == WEB_APP_BUNDLE);
    let (problem, web_capable) = match (base_position, web_app_position) {
        (Some(base), Some(web_app)) if base < web_app => (None, true),
        (None, _) => (
            Some("bundles 缺少 @deepseek-ai/dsh-base".to_string()),
            false,
        ),
        (_, None) => (
            Some("bundles 缺少 @deepseek-ai/dsh-web-app".to_string()),
            false,
        ),
        (Some(_), Some(_)) => (
            Some("@deepseek-ai/dsh-base 必须在 @deepseek-ai/dsh-web-app 之前".to_string()),
            false,
        ),
    };

    DshProfileSummary {
        name,
        dir: dir_string,
        exists: true,
        web_capable,
        problem,
    }
}

/// 遍历 $DSH_HOME/profiles/*/package.json，非法 manifest 以 problem 呈现而不抛错。
pub fn list_profiles(home: &Path) -> Vec<DshProfileSummary> {
    let profiles_dir = home.join("profiles");
    let Ok(entries) = fs::read_dir(&profiles_dir) else {
        return Vec::new();
    };

    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name == "node_modules" || name.starts_with('.') {
            continue;
        }
        let package_path = dir.join("package.json");
        if !package_path.is_file() {
            continue;
        }
        profiles.push(profile_summary(&dir, &package_path));
    }
    profiles.sort_by(|left, right| left.name.cmp(&right.name));
    profiles
}

/// 读取 profile 状态；缺失或损坏时恢复默认状态，不 panic。
pub fn load_state(path: &Path) -> ProfileState {
    let Ok(raw) = fs::read_to_string(path) else {
        return ProfileState::default();
    };
    let Ok(mut state) = serde_json::from_str::<ProfileState>(&raw) else {
        return ProfileState::default();
    };
    sanitize_state(&mut state);
    state
}

fn sanitize_state(state: &mut ProfileState) {
    if state.version != STATE_VERSION {
        *state = ProfileState::default();
        return;
    }
    if !is_valid_profile_name(&state.active) {
        state.active = DEFAULT_PROFILE.to_string();
    }
    if state
        .pending
        .as_deref()
        .map_or(false, |name| !is_valid_profile_name(name))
    {
        state.pending = None;
    }
    if !is_valid_profile_name(&state.last_known_good) {
        state.last_known_good = DEFAULT_PROFILE.to_string();
    }
}

#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("同步状态目录失败: {error}"))
}

/// 原子保存状态：同目录临时文件 + rename，Unix 下权限固定为 0o600。
pub fn save_state(path: &Path, state: &ProfileState) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| format!("创建状态目录失败: {error}"))?;
    }
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "profile-state".to_string());
    let nonce = STATE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = path.with_file_name(format!("{file_name}.{}.{nonce}.tmp", std::process::id()));
    let json = serde_json::to_string_pretty(state)
        .map_err(|error| format!("序列化 profile 状态失败: {error}"))?;

    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp_path)
            .map_err(|error| format!("创建临时状态文件失败: {error}"))?;
        file.write_all(json.as_bytes())
            .map_err(|error| format!("写入临时状态文件失败: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("同步临时状态文件失败: {error}"))?;
        fs::rename(&temp_path, path).map_err(|error| format!("替换状态文件失败: {error}"))?;
        #[cfg(unix)]
        sync_parent_dir(path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn is_web_capable(profiles: &[DshProfileSummary], name: &str) -> bool {
    if name == DEFAULT_PROFILE {
        return true;
    }
    profiles
        .iter()
        .any(|profile| profile.name == name && profile.web_capable)
}

/// 启动前选择 profile：优先 pending，其次 last_known_good，最后回退 web；随后清空 pending。
pub fn begin_startup(state_path: &Path, home: &Path) -> Result<StartupContext, String> {
    let profiles = list_profiles(home);
    let mut state = load_state(state_path);
    let last_known_good = state.last_known_good.clone();
    let pending = state.pending.clone();

    let pending_valid = pending
        .as_deref()
        .map_or(false, |name| is_web_capable(&profiles, name));
    let last_known_good_valid = is_web_capable(&profiles, &last_known_good);

    if !last_known_good_valid {
        state.last_known_good = DEFAULT_PROFILE.to_string();
    }

    let active = if pending_valid {
        pending.unwrap_or_default()
    } else if last_known_good_valid {
        last_known_good.clone()
    } else {
        DEFAULT_PROFILE.to_string()
    };
    let context = StartupContext {
        active: active.clone(),
        last_known_good: if last_known_good_valid {
            last_known_good
        } else {
            DEFAULT_PROFILE.to_string()
        },
    };

    state.active = active;
    state.pending = None;
    save_state(state_path, &state)?;
    Ok(context)
}

/// 选择 profile：校验存在且 web_capable，只写入 pending，不改变 active / last_known_good。
pub fn select_profile(state_path: &Path, home: &Path, name: &str) -> Result<ProfileState, String> {
    if !is_valid_profile_name(name) {
        return Err("无效 profile 名称".to_string());
    }
    let web_capable = list_profiles(home)
        .iter()
        .any(|profile| profile.name == name && profile.web_capable);
    if !web_capable {
        return Err(format!("profile {name} 不存在或不可作为 web profile"));
    }

    let mut state = load_state(state_path);
    state.pending = Some(name.to_string());
    save_state(state_path, &state)?;
    Ok(state)
}

/// 启动成功且健康检查通过后提交 last_known_good。
pub fn mark_healthy(state_path: &Path, active: &str) -> Result<ProfileState, String> {
    let mut state = load_state(state_path);
    if state.active != active {
        return Err("active 与当前 profile 状态不一致".to_string());
    }
    state.last_known_good = active.to_string();
    state.pending = None;
    save_state(state_path, &state)?;
    Ok(state)
}

/// 启动失败时回滚：active 恢复为 last_known_good，并清空 pending。
pub fn rollback_startup(state_path: &Path) -> Result<ProfileState, String> {
    let mut state = load_state(state_path);
    state.active = state.last_known_good.clone();
    state.pending = None;
    save_state(state_path, &state)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "dsh-profiles-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_profile(home: &Path, name: &str, bundles: &[&str]) -> PathBuf {
        let dir = home.join("profiles").join(name);
        fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "name": format!("dsh-profile-{name}"),
            "dsh": {
                "profile": {
                    "bundles": bundles,
                }
            }
        });
        fs::write(
            dir.join("package.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        dir
    }

    #[test]
    fn list_profiles_marks_invalid_manifests() {
        let root = temp_root("list");
        let home = root.join("home");
        write_profile(
            &home,
            "web",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        write_profile(
            &home,
            "wrong-order",
            &["@deepseek-ai/dsh-web-app", "@deepseek-ai/dsh-base"],
        );

        let broken = home.join("profiles").join("broken");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join("package.json"), "{ not json").unwrap();

        let missing_bundles_dir = home.join("profiles").join("missing-bundles");
        fs::create_dir_all(&missing_bundles_dir).unwrap();
        fs::write(
            missing_bundles_dir.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "name": "missing-bundles",
                "dsh": {
                    "profile": {}
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let non_string_bundles_dir = home.join("profiles").join("non-string-bundles");
        fs::create_dir_all(&non_string_bundles_dir).unwrap();
        fs::write(
            non_string_bundles_dir.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "name": "non-string-bundles",
                "dsh": {
                    "profile": {
                        "bundles": [1]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let no_package_dir = home.join("profiles").join("no-package");
        fs::create_dir_all(&no_package_dir).unwrap();

        let ignored = home.join("profiles").join("node_modules");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(
            ignored.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "name": "ignored",
                "dsh": {
                    "profile": {
                        "bundles": ["@deepseek-ai/dsh-base"]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let profiles = list_profiles(&home);
        assert_eq!(profiles.len(), 5);

        let web = profiles.iter().find(|p| p.name == "web").unwrap();
        assert!(web.exists);
        assert!(web.web_capable);
        assert!(web.problem.is_none());

        let wrong_order = profiles.iter().find(|p| p.name == "wrong-order").unwrap();
        assert!(wrong_order.exists);
        assert!(!wrong_order.web_capable);
        assert!(wrong_order.problem.is_some());

        let broken = profiles.iter().find(|p| p.name == "broken").unwrap();
        assert!(broken.exists);
        assert!(!broken.web_capable);
        assert!(broken.problem.is_some());

        assert!(!profiles.iter().any(|p| p.name == "node_modules"));

        let missing_bundles = profiles
            .iter()
            .find(|p| p.name == "missing-bundles")
            .unwrap();
        assert!(missing_bundles.exists);
        assert!(!missing_bundles.web_capable);
        assert!(missing_bundles.problem.is_some());

        let non_string_bundles = profiles
            .iter()
            .find(|p| p.name == "non-string-bundles")
            .unwrap();
        assert!(non_string_bundles.exists);
        assert!(!non_string_bundles.web_capable);
        assert!(non_string_bundles.problem.is_some());

        assert!(!profiles.iter().any(|p| p.name == "no-package"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn begin_startup_prioritizes_pending_and_clears_it() {
        let root = temp_root("begin-pending");
        let home = root.join("home");
        write_profile(
            &home,
            "web",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        write_profile(
            &home,
            "custom",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "web".into(),
                pending: Some("custom".into()),
                last_known_good: "web".into(),
            },
        )
        .unwrap();

        let context = begin_startup(&state_path, &home).unwrap();
        assert_eq!(context.active, "custom");
        assert_eq!(context.last_known_good, "web");

        let state = load_state(&state_path);
        assert_eq!(state.active, "custom");
        assert_eq!(state.pending, None);
        assert_eq!(state.last_known_good, "web");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn begin_startup_normalizes_invalid_last_known_good_to_web() {
        let root = temp_root("begin-lkg");
        let home = root.join("home");
        write_profile(
            &home,
            "broken",
            &["@deepseek-ai/dsh-web-app", "@deepseek-ai/dsh-base"],
        );
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "broken".into(),
                pending: None,
                last_known_good: "missing".into(),
            },
        )
        .unwrap();

        let context = begin_startup(&state_path, &home).unwrap();
        assert_eq!(context.active, "web");
        assert_eq!(context.last_known_good, "web");
        assert_eq!(load_state(&state_path).last_known_good, "web");

        let state = rollback_startup(&state_path).unwrap();
        assert_eq!(state.active, "web");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn begin_startup_rolls_back_invalid_pending_to_last_known_good() {
        let root = temp_root("begin-rollback");
        let home = root.join("home");
        write_profile(
            &home,
            "web",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "web".into(),
                pending: Some("missing".into()),
                last_known_good: "web".into(),
            },
        )
        .unwrap();

        let context = begin_startup(&state_path, &home).unwrap();
        assert_eq!(context.active, "web");
        assert_eq!(context.last_known_good, "web");

        let state = load_state(&state_path);
        assert_eq!(state.active, "web");
        assert_eq!(state.pending, None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn select_profile_writes_pending_without_changing_active() {
        let root = temp_root("select");
        let home = root.join("home");
        write_profile(
            &home,
            "web",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        write_profile(
            &home,
            "custom",
            &["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"],
        );
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "web".into(),
                pending: None,
                last_known_good: "web".into(),
            },
        )
        .unwrap();

        let state = select_profile(&state_path, &home, "custom").unwrap();
        assert_eq!(state.active, "web");
        assert_eq!(state.pending.as_deref(), Some("custom"));
        assert_eq!(state.last_known_good, "web");
        assert_eq!(load_state(&state_path), state);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn mark_healthy_commits_last_known_good() {
        let root = temp_root("healthy");
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "custom".into(),
                pending: Some("custom".into()),
                last_known_good: "web".into(),
            },
        )
        .unwrap();

        let state = mark_healthy(&state_path, "custom").unwrap();
        assert_eq!(state.active, "custom");
        assert_eq!(state.last_known_good, "custom");
        assert_eq!(state.pending, None);
        assert_eq!(load_state(&state_path), state);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn mark_healthy_rejects_active_mismatch() {
        let root = temp_root("healthy-mismatch");
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "web".into(),
                pending: None,
                last_known_good: "web".into(),
            },
        )
        .unwrap();
        let before = load_state(&state_path);

        assert!(mark_healthy(&state_path, "custom").is_err());
        assert_eq!(load_state(&state_path), before);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rollback_startup_returns_to_last_known_good() {
        let root = temp_root("rollback");
        let state_path = root.join("profile-state.json");
        save_state(
            &state_path,
            &ProfileState {
                version: 1,
                active: "custom".into(),
                pending: Some("custom".into()),
                last_known_good: "web".into(),
            },
        )
        .unwrap();

        let state = rollback_startup(&state_path).unwrap();
        assert_eq!(state.active, "web");
        assert_eq!(state.pending, None);
        assert_eq!(state.last_known_good, "web");
        assert_eq!(load_state(&state_path), state);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn damaged_state_recovers_to_default() {
        let root = temp_root("damaged");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let state_path = root.join("profile-state.json");
        fs::write(&state_path, "{ not json").unwrap();

        assert_eq!(load_state(&state_path), ProfileState::default());

        let context = begin_startup(&state_path, &home).unwrap();
        assert_eq!(context.active, "web");
        assert_eq!(context.last_known_good, "web");
        assert_eq!(load_state(&state_path), ProfileState::default());

        fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn save_state_uses_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("mode");
        let state_path = root.join("profile-state.json");
        save_state(&state_path, &ProfileState::default()).unwrap();

        let mode = fs::metadata(&state_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        fs::remove_dir_all(&root).ok();
    }
}
