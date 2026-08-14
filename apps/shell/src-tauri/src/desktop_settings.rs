use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopSection {
    pub autostart: Option<bool>,
    pub startup_mode: Option<String>,
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

pub fn startup_mode(section: &DesktopSection) -> &'static str {
    match section.startup_mode.as_deref() {
        Some("tray") => "tray",
        Some("minimized") => "minimized",
        _ => "normal",
    }
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
        assert_eq!(startup_mode(&section), "tray");
    }

    #[test]
    fn defaults_to_normal() {
        assert_eq!(startup_mode(&DesktopSection::default()), "normal");
    }
}
