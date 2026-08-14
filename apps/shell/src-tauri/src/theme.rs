//! 壳侧主题：读取 settings.yaml 的 ui-theme 分节，解析少量 token 供启动页/窗口背景。
//! 移植自参考仓库 src/shared/themes.js（家族种子 + mixHex + resolveMode）。

use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_FAMILY_ID: &str = "deepseek";

#[derive(Clone, Copy)]
struct Seeds {
    accent: &'static str,
    background: &'static str,
    foreground: &'static str,
}

struct Family {
    id: &'static str,
    light: Seeds,
    dark: Seeds,
}

const fn seeds(accent: &'static str, background: &'static str, foreground: &'static str) -> Seeds {
    Seeds {
        accent,
        background,
        foreground,
    }
}

const FAMILIES: &[Family] = &[
    Family {
        id: "deepseek",
        light: seeds("#4176e6", "#ffffff", "#0f1115"),
        dark: seeds("#6ea8ff", "#151517", "#f5f5f5"),
    },
    Family {
        id: "midnight",
        light: seeds("#3b6fd4", "#f3f6fb", "#1a1f2b"),
        dark: seeds("#6ea8ff", "#0b0d12", "#e8eef9"),
    },
    Family {
        id: "celadon",
        light: seeds("#0f766e", "#f3faf7", "#10211c"),
        dark: seeds("#3dd6b5", "#071411", "#e7f6f1"),
    },
    Family {
        id: "violet",
        light: seeds("#7c3aed", "#f7f3fc", "#1c1524"),
        dark: seeds("#c4a1ff", "#120e18", "#f3eefc"),
    },
    Family {
        id: "amber",
        light: seeds("#b45309", "#fbf6ee", "#1c1915"),
        dark: seeds("#e2b15c", "#14100b", "#f6efe4"),
    },
    Family {
        id: "paper",
        light: seeds("#0f766e", "#f3efe6", "#1c1915"),
        dark: seeds("#5eead4", "#1a1712", "#f6efe4"),
    },
    Family {
        id: "contrast",
        light: seeds("#111111", "#ffffff", "#050505"),
        dark: seeds("#ffffff", "#050505", "#f5f5f5"),
    },
];

/// settings.yaml 的 ui-theme 分节（camelCase 键；缺失字段回退默认）。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiThemeSection {
    pub preference: Option<String>,
    pub active_light_theme_id: Option<String>,
    pub active_dark_theme_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UiThemeTokens {
    pub bg: String,
    pub fg: String,
    pub muted: String,
    pub accent: String,
    pub field: String,
    pub line: String,
    pub button_fg: String,
    pub scheme: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UiThemeSnapshot {
    pub tokens: UiThemeTokens,
    pub preference: String,
    pub mode: String,
}

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let value = hex.trim_start_matches('#');
    let read_channel = |range: std::ops::Range<usize>| {
        value
            .get(range)
            .and_then(|part| u8::from_str_radix(part, 16).ok())
            .unwrap_or(0)
    };
    (read_channel(0..2), read_channel(2..4), read_channel(4..6))
}

fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// 在 left 与 right 之间按 amount(0..=1) 插值。
fn mix_hex(left: &str, right: &str, amount: f64) -> String {
    let (lr, lg, lb) = parse_hex(left);
    let (rr, rg, rb) = parse_hex(right);
    let t = amount.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    to_hex(channel(lr, rr), channel(lg, rg), channel(lb, rb))
}

fn resolve_mode(preference: &str, system_dark: bool) -> &'static str {
    match preference {
        "dark" => "dark",
        "light" => "light",
        _ => {
            if system_dark {
                "dark"
            } else {
                "light"
            }
        }
    }
}

fn find_family(id: &str) -> &'static Family {
    FAMILIES
        .iter()
        .find(|family| family.id == id)
        .unwrap_or(&FAMILIES[0])
}

