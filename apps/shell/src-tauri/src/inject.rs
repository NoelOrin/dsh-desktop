//! dsh web 页面注入：负责把受控桥接、侧边栏右键菜单与开发热更新脚本注入 loopback 页面。

/// 判断是否为 dsh web 的 loopback 页面（用于桥接注入）。
pub fn is_dsh_web_url(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "::1" | "[::1]" | "0:0:0:0:0:0:0:1" | "[0:0:0:0:0:0:0:1]")
    )
}

/// 向 dsh web 页面注入壳能力。脚本常量定义在 `lib.rs`，由模块统一消费，
/// 避免桥接暴露、注入判定与页面生命周期散落在不同位置。
pub fn inject_dsh_web<R: tauri::Runtime>(webview: &tauri::Webview<R>, url: &tauri::Url) {
    if !is_dsh_web_url(url) {
        return;
    }
    let _ = webview.eval(super::BRIDGE_SCRIPT);
    let _ = webview.eval(super::HARNESS_CHROME_SCRIPT);
    #[cfg(debug_assertions)]
    {
        let _ = webview.eval(super::DEV_RELOAD_SCRIPT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_loopback_hosts() {
        assert!(is_dsh_web_url(
            &tauri::Url::parse("http://127.0.0.1:49321").unwrap()
        ));
        assert!(is_dsh_web_url(
            &tauri::Url::parse("http://localhost:49321").unwrap()
        ));
        assert!(is_dsh_web_url(
            &tauri::Url::parse("http://[::1]:49321").unwrap()
        ));
        assert!(!is_dsh_web_url(
            &tauri::Url::parse("https://example.com").unwrap()
        ));
    }
}
