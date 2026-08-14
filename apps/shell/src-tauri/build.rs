fn main() {
    // 为应用自定义命令生成 allow-* / deny-* ACL 权限，
    // 供 capabilities 声明（含 remote 白名单下的桥接命令）。
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "get_status",
            "restart",
            "install_dsh",
            "open_log_directory",
            "get_config",
            "set_config",
            "open_external",
            "get_autostart",
            "set_autostart",
        ]));
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("{error}");
        std::process::exit(1);
    }
}
