//! 内嵌插件装配：把 Tauri resources 中打包的 dsh 插件复制进 dsh 的 profile node_modules，
//! 并生成 --patch overlay 文件，使 dsh web 启动时自动挂载这些插件。
//! 全程 best-effort：任何失败只记日志，绝不阻塞 dsh 启动。

use std::fs;
use std::path::{Path, PathBuf};

/// 已装配的内嵌插件（挂载信息，用于生成 overlay）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedPlugin {
    pub id: String,
    pub name: String,
}

/// 遍历 `resource_dir/plugins/*` 子目录，把每个含 `dshDesktop.id` 的插件包
/// 装配到 `home/profiles/node_modules/<name>/`（dsh 的模块兜底目录）。
/// 以版本号为键：目标缺失或版本不同才重装（幂等、自愈）。单包失败只记日志跳过；
/// plugins 根目录缺失直接返回空。返回装配成功的插件列表。
pub fn assemble(resource_dir: &Path, home: &Path, log: &mut dyn FnMut(&str)) -> Vec<MountedPlugin> {
    assemble_inner(resource_dir, home, log, false)
}

/// 开发模式装配：忽略版本号，每次启动都重装内嵌插件，确保源码改动立即生效。
pub fn assemble_dev(
    resource_dir: &Path,
    home: &Path,
    log: &mut dyn FnMut(&str),
) -> Vec<MountedPlugin> {
    assemble_inner(resource_dir, home, log, true)
}

/// 装配内嵌插件并生成 `--patch` overlay。
///
/// 开发模式使用 `assemble_dev` 强制重装，发布模式使用版本键控的 `assemble`；
/// 任一阶段失败都不影响 dsh 启动，仅向日志写入原因。
pub fn prepare_overlay(
    resource_dir: Option<&Path>,
    home: Option<&Path>,
    app_data_dir: Option<&Path>,
    dev: bool,
    log: &mut dyn FnMut(&str),
) -> Option<PathBuf> {
    let mounted = match (resource_dir, home) {
        (Some(resource_dir), Some(home)) if dev => assemble_dev(resource_dir, home, log),
        (Some(resource_dir), Some(home)) => assemble(resource_dir, home, log),
        _ => Vec::new(),
    };
    app_data_dir.and_then(|dir| write_overlay(dir, &mounted))
}

fn assemble_inner(
    resource_dir: &Path,
    home: &Path,
    log: &mut dyn FnMut(&str),
    force: bool,
) -> Vec<MountedPlugin> {
    let plugins_root = resource_dir.join("plugins");
    let entries = match fs::read_dir(&plugins_root) {
        Ok(entries) => entries,
        Err(error) => {
            log(&format!(
                "[desktop] 未找到内嵌插件目录 {}（{error}），跳过装配",
                plugins_root.display()
            ));
            return Vec::new();
        }
    };

    let mut mounted = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                log(&format!("[desktop] 读取内嵌插件目录出错（跳过）：{error}"));
                continue;
            }
        };
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir = entry.path();

        let manifest = match read_manifest(&dir) {
            Ok(manifest) => manifest,
            Err(message) => {
                log(&message);
                continue;
            }
        };
        let Some(id) = manifest
            .get("dshDesktop")
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_str())
        else {
            log(&format!(
                "[desktop] 跳过插件 {}：package.json 缺少 dshDesktop.id",
                dir.display()
            ));
            continue;
        };
        let Some(name) = manifest.get("name").and_then(|v| v.as_str()) else {
            log(&format!(
                "[desktop] 跳过插件 {}：package.json 缺少 name",
                dir.display()
            ));
            continue;
        };
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let target = home.join("profiles").join("node_modules").join(name);

        if !force && target_has_version(&target, version) {
            log(&format!(
                "[desktop] 插件 {name}@{version} 已装配（版本相同），跳过复制"
            ));
            mounted.push(MountedPlugin {
                id: id.to_string(),
                name: name.to_string(),
            });
            continue;
        }

        // 原子重装：先拷到 <name>.tmp，再删旧目标，rename 上位
        let tmp = PathBuf::from(format!("{}.tmp", target.display()));
        let _ = fs::remove_dir_all(&tmp);
        if let Err(error) = copy_dir_recursive(&dir, &tmp) {
            log(&format!(
                "[desktop] 装配插件 {name} 失败（复制到临时目录）：{error}"
            ));
            let _ = fs::remove_dir_all(&tmp);
            continue;
        }
        if target.exists() {
            if let Err(error) = fs::remove_dir_all(&target) {
                log(&format!(
                    "[desktop] 装配插件 {name} 失败（清理旧目标）：{error}"
                ));
                let _ = fs::remove_dir_all(&tmp);
                continue;
            }
        }
        if let Err(error) = fs::rename(&tmp, &target) {
            log(&format!(
                "[desktop] 装配插件 {name} 失败（重命名）：{error}"
            ));
            let _ = fs::remove_dir_all(&tmp);
            continue;
        }

        log(&format!(
            "[desktop] 已装配插件 {name}@{version} → {}",
            target.display()
        ));
        mounted.push(MountedPlugin {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    mounted
}

/// 返回内嵌插件的 resources 根目录。
///
/// 发布模式下 Tauri 会把 `resources/**` 按相对路径复制到应用资源目录下的
/// `resources/` 子目录，因此这里返回 `resource_dir/resources`，最终读取
/// `resource_dir/resources/plugins`。开发模式下 Tauri 的 `resource_dir()` 指向
/// `target/<profile>`，不会包含源码里的 `resources`，因此显式回退到
/// `CARGO_MANIFEST_DIR/resources`，保证 `yarn dev` 也能装配内嵌插件。
pub fn plugins_resource_dir(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))
    } else {
        resource_dir.map(|dir| dir.join("resources"))
    }
}

