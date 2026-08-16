use std::fs;
use std::path::Path;

/// dsh 桌面运行模式，与 packages/contracts 的 DesktopMode 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopMode {
    Compatibility,
    Advanced,
}

impl DesktopMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compatibility => "compatibility",
            Self::Advanced => "advanced",
        }
    }
}

/// 读取 `$DSH_HOME/settings.yaml` 的 `dsh-desktop.mode`；非法值回退 compatibility。
pub fn read_desktop_mode(home: &Path) -> DesktopMode {
    if cfg!(target_os = "linux") {
        return DesktopMode::Compatibility;
    }
    let Ok(raw) = fs::read_to_string(home.join("settings.yaml")) else {
        return DesktopMode::Compatibility;
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
        return DesktopMode::Compatibility;
    };
    match value
        .get("dsh-desktop")
        .and_then(|section| section.get("mode"))
        .and_then(|mode| mode.as_str())
    {
        Some("advanced") => DesktopMode::Advanced,
        _ => DesktopMode::Compatibility,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dsh-mode-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn reads_advanced_mode_from_settings() {
        let home = temp_home("advanced");
        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join("settings.yaml"),
            "dsh-desktop:\n  mode: advanced\n",
        )
        .unwrap();
        assert_eq!(read_desktop_mode(&home), DesktopMode::Advanced);
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn falls_back_to_compatibility_for_missing_or_invalid() {
        let home = temp_home("invalid");
        fs::create_dir_all(&home).unwrap();
        assert_eq!(read_desktop_mode(&home), DesktopMode::Compatibility);
        fs::write(
            home.join("settings.yaml"),
            "dsh-desktop:\n  mode: unknown\n",
        )
        .unwrap();
        assert_eq!(read_desktop_mode(&home), DesktopMode::Compatibility);
        fs::write(home.join("settings.yaml"), "not: [yaml").unwrap();
        assert_eq!(read_desktop_mode(&home), DesktopMode::Compatibility);
        fs::remove_dir_all(&home).ok();
    }
}
