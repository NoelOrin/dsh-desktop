use serde::Serialize;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationAction {
    pub kind: String,
    pub session_id: Option<String>,
    pub url: Option<String>,
    pub path: Option<String>,
}

pub fn show(app: &AppHandle, title: &str, body: &str, action: Option<NotificationAction>) {
    let app = app.clone();
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("DSH Desktop")
        .summary(title)
        .body(body);
    #[cfg(target_os = "windows")]
    notification.app_id(&app.package_info().identifier);
    if action.is_some() {
        notification.action("default", "打开");
    }
    let Ok(handle) = notification.show() else {
        return;
    };
    thread::spawn(move || {
        let _ = handle.wait_for_response(move |response: &notify_rust::NotificationResponse| {
            let clicked = matches!(
                response,
                notify_rust::NotificationResponse::Default
                    | notify_rust::NotificationResponse::Action(_)
            );
            if !clicked {
                return;
            }
            if let Some(action) = action {
                let _ = app.emit("dsh-notification-action", action);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
    });
}
