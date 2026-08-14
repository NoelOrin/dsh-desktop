use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DshConfig {
    pub dsh_bin: Option<String>,
    pub dsh_node: Option<String>,
    pub dsh_home: Option<String>,
}

impl DshConfig {
    /// config.json 中的值优先于环境变量；返回“生效配置”。
    pub fn effective(&self, getenv: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            dsh_bin: self.dsh_bin.clone().or_else(|| getenv("DSH_BIN")),
            dsh_node: self.dsh_node.clone().or_else(|| getenv("DSH_NODE")),
            dsh_home: self.dsh_home.clone().or_else(|| getenv("DSH_HOME")),
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
        };
        let effective = config.effective(|k| match k {
            "DSH_BIN" => Some("/env/bin".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_bin.as_deref(), Some("/stored/bin"));
    }

    #[test]
    fn effective_falls_back_to_env() {
        let config = DshConfig::default();
        let effective = config.effective(|k| match k {
            "DSH_NODE" => Some("/env/node".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_node.as_deref(), Some("/env/node"));
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
        };
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded, config);
        std::fs::remove_dir_all(&dir).ok();
    }
}
