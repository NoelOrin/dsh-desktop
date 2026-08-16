//! dsh web 页面注入：负责把受控桥接、侧边栏右键菜单与开发热更新脚本注入
//! 当前桌面壳托管的 dsh 页面。

/// 判断是否为当前桌面壳托管的 dsh origin（用于桥接注入）。
pub fn is_managed_dsh_url(url: &tauri::Url, managed: Option<&tauri::Url>) -> bool {
    managed.is_some_and(|managed| managed.origin() == url.origin())
}

/// 判断是否同时满足托管 origin 与一次性 host token 校验。
fn is_managed_dsh_url_with_token(
    url: &tauri::Url,
    managed: Option<&tauri::Url>,
    host_token: Option<&str>,
) -> bool {
    if !is_managed_dsh_url(url, managed) {
        return false;
    }
    let Some(expected) = host_token else {
        return false;
    };
    url.query_pairs()
        .any(|(key, value)| key == super::HOST_TOKEN_QUERY_KEY && value == expected)
}

/// 向 dsh web 页面注入壳能力。脚本常量定义在 `lib.rs`，由模块统一消费，
/// 避免桥接暴露、注入判定与页面生命周期散落在不同位置。
pub fn inject_dsh_web<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    url: &tauri::Url,
    managed_dsh_url: &std::sync::Mutex<Option<tauri::Url>>,
    host_token: &std::sync::Mutex<Option<String>>,
) {
    let managed = managed_dsh_url.lock().ok().and_then(|guard| guard.clone());
    let host_token = host_token.lock().ok().and_then(|guard| guard.clone());
    if !is_managed_dsh_url_with_token(url, managed.as_ref(), host_token.as_deref()) {
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
    fn accepts_managed_origin_with_matching_host_token() {
        let managed = tauri::Url::parse("http://127.0.0.1:49321").unwrap();
        let url =
            tauri::Url::parse("http://127.0.0.1:49321/chat?dsh_desktop_token=abc123&theme=dark")
                .unwrap();
        assert!(is_managed_dsh_url_with_token(
            &url,
            Some(&managed),
            Some("abc123")
        ));
    }

    #[test]
    fn rejects_wrong_or_missing_host_token() {
        let managed = tauri::Url::parse("http://127.0.0.1:49321").unwrap();
        let wrong =
            tauri::Url::parse("http://127.0.0.1:49321/chat?dsh_desktop_token=wrong").unwrap();
        assert!(!is_managed_dsh_url_with_token(
            &wrong,
            Some(&managed),
            Some("abc123")
        ));

        let missing = tauri::Url::parse("http://127.0.0.1:49321/chat").unwrap();
        assert!(!is_managed_dsh_url_with_token(
            &missing,
            Some(&managed),
            Some("abc123")
        ));
        assert!(!is_managed_dsh_url_with_token(&wrong, Some(&managed), None));
    }

    #[test]
    fn rejects_when_no_managed_url_exists() {
        let url =
            tauri::Url::parse("http://127.0.0.1:49321/chat?dsh_desktop_token=abc123").unwrap();
        assert!(!is_managed_dsh_url_with_token(&url, None, Some("abc123")));
    }

    #[test]
    fn only_accepts_managed_dsh_origin() {
        let managed = tauri::Url::parse("http://127.0.0.1:49321").unwrap();
        assert!(is_managed_dsh_url(
            &tauri::Url::parse("http://127.0.0.1:49321/chat").unwrap(),
            Some(&managed)
        ));
        assert!(!is_managed_dsh_url(
            &tauri::Url::parse("http://127.0.0.1:49322").unwrap(),
            Some(&managed)
        ));
        assert!(!is_managed_dsh_url(
            &tauri::Url::parse("http://localhost:49321").unwrap(),
            Some(&managed)
        ));
        assert!(!is_managed_dsh_url(
            &tauri::Url::parse("https://example.com").unwrap(),
            Some(&managed)
        ));
        assert!(!is_managed_dsh_url(
            &tauri::Url::parse("http://127.0.0.1:49321").unwrap(),
            None
        ));
    }
}