fn tokens_for(section: &UiThemeSection, mode: &str, _system_dark: bool) -> UiThemeTokens {
    let family_id = if mode == "dark" {
        section
            .active_dark_theme_id
            .as_deref()
            .unwrap_or(DEFAULT_FAMILY_ID)
    } else {
        section
            .active_light_theme_id
            .as_deref()
            .unwrap_or(DEFAULT_FAMILY_ID)
    };
    let family = find_family(family_id);
    let seeds = if mode == "dark" {
        &family.dark
    } else {
        &family.light
    };
    let scheme = if mode == "dark" { "dark" } else { "light" };
    let bg = seeds.background;
    let fg = seeds.foreground;
    let muted = mix_hex(fg, bg, 0.42);
    let field = mix_hex(bg, fg, 0.06);
    let line = if scheme == "light" {
        "rgba(15, 17, 21, 0.12)"
    } else {
        "rgba(245, 245, 245, 0.10)"
    };
    let button_fg = if scheme == "light" {
        mix_hex(bg, "#000000", 0.08)
    } else {
        mix_hex(bg, "#000000", 0.0)
    };
    UiThemeTokens {
        bg: bg.to_string(),
        fg: fg.to_string(),
        muted,
        accent: seeds.accent.to_string(),
        field,
        line: line.to_string(),
        button_fg,
        scheme: scheme.to_string(),
    }
}

/// 解析 settings.yaml 中的 ui-theme 分节；文件缺失/非法时返回默认分节。
pub fn read_ui_theme_section(settings_path: &Path) -> UiThemeSection {
    let Ok(raw) = std::fs::read_to_string(settings_path) else {
        return UiThemeSection::default();
    };
    let doc: Result<serde_yaml::Value, _> = serde_yaml::from_str(&raw);
    let Ok(doc) = doc else {
        return UiThemeSection::default();
    };
    let Some(section) = doc.get("ui-theme") else {
        return UiThemeSection::default();
    };
    serde_yaml::from_value(section.clone()).unwrap_or_default()
}

/// 组装启动页/窗口背景要用的快照。
pub fn resolve_ui_theme(section: &UiThemeSection, system_dark: bool) -> UiThemeSnapshot {
    let mode = resolve_mode(
        section.preference.as_deref().unwrap_or("system"),
        system_dark,
    );
    let preference = section
        .preference
        .clone()
        .unwrap_or_else(|| "system".to_string());
    UiThemeSnapshot {
        tokens: tokens_for(section, mode, system_dark),
        preference,
        mode: mode.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mode_resolves_system() {
        assert_eq!(resolve_mode("system", true), "dark");
        assert_eq!(resolve_mode("system", false), "light");
        assert_eq!(resolve_mode("light", true), "light");
        assert_eq!(resolve_mode("dark", false), "dark");
    }

    #[test]
    fn missing_section_falls_back_to_deepseek_dark() {
        let section = UiThemeSection::default();
        let snapshot = resolve_ui_theme(&section, true);
        assert_eq!(snapshot.mode, "dark");
        assert_eq!(snapshot.tokens.scheme, "dark");
        assert_eq!(snapshot.tokens.bg, "#151517");
    }

    #[test]
    fn parses_camel_case_section() {
        let raw =
            "ui-theme:\n  preference: light\n  activeLightThemeId: midnight\n  activeDarkThemeId: midnight\n";
        let path = std::env::temp_dir().join("dsh-desktop-theme-test.yaml");
        std::fs::write(&path, raw).unwrap();
        let section = read_ui_theme_section(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(section.preference.as_deref(), Some("light"));
        assert_eq!(section.active_light_theme_id.as_deref(), Some("midnight"));
        assert_eq!(section.active_dark_theme_id.as_deref(), Some("midnight"));
        let snapshot = resolve_ui_theme(&section, true);
        assert_eq!(snapshot.tokens.bg, "#f3f6fb");
    }

    #[test]
    fn mix_hex_interpolates() {
        assert_eq!(mix_hex("#000000", "#ffffff", 1.0), "#ffffff");
        assert_eq!(mix_hex("#000000", "#ffffff", 0.0), "#000000");
    }
}
