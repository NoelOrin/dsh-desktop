use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopSection {
    pub autostart: Option<bool>,
    pub startup_mode: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartupMode {
    #[default]
    Normal,
    Tray,
    Minimized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DesktopSettings {
    pub autostart: bool,
    pub startup_mode: StartupMode,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DesktopSettingsFile {
    startup_mode: Option<StartupMode>,
}

pub fn read_desktop_section(settings_path: &Path) -> DesktopSection {
    let Ok(raw) = std::fs::read_to_string(settings_path) else {
        return DesktopSection::default();
    };
    let doc: Result<serde_yaml::Value, _> = serde_yaml::from_str(&raw);
    let Ok(doc) = doc else {
        return DesktopSection::default();
    };
    let Some(section) = doc.get("desktop") else {
        return DesktopSection::default();
    };
    serde_yaml::from_value(section.clone()).unwrap_or_default()
}

pub fn startup_mode(section: &DesktopSection) -> StartupMode {
    match section.startup_mode.as_deref() {
        Some("tray") => StartupMode::Tray,
        Some("minimized") => StartupMode::Minimized,
        _ => StartupMode::Normal,
    }
}

pub fn read_startup_mode(path: &Path) -> Option<StartupMode> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return None;
    };
    let Ok(file) = serde_json::from_str::<DesktopSettingsFile>(&raw) else {
        return None;
    };
    file.startup_mode
}

pub fn save_desktop_settings(path: &Path, settings: &DesktopSettings) -> Result<(), String> {
    let file = DesktopSettingsFile {
        startup_mode: Some(settings.startup_mode),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_section() {
        let raw = "desktop:\n  autostart: true\n  startupMode: tray\n";
        let path = std::env::temp_dir().join("dsh-desktop-settings-test.yaml");
        std::fs::write(&path, raw).unwrap();
        let section = read_desktop_section(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(section.autostart, Some(true));
        assert_eq!(startup_mode(&section), StartupMode::Tray);
    }

    #[test]
    fn defaults_to_normal() {
        assert_eq!(
            startup_mode(&DesktopSection::default()),
            StartupMode::Normal
        );
    }

    #[test]
    fn startup_mode_roundtrip() {
        let path = std::env::temp_dir().join("dsh-desktop-settings-test.json");
        let settings = DesktopSettings {
            autostart: true,
            startup_mode: StartupMode::Minimized,
        };
        save_desktop_settings(&path, &settings).unwrap();
        assert_eq!(read_startup_mode(&path), Some(StartupMode::Minimized));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_startup_file_defaults_to_none() {
        let path = std::env::temp_dir().join("dsh-desktop-settings-missing.json");
        std::fs::remove_file(&path).ok();
        assert_eq!(read_startup_mode(&path), None);
    }
}