/// 读取插件目录的 package.json 并解析为 JSON 值；失败时返回中文日志消息。
fn read_manifest(dir: &Path) -> Result<serde_json::Value, String> {
    let path = dir.join("package.json");
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "[desktop] 跳过插件 {}：读取 package.json 失败（{error}）",
            dir.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "[desktop] 跳过插件 {}：package.json 解析失败（{error}）",
            dir.display()
        )
    })
}

/// 目标目录的 package.json 版本是否与源版本一致（一致则无需重装）。
fn target_has_version(target: &Path, version: &str) -> bool {
    let Ok(text) = fs::read_to_string(target.join("package.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    manifest.get("version").and_then(|v| v.as_str()) == Some(version)
}

/// 递归复制目录（std::fs 手写，不引入 fs_extra）。
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// 把已装配插件列表写成 `dir/embedded-plugins.patch.yml` overlay 文件；
/// 空列表返回 None（不生成）。插件名必须单引号包裹（@ 是 YAML 1.1 保留指示符）。
pub fn write_overlay(dir: &Path, mounted: &[MountedPlugin]) -> Option<PathBuf> {
    if mounted.is_empty() {
        return None;
    }
    let mut content = String::new();
    for plugin in mounted {
        content.push_str(&format!(
            "- insert:\n    - id: {}\n      name: '{}'\n",
            plugin.id, plugin.name
        ));
    }
    let path = dir.join("embedded-plugins.patch.yml");
    if let Err(error) = fs::write(&path, content) {
        eprintln!("[desktop] 写入 overlay {} 失败：{error}", path.display());
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn write_overlay_quotes_names() {
        let dir = temp_dir().join(format!("dsh-overlay-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mounted = vec![
            MountedPlugin {
                id: "bridge".into(),
                name: "@dsh-desktop/plugin-bridge".into(),
            },
            MountedPlugin {
                id: "projects".into(),
                name: "@dsh-desktop/plugin-projects".into(),
            },
        ];
        let path = write_overlay(&dir, &mounted).expect("overlay 应生成");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content
            .contains("- insert:\n    - id: bridge\n      name: '@dsh-desktop/plugin-bridge'\n"));
        assert!(content.contains(
            "- insert:\n    - id: projects\n      name: '@dsh-desktop/plugin-projects'\n"
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_overlay_none_when_empty() {
        assert!(write_overlay(&temp_dir(), &[]).is_none());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn dev_plugins_resource_dir_points_to_source_resources() {
        let path = plugins_resource_dir(None).expect("debug 下应返回源码 resources");
        assert_eq!(path.file_name().unwrap(), "resources");
        assert!(path.join("plugins").is_dir());
    }

    #[test]
    fn assemble_copies_when_missing_and_version_differs() {
        // 构造：resources/plugins/bridge/{package.json, cordis.patch.yml, lib/index.js}
        // 目标 home/profiles/node_modules/@dsh-desktop/plugin-bridge
        let root = temp_dir().join(format!("dsh-assemble-test-{}", std::process::id()));
        let resources = root.join("resources");
        let plugins = resources.join("plugins");
        let home = root.join("home");

        // bridge 插件：name/version/dshDesktop.id 齐全
        let bridge = plugins.join("bridge");
        std::fs::create_dir_all(bridge.join("lib")).unwrap();
        std::fs::write(bridge.join("lib/index.js"), "// v1").unwrap();
        std::fs::write(bridge.join("cordis.patch.yml"), "").unwrap();
        std::fs::write(
            bridge.join("package.json"),
            r#"{"name":"@dsh-desktop/plugin-bridge","version":"0.1.0","dshDesktop":{"id":"bridge"}}"#,
        )
        .unwrap();

        // 无 dshDesktop.id 的目录 → 跳过（4）
        let no_id = plugins.join("no-id");
        std::fs::create_dir_all(&no_id).unwrap();
        std::fs::write(
            no_id.join("package.json"),
            r#"{"name":"@dsh-desktop/no-id","version":"1.0.0"}"#,
        )
        .unwrap();

        // 1) 缺失 → 复制，返回 mounted 含 id/name
        let mut logs = Vec::new();
        let mounted = assemble(&resources, &home, &mut |line| logs.push(line.to_string()));
        assert_eq!(mounted.len(), 1);
        assert_eq!(mounted[0].id, "bridge");
        assert_eq!(mounted[0].name, "@dsh-desktop/plugin-bridge");
        let target = home
            .join("profiles")
            .join("node_modules")
            .join("@dsh-desktop/plugin-bridge");
        assert_eq!(
            std::fs::read_to_string(target.join("lib/index.js")).unwrap(),
            "// v1"
        );
        assert!(target.join("cordis.patch.yml").exists());
        // no-id 未被复制
        assert!(!home
            .join("profiles")
            .join("node_modules")
            .join("@dsh-desktop/no-id")
            .exists());

        // 2) 同版本 → 不再复制（改源文件内容后目标不变）
        std::fs::write(bridge.join("lib/index.js"), "// v1-modified").unwrap();
        let mounted2 = assemble(&resources, &home, &mut |_| {});
        assert_eq!(mounted2.len(), 1);
        assert_eq!(
            std::fs::read_to_string(target.join("lib/index.js")).unwrap(),
            "// v1"
        );

        // 3) 版本不同 → 重新复制
        std::fs::write(
            bridge.join("package.json"),
            r#"{"name":"@dsh-desktop/plugin-bridge","version":"0.2.0","dshDesktop":{"id":"bridge"}}"#,
        )
        .unwrap();
        let mounted3 = assemble(&resources, &home, &mut |_| {});
        assert_eq!(mounted3.len(), 1);
        assert_eq!(
            std::fs::read_to_string(target.join("lib/index.js")).unwrap(),
            "// v1-modified"
        );
        let target_pkg = std::fs::read_to_string(target.join("package.json")).unwrap();
        assert!(target_pkg.contains("\"version\":\"0.2.0\""));

        // 5) resources/plugins 不存在 → 空 Vec，不 panic
        assert!(assemble(&root.join("missing"), &home, &mut |_| {}).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn assemble_dev_refreshes_same_version() {
        let root = temp_dir().join(format!("dsh-assemble-dev-test-{}", std::process::id()));
        let resources = root.join("resources");
        let plugins = resources.join("plugins");
        let home = root.join("home");
        let bridge = plugins.join("bridge");
        std::fs::create_dir_all(bridge.join("lib")).unwrap();
        std::fs::write(
            bridge.join("package.json"),
            r#"{"name":"@dsh-desktop/plugin-bridge","version":"0.1.0","dshDesktop":{"id":"bridge"}}"#,
        )
        .unwrap();
        std::fs::write(bridge.join("lib/index.js"), "// v1").unwrap();

        let target = home
            .join("profiles")
            .join("node_modules")
            .join("@dsh-desktop/plugin-bridge");
        assemble_dev(&resources, &home, &mut |_| {});
        assert_eq!(
            std::fs::read_to_string(target.join("lib/index.js")).unwrap(),
            "// v1"
        );

        // 版本不变时，开发模式仍应重装，避免旧产物继续被加载。
        std::fs::write(bridge.join("lib/index.js"), "// v1-updated").unwrap();
        assemble_dev(&resources, &home, &mut |_| {});
        assert_eq!(
            std::fs::read_to_string(target.join("lib/index.js")).unwrap(),
            "// v1-updated"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prepare_overlay_writes_overlay_when_plugin_mounted() {
        let root = temp_dir().join(format!("dsh-overlay-prepare-test-{}", std::process::id()));
        let resources = root.join("resources");
        let plugins = resources.join("plugins");
        let home = root.join("home");
        let app_data = root.join("app-data");
        std::fs::create_dir_all(&app_data).unwrap();
        let bridge = plugins.join("bridge");
        std::fs::create_dir_all(bridge.join("lib")).unwrap();
        std::fs::write(
            bridge.join("package.json"),
            r#"{"name":"@dsh-desktop/plugin-bridge","version":"0.1.0","dshDesktop":{"id":"bridge"}}"#,
        )
        .unwrap();

        let overlay = prepare_overlay(
            Some(&resources),
            Some(&home),
            Some(&app_data),
            false,
            &mut |_| {},
        )
        .expect("overlay 应生成");
        let content = std::fs::read_to_string(&overlay).unwrap();
        assert!(content.contains("name: '@dsh-desktop/plugin-bridge'"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
