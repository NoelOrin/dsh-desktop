use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DshConfig {
    pub dsh_bin: Option<String>,
    pub dsh_node: Option<String>,
    pub dsh_home: Option<String>,
    /// 外部远程插件预设文件；显式路径优先，否则回退到 DSH_DESKTOP_REMOTE_PLUGINS_PATH。
    #[serde(default)]
    pub remote_plugins_path: Option<String>,
    #[serde(default)]
    pub shortcuts: Vec<String>,
    #[serde(default)]
    pub remote_plugins: Vec<RemotePluginPreset>,
}

/// 远程插件预设来源。外部文件中的预设为固定配置，UI 不允许直接改写。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemotePluginSource {
    #[default]
    Local,
    External,
}

/// 用户预设的远程插件源；启动时对 active profile 执行 `dsh plugin add <url>`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RemotePluginPreset {
    #[serde(default)]
    pub id: String,
    pub url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_remote_group")]
    pub group: String,
    #[serde(default)]
    pub source: RemotePluginSource,
}

fn default_enabled() -> bool {
    true
}

fn default_remote_group() -> String {
    "default".to_string()
}

impl DshConfig {
    /// config.json 中的值优先于环境变量；返回“生效配置”。
    pub fn effective(&self, getenv: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            dsh_bin: self.dsh_bin.clone().or_else(|| getenv("DSH_BIN")),
            dsh_node: self.dsh_node.clone().or_else(|| getenv("DSH_NODE")),
            dsh_home: self.dsh_home.clone().or_else(|| getenv("DSH_HOME")),
            remote_plugins_path: self
                .remote_plugins_path
                .clone()
                .or_else(|| getenv("DSH_DESKTOP_REMOTE_PLUGINS_PATH")),
            shortcuts: self.shortcuts.clone(),
            remote_plugins: self.remote_plugins.clone(),
        }
    }
}

pub fn load(path: &Path) -> DshConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, config: &DshConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_prefers_stored_over_env() {
        let config = DshConfig {
            dsh_bin: Some("/stored/bin".into()),
            dsh_node: None,
            dsh_home: None,
            remote_plugins_path: Some("/stored/plugins.json".into()),
            shortcuts: vec![],
            remote_plugins: vec![],
        };
        let effective = config.effective(|k| match k {
            "DSH_BIN" => Some("/env/bin".into()),
            "DSH_DESKTOP_REMOTE_PLUGINS_PATH" => Some("/env/plugins.json".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_bin.as_deref(), Some("/stored/bin"));
        assert_eq!(
            effective.remote_plugins_path.as_deref(),
            Some("/stored/plugins.json")
        );
    }

    #[test]
    fn effective_falls_back_to_env() {
        let config = DshConfig::default();
        let effective = config.effective(|k| match k {
            "DSH_NODE" => Some("/env/node".into()),
            "DSH_DESKTOP_REMOTE_PLUGINS_PATH" => Some("/env/plugins.json".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_node.as_deref(), Some("/env/node"));
        assert_eq!(
            effective.remote_plugins_path.as_deref(),
            Some("/env/plugins.json")
        );
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("dsh-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let config = DshConfig {
            dsh_bin: Some("/a".into()),
            dsh_node: Some("/b".into()),
            dsh_home: Some("/c".into()),
            remote_plugins_path: Some("/external/plugins.json".into()),
            shortcuts: vec!["CmdOrCtrl+Shift+D".into()],
            remote_plugins: vec![RemotePluginPreset {
                id: "remote-1".into(),
                url: "https://example.com/plugin.tgz".into(),
                enabled: true,
                group: "tools".into(),
                source: RemotePluginSource::External,
            }],
        };
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded, config);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn old_config_without_remote_plugins_loads_with_defaults() {
        let dir = std::env::temp_dir().join(format!("dsh-config-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"dsh_home":"/tmp/dsh"}"#).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.dsh_home.as_deref(), Some("/tmp/dsh"));
        assert!(loaded.remote_plugins.is_empty());
        assert!(loaded.remote_plugins_path.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_preset_defaults_group_and_source() {
        let json = r#"{"id":"legacy","url":"https://example.com/plugin.tgz","enabled":true}"#;
        let preset: RemotePluginPreset = serde_json::from_str(json).unwrap();
        assert_eq!(preset.id, "legacy");
        assert_eq!(preset.group, "default");
        assert_eq!(preset.source, RemotePluginSource::Local);
    }
}
