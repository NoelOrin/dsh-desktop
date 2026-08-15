use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

mod config;
mod desktop_settings;
mod embedded;
mod host_lifecycle;
mod inject;
mod notifications;
mod process;
mod profiles;
mod projects;
mod theme;

use config::DshConfig;
use host_lifecycle::{HostLifecycle, RestartDecision};
use theme::{read_ui_theme_section, resolve_ui_theme, UiThemeSnapshot};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, State, WebviewUrl, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, GlobalShortcutExt, Shortcut, ShortcutState,
};
use tauri_plugin_notification::NotificationExt;

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_LOGS: usize = 500;
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// 子进程意外退出后的自动重启上限（看门狗，超过则转 failed）。
const MAX_AUTO_RESTARTS: u32 = host_lifecycle::DEFAULT_AUTO_RESTART_LIMIT;
/// 优雅退出：SIGTERM 后等待子进程退出的宽限期。
const GRACE_PERIOD: Duration = Duration::from_secs(2);
/// pending_deeplinks 最大保留条数，超限时淘汰最旧 payload。
const MAX_PENDING_DEEPLINKS: usize = 128;
/// 未消费深链 payload 的保留时间，超过后不再补发。
const PENDING_DEEPLINK_TTL: Duration = Duration::from_secs(5 * 60);
/// GitHub Release 元数据请求的总超时。
const UPDATE_API_TIMEOUT: Duration = Duration::from_secs(30);
/// 更新安装包下载请求的总超时。
const UPDATE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

static LOG_LOCK: Mutex<()> = Mutex::new(());
static SHELL_PATH_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// 托盘菜单项 id
const TRAY_STATUS: &str = "tray-status";
const TRAY_COPY_URL: &str = "tray-copy-url";
const TRAY_OPEN_BROWSER: &str = "tray-open-browser";
const TRAY_STOP_DSH: &str = "tray-stop-dsh";
const TRAY_RESTART_DSH: &str = "tray-restart-dsh";
const TRAY_SHOW_MAIN: &str = "tray-show-main";
const TRAY_QUIT: &str = "tray-quit";

/// dsh web 注入校验使用的随机 token 查询参数名。
const HOST_TOKEN_QUERY_KEY: &str = "dsh_desktop_token";

/// 允许 main 窗口导航到的本地 origin：Tauri 本地页面与固定开发服务器。
fn is_shell_url(url: &tauri::Url) -> bool {
    if url.scheme() == "tauri" && url.host_str() == Some("localhost") {
        return true;
    }
    if url.scheme() != "http" {
        return false;
    }
    ["http://localhost:5173", "http://127.0.0.1:5173"]
        .iter()
        .any(|candidate| {
            tauri::Url::parse(candidate)
                .map(|candidate| candidate.origin() == url.origin())
                .unwrap_or(false)
        })
}

/// dsh 页面仅允许当前桌面壳托管的 origin，避免任意 loopback 服务复用桥接能力。
fn is_managed_dsh_url(url: &tauri::Url, managed: Option<&tauri::Url>) -> bool {
    managed.is_some_and(|managed| managed.origin() == url.origin())
}

/// 生成一次运行使用的 dsh web 注入 token（24 bytes hex）。
fn generate_host_token() -> String {
    let mut bytes = [0u8; 24];
    getrandom::getrandom(&mut bytes).expect("生成 dsh web 注入 token 失败");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 构造 dsh 启动参数：以 --profile <active> 开头，不使用 "web" 别名。
/// overlay 非 None 时追加 --patch <path>。
fn dsh_web_args(active: &str, overlay: Option<&Path>) -> Vec<String> {
    let mut args = vec!["--profile".to_string(), active.to_string()];
    if let Some(overlay_path) = overlay {
        args.push("--patch".to_string());
        args.push(overlay_path.to_string_lossy().into_owned());
    }
    args.extend([
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        "0".to_string(),
    ]);
    args
}

fn is_external_url(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// 动态创建 main 窗口，使导航/新窗口策略在首次加载前就生效。
fn create_main_window<R: tauri::Runtime>(
    app: &tauri::App<R>,
    managed_dsh_url: &Arc<Mutex<Option<tauri::Url>>>,
) -> tauri::Result<()> {
    let navigation_managed_url = managed_dsh_url.clone();
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("DSH Desktop")
            .inner_size(1280.0, 860.0)
            .min_inner_size(480.0, 600.0)
            .center()
            .resizable(true)
            .visible(true)
            .background_color(tauri::window::Color(245, 246, 250, 255))
            .on_navigation(move |url| {
                if is_shell_url(url) {
                    return true;
                }
                if is_managed_dsh_url(url, navigation_managed_url.lock().unwrap().as_ref()) {
                    return true;
                }
                if is_external_url(url) {
                    let _ = open_with_system(url.as_str());
                }
                false
            })
            .on_new_window(|url, _features| {
                if is_external_url(&url) {
                    let _ = open_with_system(url.as_str());
                }
                NewWindowResponse::Deny
            });

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .decorations(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.decorations(false);
    }

    builder.build()?;
    Ok(())
}

/// 注入到 dsh web（loopback 远程页面）的桥接脚本，定义 window.__DSH_DESKTOP__。
/// 仅暴露最小能力切片（见 capabilities/bridge.json 与 shortcuts.json 的 remote 白名单）。
const BRIDGE_SCRIPT: &str = r#"(function () {
  "use strict";
  if (window.__DSH_DESKTOP__) return;
  var internals = window.__TAURI_INTERNALS__;
  if (!internals) return;
  function invoke(cmd, args) {
    return internals.invoke(cmd, args || {});
  }
  function listen(event, cb) {
    return internals.invoke("plugin:event|listen", {
      event: event,
      target: { kind: "Any" },
      handler: internals.transformCallback(function (e) { cb(e.payload); }),
    }).then(function (eventId) {
      return function () {
        return internals.invoke("plugin:event|unlisten", {
          event: event,
          eventId: eventId,
        });
      };
    });
  }
  window.__DSH_DESKTOP__ = {
    openExternal: function (target) { return invoke("open_external", { target: target }); },
    windowAction: function (action) { return invoke("window_action", { action: action }); },
    onWindowState: function (cb) { return listen("dsh-window-state", cb); },
    getStatus: function () { return invoke("get_status"); },
    restart: function () { return invoke("restart"); },
    installDsh: function () { return invoke("install_dsh"); },
    openLogDirectory: function () { return invoke("open_log_directory"); },
    getConfig: function () { return invoke("get_config"); },
    setConfig: function (config) { return invoke("set_config", { config: config }); },
    onStatus: function (cb) { return listen("dsh-status", cb); },
    onLog: function (cb) { return listen("dsh-log", cb); },
    autostart: {
      get: function () { return invoke("get_autostart"); },
      set: function (enabled) { return invoke("set_autostart", { enabled: enabled }); },
    },
    desktop: {
      get: function () { return invoke("get_desktop_settings"); },
      set: function (settings) { return invoke("set_desktop_settings", { settings: settings }); },
    },
    profiles: {
      list: function () { return invoke("get_profiles"); },
      active: function () { return invoke("get_active_profile"); },
      select: function (name) { return invoke("select_profile", { name: name }); },
    },
    shortcuts: {
      register: function (s, cb) {
        return invoke("register_shortcut", { shortcut: s }).then(function () {
          if (typeof cb !== "function") return function () {};
          return listen("dsh-shortcut", function (e) { if (e.payload === s) cb(); });
        });
      },
      unregister: function (s) { return invoke("unregister_shortcut", { shortcut: s }); },
      list: function () { return invoke("get_shortcuts"); },
      unregisterAll: function () { return invoke("unregister_all_shortcuts"); },
    },
    onShortcut: function (cb) { return listen("dsh-shortcut", cb); },
    update: {
      check: function () { return invoke("check_update"); },
      install: function () { return invoke("install_update"); },
    },
  };
})();"#;

/// 注入 dsh web 的侧边栏右键菜单脚本；标题栏形态已由 bridge client 提供。
const HARNESS_CHROME_SCRIPT: &str = r##"(function () {
  "use strict";

  function findSidebarRoot() {
    // 侧栏根节点用稳定的 data-slot 定位，避免依赖 CSS module 哈希 class。
    var slot = document.querySelector('[data-slot="sidebar"]');
    var root = slot && slot.firstElementChild;
    return root instanceof HTMLElement ? root : null;
  }

  // ── 侧边栏右键菜单 ─────────────────────────────────────────────
  // 数据与动作经 @dsh-desktop/plugin-projects 的 loopback 端点提供，
  // 菜单本体由本脚本渲染；定位只依赖稳定 data-slot / aria-label，不依赖哈希 class。
  var SIDEBAR_MENU_ID = "dsh-shell-context-menu";
  var SIDEBAR_TOAST_ID = "dsh-shell-context-toast";
  var SIDEBAR_MENU_STYLE_ID = "dsh-shell-context-style";
  var sidebarWorkspacesCache = null;
  var sidebarWorkspacesCacheAt = 0;
  var sidebarSessionsCache = null;
  var sidebarSessionsCacheAt = 0;
  var sidebarMenuDismiss = null;

  function sidebarMenuStyleText() {
    return [
      "#" + SIDEBAR_MENU_ID + " {",
      "  position: fixed;",
      "  z-index: 2147483646;",
      "  min-width: 200px;",
      "  padding: 4px;",
      "  border: 1px solid var(--dsw-alias-border-l3, rgba(0,0,0,0.12));",
      "  border-radius: 8px;",
      "  background: var(--dsw-alias-bg-layer-2, #ffffff);",
      "  color: var(--dsw-alias-label-primary, #18181b);",
      "  box-shadow: 0 8px 24px rgba(0,0,0,0.16);",
      "  font-size: 13px;",
      "  line-height: 18px;",
      "}",
      "#" + SIDEBAR_MENU_ID + " button {",
      "  display: block;",
      "  width: 100%;",
      "  box-sizing: border-box;",
      "  padding: 6px 10px;",
      "  border: 0;",
      "  border-radius: 6px;",
      "  background: transparent;",
      "  color: inherit;",
      "  font: inherit;",
      "  text-align: left;",
      "  cursor: pointer;",
      "}",
      "#" + SIDEBAR_MENU_ID + " button:hover {",
      "  background: var(--dsw-alias-interactive-bg-hover, rgba(0,0,0,0.06));",
      "}",
      "#" + SIDEBAR_MENU_ID + " button.danger { color: var(--dsw-alias-state-error-primary, #d92d20); }",
      "#" + SIDEBAR_MENU_ID + " .sep { height: 1px; margin: 4px 6px; background: var(--dsw-alias-border-l1, rgba(0,0,0,0.08)); }",
      "#" + SIDEBAR_MENU_ID + " .hint { padding: 6px 10px; color: var(--dsw-alias-label-tertiary, #71717a); }",
      "#" + SIDEBAR_MENU_ID + " .edit { display: flex; flex-direction: column; gap: 8px; padding: 8px 10px; }",
      "#" + SIDEBAR_MENU_ID + " .edit input {",
      "  box-sizing: border-box;",
      "  width: 100%;",
      "  padding: 5px 8px;",
      "  border: 1px solid var(--dsw-alias-border-l2, rgba(0,0,0,0.16));",
      "  border-radius: 6px;",
      "  background: var(--dsw-alias-bg-base, #ffffff);",
      "  color: inherit;",
      "  font: inherit;",
      "}",
      "#" + SIDEBAR_MENU_ID + " .edit .row { display: flex; gap: 8px; }",
      "#" + SIDEBAR_MENU_ID + " .edit .row button { flex: 1; text-align: center; }",
      "#" + SIDEBAR_TOAST_ID + " {",
      "  position: fixed;",
      "  top: 12px;",
      "  left: 50%;",
      "  transform: translateX(-50%);",
      "  z-index: 2147483646;",
      "  padding: 8px 14px;",
      "  border-radius: 8px;",
      "  background: var(--dsw-alias-bg-layer-2, #18181b);",
      "  color: var(--dsw-alias-label-primary, #ffffff);",
      "  box-shadow: 0 6px 18px rgba(0,0,0,0.18);",
      "  font-size: 13px;",
      "  line-height: 18px;",
      "  opacity: 0;",
      "  transition: opacity 0.18s ease;",
      "  pointer-events: none;",
      "}"
    ].join("\n");
  }

  function ensureSidebarMenuStyle() {
    var style = document.getElementById(SIDEBAR_MENU_STYLE_ID);
    if (style) return;
    style = document.createElement("style");
    style.id = SIDEBAR_MENU_STYLE_ID;
    style.textContent = sidebarMenuStyleText();
    (document.head || document.documentElement).appendChild(style);
  }

  function sidebarToast(text) {
    var toast = document.getElementById(SIDEBAR_TOAST_ID);
    if (!toast) {
      toast = document.createElement("div");
      toast.id = SIDEBAR_TOAST_ID;
      (document.body || document.documentElement).appendChild(toast);
    }
    toast.textContent = text;
    requestAnimationFrame(function () {
      toast.style.opacity = "1";
    });
    window.setTimeout(function () {
      toast.style.opacity = "0";
    }, 2400);
  }

  function hideSidebarMenu() {
    var menu = document.getElementById(SIDEBAR_MENU_ID);
    if (menu) menu.remove();
    if (sidebarMenuDismiss) {
      sidebarMenuDismiss();
      sidebarMenuDismiss = null;
    }
  }

  function bindSidebarMenuDismiss() {
    var onPointerDown = function (event) {
      var menu = document.getElementById(SIDEBAR_MENU_ID);
      if (menu && menu.contains(event.target)) return;
      hideSidebarMenu();
    };
    var onKeyDown = function (event) {
      if (event.key === "Escape") hideSidebarMenu();
    };
    var onViewportChange = function () {
      hideSidebarMenu();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onViewportChange, true);
    window.addEventListener("resize", onViewportChange);
    return function () {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onViewportChange, true);
      window.removeEventListener("resize", onViewportChange);
    };
  }

  function loadSidebarWorkspaces() {
    var now = Date.now();
    if (sidebarWorkspacesCache && now - sidebarWorkspacesCacheAt < 5000) {
      return Promise.resolve(sidebarWorkspacesCache);
    }
    return fetch("/dsh-desktop/workspaces", { headers: { accept: "application/json" } })
      .then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      })
      .then(function (data) {
        var list = Array.isArray(data && data.workspaces) ? data.workspaces : [];
        sidebarWorkspacesCache = list;
        sidebarWorkspacesCacheAt = Date.now();
        return list;
      });
  }

  function loadSidebarSessions() {
    var now = Date.now();
    if (sidebarSessionsCache && now - sidebarSessionsCacheAt < 5000) {
      return Promise.resolve(sidebarSessionsCache);
    }
    return fetch("/dsh-desktop/sessions", { headers: { accept: "application/json" } })
      .then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      })
      .then(function (data) {
        var list = Array.isArray(data && data.sessions) ? data.sessions : [];
        sidebarSessionsCache = list;
        sidebarSessionsCacheAt = Date.now();
        return list;
      });
  }

  function invalidateSidebarWorkspaces() {
    sidebarWorkspacesCache = null;
    sidebarWorkspacesCacheAt = 0;
    sidebarSessionsCache = null;
    sidebarSessionsCacheAt = 0;
  }

  function postWorkspaceAction(id, action, extra) {
    var payload = { id: id, action: action };
    if (extra) {
      for (var key in extra) payload[key] = extra[key];
    }
    return fetch("/dsh-desktop/workspaces/action", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    }).then(function (response) {
      return response.json();
    });
  }

  function postSidebarAction(action, extra) {
    var payload = { action: action };
    if (extra) {
      for (var key in extra) payload[key] = extra[key];
    }
    return fetch("/dsh-desktop/workspaces/action", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    }).then(function (response) {
      return response.json();
    });
  }

  function findSidebarWorkspaceRow(target) {
    if (!target || typeof target.closest !== "function") return null;
    var row = target.closest('[data-slot="sidebar.workspaces"] [role="treeitem"][aria-expanded]');
    return row instanceof HTMLElement ? row : null;
  }

  function sidebarActionName(label, kind) {
    if (typeof label !== "string") return null;
    if (kind === "workspace") {
      var match = label.match(/^工作区“(.+)”的操作$/);
      if (match) return match[1];
      match = label.match(/^Workspace actions for (.+)$/);
      return match ? match[1] : null;
    }
    var match = label.match(/^会话“(.+)”的操作$/);
    if (match) return match[1];
    match = label.match(/^Session actions for (.+)$/);
    return match ? match[1] : null;
  }

  function findSidebarWorkspaceName(target) {
    var row = findSidebarWorkspaceRow(target);
    if (!row) return null;
    var node = row;
    while (node && node !== document.body && node !== document.documentElement) {
      var buttons = node.querySelectorAll && node.querySelectorAll('[role="treeitem"] button[aria-label^="工作区"], [role="treeitem"] button[aria-label^="Workspace"]');
      if (buttons) {
        for (var i = 0; i < buttons.length; i++) {
          var name = sidebarActionName(buttons[i].getAttribute("aria-label"), "workspace");
          if (name) return name;
        }
      }
      node = node.parentElement;
    }
    return null;
  }

  function findSidebarSession(target) {
    if (!target || typeof target.closest !== "function") return null;
    var row = target.closest('[data-slot="sidebar.workspaces"] [role="treeitem"][aria-selected]');
    if (!(row instanceof HTMLElement)) return null;
    var button = row.querySelector && row.querySelector('button[aria-label^="会话"], button[aria-label^="Session"]');
    if (!(button instanceof HTMLElement)) return null;
    var aria = button.getAttribute("aria-label");
    var title = sidebarActionName(aria, "session");
    var workspaceName = null;
    var node = row.parentElement;
    while (node && node !== document.body && node !== document.documentElement) {
      for (var i = 0; i < node.children.length; i++) {
        var child = node.children[i];
        if (child instanceof HTMLElement && child.getAttribute("role") === "treeitem" && child.getAttribute("aria-expanded") !== null) {
          workspaceName = findSidebarWorkspaceName(child);
          break;
        }
      }
      if (workspaceName) break;
      node = node.parentElement;
    }
    return {
      row: row,
      button: button,
      title: title,
      workspaceName: workspaceName
    };
  }

  function runSidebarSessionAction(session, action, menu) {
    postSidebarAction(action, { session_id: session.id })
      .then(function (result) {
        if (!result || !result.ok) {
          sidebarToast((result && result.error) || "操作失败");
          return;
        }
        if (action === "session-archive") {
          sidebarToast("聊天已归档");
        }
        invalidateSidebarWorkspaces();
        hideSidebarMenu();
      })
      .catch(function (error) {
        sidebarToast("操作失败: " + error.message);
        hideSidebarMenu();
      });
  }

  function buildSidebarSessionMenu(session, x, y) {
    var menu = document.createElement("div");
    menu.id = SIDEBAR_MENU_ID;
    var items = [];
    items.push({ id: "session-archive", label: "归档聊天" });
    for (var i = 0; i < items.length; i++) {
      var item = items[i];
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = item.label;
      button.addEventListener("click", function (entry) {
        return function () {
          runSidebarSessionAction(session, entry.id, menu);
        };
      }(item));
      menu.appendChild(button);
    }
    (document.body || document.documentElement).appendChild(menu);
    positionSidebarMenu(menu, x, y);
  }

  function openSidebarSessionMenu(session, x, y) {
    hideSidebarMenu();
    ensureSidebarMenuStyle();
    loadSidebarSessions()
      .then(function (sessions) {
        var found = null;
        for (var i = 0; i < sessions.length; i++) {
          if (sessions[i].title !== session.title) continue;
          if (session.workspaceName && sessions[i].workspace_name !== session.workspaceName) continue;
          found = sessions[i];
          break;
        }
        if (!found) {
          sidebarToast("未找到会话“" + session.title + "”");
          return;
        }
        buildSidebarSessionMenu(found, x, y);
      })
      .catch(function (error) {
        sidebarToast("会话菜单不可用: " + error.message);
      });
  }

  function runSidebarMenuAction(workspace, action, menu) {
    if (action === "edit") {
      renderSidebarMenuEdit(workspace, menu);
      return;
    }
    if (action === "remove") {
      renderSidebarMenuRemove(workspace, menu);
      return;
    }
    postWorkspaceAction(workspace.id, action)
      .then(function (result) {
        if (!result || !result.ok) {
          sidebarToast((result && result.error) || "操作失败");
          return;
        }
        if (action === "pin") {
          sidebarToast("已置顶");
        } else if (action === "unpin") {
          sidebarToast("已取消置顶");
        } else if (action === "finder") {
          if (window.__DSH_DESKTOP__ && window.__DSH_DESKTOP__.openExternal && result.path) {
            window.__DSH_DESKTOP__.openExternal(result.path).catch(function (error) {
              sidebarToast("打开目录失败: " + error);
            });
          } else {
            sidebarToast("桌面桥接不可用");
          }
        } else if (action === "worktree") {
          sidebarToast("已创建永久工作树: " + result.target);
        } else if (action === "archive") {
          sidebarToast("聊天已归档");
        }
        invalidateSidebarWorkspaces();
        hideSidebarMenu();
      })
      .catch(function (error) {
        sidebarToast("操作失败: " + error.message);
        hideSidebarMenu();
      });
  }

  function renderSidebarMenuEdit(workspace, menu) {
    menu.textContent = "";
    var box = document.createElement("div");
    box.className = "edit";
    var input = document.createElement("input");
    input.type = "text";
    input.value = workspace.name;
    input.setAttribute("aria-label", "项目名称");
    var row = document.createElement("div");
    row.className = "row";
    var save = document.createElement("button");
    save.type = "button";
    save.textContent = "保存";
    save.addEventListener("click", function () {
      var name = input.value.trim();
      if (!name) {
        input.focus();
        return;
      }
      postWorkspaceAction(workspace.id, "edit", { name: name })
        .then(function (result) {
          if (!result || !result.ok) {
            sidebarToast((result && result.error) || "保存失败");
            return;
          }
          sidebarToast("已保存");
          invalidateSidebarWorkspaces();
          hideSidebarMenu();
        })
        .catch(function (error) {
          sidebarToast("保存失败: " + error.message);
        });
    });
    var cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "取消";
    cancel.addEventListener("click", hideSidebarMenu);
    row.appendChild(save);
    row.appendChild(cancel);
    box.appendChild(input);
    box.appendChild(row);
    menu.appendChild(box);
    input.focus();
    input.select();
  }

  function renderSidebarMenuRemove(workspace, menu) {
    menu.textContent = "";
    var hint = document.createElement("div");
    hint.className = "hint";
    hint.textContent = "确认移除“" + workspace.name + "”？仅移出侧边栏，不删除磁盘目录。";
    var row = document.createElement("div");
    row.className = "edit row";
    var remove = document.createElement("button");
    remove.type = "button";
    remove.className = "danger";
    remove.textContent = "移除";
    remove.addEventListener("click", function () {
      postWorkspaceAction(workspace.id, "remove")
        .then(function (result) {
          if (!result || !result.ok) {
            sidebarToast((result && result.error) || "移除失败");
            return;
          }
          sidebarToast("已移除");
          invalidateSidebarWorkspaces();
          hideSidebarMenu();
        })
        .catch(function (error) {
          sidebarToast("移除失败: " + error.message);
        });
    });
    var cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "取消";
    cancel.addEventListener("click", hideSidebarMenu);
    row.appendChild(remove);
    row.appendChild(cancel);
    menu.appendChild(hint);
    menu.appendChild(row);
  }

  function positionSidebarMenu(menu, x, y) {
    var MARGIN = 8;
    var rect = menu.getBoundingClientRect();
    var left = Math.min(Math.max(x, MARGIN), window.innerWidth - rect.width - MARGIN);
    var top = Math.min(Math.max(y, MARGIN), window.innerHeight - rect.height - MARGIN);
    menu.style.left = Math.max(left, 0) + "px";
    menu.style.top = Math.max(top, 0) + "px";
    sidebarMenuDismiss = bindSidebarMenuDismiss();
  }

  function buildSidebarMenu(workspace, x, y) {
    var menu = document.createElement("div");
    menu.id = SIDEBAR_MENU_ID;
    var items = [
      { id: workspace.pinned ? "unpin" : "pin", label: workspace.pinned ? "取消置顶" : "置顶项目" },
      { id: "finder", label: "在 Finder 中显示" },
      { id: "worktree", label: "创建永久工作树" },
      { id: "edit", label: "编辑项目" },
      { type: "sep" },
      { id: "archive", label: "归档聊天" },
      { id: "remove", label: "移除本地项目", danger: true }
    ];
    for (var i = 0; i < items.length; i++) {
      var item = items[i];
      if (item.type === "sep") {
        var sep = document.createElement("div");
        sep.className = "sep";
        menu.appendChild(sep);
        continue;
      }
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = item.label;
      if (item.danger) button.className = "danger";
      button.addEventListener("click", function (entry) {
        return function () {
          runSidebarMenuAction(workspace, entry.id, menu);
        };
      }(item));
      menu.appendChild(button);
    }
    (document.body || document.documentElement).appendChild(menu);
    positionSidebarMenu(menu, x, y);
  }

  function openSidebarMenu(name, x, y) {
    ensureSidebarMenuStyle();
    loadSidebarWorkspaces()
      .then(function (workspaces) {
        var workspace = null;
        for (var i = 0; i < workspaces.length; i++) {
          if (workspaces[i].name === name) {
            workspace = workspaces[i];
            break;
          }
        }
        if (!workspace) {
          sidebarToast("未找到工作区“" + name + "”");
          return;
        }
        buildSidebarMenu(workspace, x, y);
      })
      .catch(function (error) {
        sidebarToast("侧边栏菜单不可用: " + error.message);
      });
  }

  // 侧边栏区域（会话树 / 工作区行）：禁用系统原生右键菜单。
  // 工作区行与会话行都打开壳侧自定义菜单。
  function installSidebarContextMenu() {
    document.addEventListener("contextmenu", function (event) {
      var target = event.target;
      var inSidebar = false;
      var root = findSidebarRoot();
      if (root && root.contains(target)) {
        inSidebar = true;
      } else if (target && typeof target.closest === "function" && target.closest('[data-slot^="sidebar"]')) {
        inSidebar = true;
      }
      if (!inSidebar) return;
      event.preventDefault();
      var session = findSidebarSession(target);
      if (session) {
        openSidebarSessionMenu(session, event.clientX, event.clientY);
        return;
      }
      var name = findSidebarWorkspaceName(target);
      if (name) openSidebarMenu(name, event.clientX, event.clientY);
    }, true);
  }

  function install() {
    // 标题栏与窗口控制已迁移到 @dsh-desktop/plugin-bridge 的 client 面；
    // 壳侧仅保留侧边栏右键菜单，避免与 bridge 注入的桌面形态重复。
    installSidebarContextMenu();
  }

  install();
})();"##;

#[cfg(debug_assertions)]
/// 开发热更新：dsh-client-hmr 广播 rebuilt 帧后自动整页刷新，
/// 作为模块热替换的页面级兜底，避免个别 client 状态未能随 HMR 更新。
const DEV_RELOAD_SCRIPT: &str = r##"(function () {
  "use strict";
  if (window.__DSH_DEV_RELOAD__) return;
  window.__DSH_DEV_RELOAD__ = true;

  var RELOAD_DELAY_MS = 800;
  var source = null;

  function scheduleReload() {
    if (!source) return;
    source.close();
    source = null;
    setTimeout(function () {
      window.location.reload();
    }, RELOAD_DELAY_MS);
  }

  try {
    source = new EventSource("/plugins/events");
  } catch (_) {
    return;
  }

  source.addEventListener("message", function (event) {
    var frame;
    try {
      frame = JSON.parse(event.data);
    } catch (_) {
      return;
    }
    if (frame && frame.type === "rebuilt") {
      scheduleReload();
    }
  });
})();"##;

#[derive(Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RuntimePhase {
    #[default]
    Detecting,
    #[serde(rename = "missing")]
    MissingDsh,
    Installing,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Clone, Serialize)]
struct RuntimeSnapshot {
    phase: RuntimePhase,
    message: String,
    url: Option<String>,
    dsh_installed: bool,
    node_found: bool,
    dsh_version: Option<String>,
    log_dir: Option<String>,
    logs: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DeepLinkPayload {
    id: String,
    url: String,
    raw: String,
    received_at: String,
    source: String,
    args: Vec<String>,
    cwd: String,
}

#[derive(Default)]
struct Inner {
    phase: RuntimePhase,
    message: String,
    url: Option<String>,
    dsh_installed: bool,
    node_found: bool,
    dsh_version: Option<String>,
    log_dir: Option<PathBuf>,
    logs: VecDeque<String>,
    /// 尚未被 dsh web 消费的深链 payload，由 bridge 主动查询并确认。
    pending_deeplinks: VecDeque<DeepLinkPayload>,
    next_deep_link_id: u64,
}

impl Inner {
    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            phase: self.phase.clone(),
            message: self.message.clone(),
            url: self.url.clone(),
            dsh_installed: self.dsh_installed,
            node_found: self.node_found,
            dsh_version: self.dsh_version.clone(),
            log_dir: self
                .log_dir
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            logs: self.logs.iter().rev().take(200).cloned().collect(),
        }
    }

    fn prune_expired_pending_deeplinks(&mut self) {
        let now_ms = unix_millis();
        self.pending_deeplinks.retain(|payload| {
            payload
                .received_at
                .parse::<u64>()
                .map(|received_at| {
                    now_ms.saturating_sub(received_at) < PENDING_DEEPLINK_TTL.as_millis() as u64
                })
                .unwrap_or(true)
        });
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn same_launch_payload(left: &DeepLinkPayload, right: &DeepLinkPayload) -> bool {
    left.source == right.source
        && left.url == right.url
        && left.raw == right.raw
        && left.args == right.args
        && left.cwd == right.cwd
}

fn enqueue_pending_deeplink(inner: &mut Inner, payload: &DeepLinkPayload) -> bool {
    inner.prune_expired_pending_deeplinks();
    if inner
        .pending_deeplinks
        .iter()
        .any(|existing| same_launch_payload(existing, payload))
    {
        return false;
    }
    inner.pending_deeplinks.push_back(payload.clone());
    while inner.pending_deeplinks.len() > MAX_PENDING_DEEPLINKS {
        inner.pending_deeplinks.pop_front();
    }
    true
}

struct AppState {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    tx: Sender<ManagerMessage>,
    log_dir: PathBuf,
    config_path: PathBuf,
    desktop_settings_path: PathBuf,
    projects_path: PathBuf,
    profile_state_path: PathBuf,
    /// 应用是否正在退出（托盘"退出"置 true，用于关闭到托盘时区分真正退出）。
    exiting: Arc<AtomicBool>,
    /// --autostart + settings startupMode=tray 时隐藏主窗口，直到用户从托盘唤起。
    start_in_tray: Arc<AtomicBool>,
    /// 已注册的自定义全局快捷键注册表（快捷键字符串 → Shortcut），供注销时查表。
    shortcuts: Arc<Mutex<HashMap<String, Shortcut>>>,
}

fn enqueue_launch_payload(
    app: &AppHandle,
    source: String,
    url: String,
    raw: String,
    args: Vec<String>,
    cwd: String,
) {
    let state = app.state::<AppState>();
    let (payload, enqueued) = {
        let mut inner = state.inner.lock().unwrap();
        inner.next_deep_link_id += 1;
        let id = format!("dl-{}", inner.next_deep_link_id);
        let received_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default();
        let payload = DeepLinkPayload {
            id,
            url: url.clone(),
            raw,
            received_at,
            source,
            args,
            cwd,
        };
        let enqueued = enqueue_pending_deeplink(&mut inner, &payload);
        (payload, enqueued)
    };
    let ready = matches!(state.inner.lock().unwrap().phase, RuntimePhase::Ready);
    if ready && enqueued {
        let _ = app.emit("dsh-deeplink", payload);
    }
}

enum ManagerMessage {
    Start,
    Stop,
    Shutdown,
    Ready { generation: u64, url: String },
    ReadyTimeout { generation: u64 },
    ReadinessError { generation: u64, error: String },
    InstallFinished { result: Result<(), String> },
    Unhealthy { generation: u64 },
    Healthy { generation: u64 },
}

struct DshManager {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    managed_dsh_url: Arc<Mutex<Option<tauri::Url>>>,
    host_token: Arc<Mutex<Option<String>>>,
    tx: Sender<ManagerMessage>,
    rx: Receiver<ManagerMessage>,
    child: Option<Child>,
    lifecycle: HostLifecycle,
    failed_generation: Option<u64>,
    log_path: PathBuf,
    config_path: PathBuf,
    profile_state_path: PathBuf,
    startup_context: Option<profiles::StartupContext>,
    start_in_tray: Arc<AtomicBool>,
}

pub fn run() {
    let autostart_requested = std::env::args().any(|arg| arg == "--autostart");
    // 全局快捷键插件：预置快捷键（CmdOrCtrl+Shift+C 打开控制中心）已随控制中心移除；
    // 自定义快捷键由 dsh 插件经 register_shortcut / unregister_shortcut 桥接命令注册。
    let shortcut_plugin = ShortcutBuilder::new().build();

    let exiting = Arc::new(AtomicBool::new(false));
    let managed_dsh_url = Arc::new(Mutex::new(None));
    let host_token = Arc::new(Mutex::new(Some(generate_host_token())));

    tauri::Builder::default()
        // 单实例锁必须最先注册（插件按注册顺序执行）
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            if app.try_state::<AppState>().is_some() {
                if let Some(url) = args.iter().find(|arg| arg.starts_with("dsh-desktop://")) {
                    enqueue_launch_payload(
                        app,
                        "deep_link".into(),
                        url.clone(),
                        url.clone(),
                        args.clone(),
                        cwd.clone(),
                    );
                } else {
                    enqueue_launch_payload(
                        app,
                        "second_instance".into(),
                        String::new(),
                        String::new(),
                        args.clone(),
                        cwd.clone(),
                    );
                }
            }
        }))
        .plugin(shortcut_plugin)
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        // 窗口状态记忆：重启后恢复 main 窗口大小/位置/最大化
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        // 深链 dsh-desktop://（macOS 经 RunEvent::Opened；Windows/Linux 由 single-instance 转发）
        .plugin(tauri_plugin_deep_link::init())
        // 开机自启（默认关闭，由 dsh 插件设置面板控制）
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        // 向 dsh web（loopback 远程页面）注入受控桥接 window.__DSH_DESKTOP__
        .on_page_load({
            let managed_dsh_url = managed_dsh_url.clone();
            let host_token = host_token.clone();
            move |webview, payload| {
                if payload.event() == PageLoadEvent::Finished {
                    inject::inject_dsh_web(webview, payload.url(), &managed_dsh_url, &host_token);
                }
            }
        })
        .setup(move |app| {
            create_main_window(app, &managed_dsh_url)?;

            let app_handle = app.handle().clone();
            let app_data = app_handle.path().app_data_dir()?;
            let log_dir = app_data.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            let log_path = log_dir.join("dsh.log");
            let config_path = app_data.join("config.json");
            let desktop_settings_path = app_data.join("desktop-settings.json");
            let projects_path = app_data.join("projects.json");
            let profile_state_path = app_data.join("profile-state.json");

            let inner = Arc::new(Mutex::new(Inner {
                log_dir: Some(log_dir.clone()),
                ..Inner::default()
            }));

            let (tx, rx) = mpsc::channel();

            let launch_settings_home = settings_home(&config_path);
            let startup_mode = desktop_settings::read_startup_mode(&desktop_settings_path)
                .unwrap_or_else(|| {
                    launch_settings_home
                        .as_deref()
                        .map(|home| home.join("settings.yaml"))
                        .map(|path| {
                            desktop_settings::startup_mode(&desktop_settings::read_desktop_section(
                                &path,
                            ))
                        })
                        .unwrap_or(desktop_settings::StartupMode::Normal)
                });
            let start_in_tray = Arc::new(AtomicBool::new(
                autostart_requested && startup_mode == desktop_settings::StartupMode::Tray,
            ));
            if autostart_requested && startup_mode == desktop_settings::StartupMode::Minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.minimize();
                }
            }
            if start_in_tray.load(Ordering::Relaxed) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            let manager = DshManager {
                app: app_handle.clone(),
                inner: inner.clone(),
                tx: tx.clone(),
                rx,
                child: None,
                lifecycle: HostLifecycle::new(MAX_AUTO_RESTARTS),
                failed_generation: None,
                log_path,
                config_path: config_path.clone(),
                profile_state_path: app_data.join("profile-state.json"),
                startup_context: None,
                start_in_tray: start_in_tray.clone(),
                managed_dsh_url: managed_dsh_url.clone(),
                host_token: host_token.clone(),
            };
            thread::spawn(move || manager.run());

            let theme_app = app_handle.clone();
            let config_path_for_theme = config_path.clone();
            app.manage(AppState {
                app: app_handle,
                inner: inner.clone(),
                tx: tx.clone(),
                log_dir,
                config_path,
                desktop_settings_path,
                projects_path,
                profile_state_path,
                exiting: exiting.clone(),
                start_in_tray: start_in_tray.clone(),
                shortcuts: Arc::new(Mutex::new(HashMap::new())),
            });

            // 重启后恢复上次持久化的全局快捷键（注册冲突仅跳过，不阻塞启动）
            if let Some(state) = app.try_state::<AppState>() {
                let config = config::load(&state.config_path);
                for shortcut in config.shortcuts {
                    let _ = register_shortcut_internal(&state, shortcut);
                }
            }

            // 深链 dsh-desktop://：统一入队，就绪时转发给 dsh web，由 bridge 查询并确认
            let deep_app = app.handle().clone();
            let deep_link_app = deep_app.clone();
            deep_link_app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let url = url.to_string();
                    enqueue_launch_payload(
                        &deep_app,
                        "deep_link".into(),
                        url.clone(),
                        url,
                        Vec::new(),
                        String::new(),
                    );
                }
            });

            let start_tx = tx.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                let _ = start_tx.send(ManagerMessage::Start);
            });

            // 主题跟随：轮询 settings.yaml 的 ui-theme 分节，变化时发 dsh-ui-theme 并更新窗口背景
            thread::spawn(move || {
                let mut last: Option<String> = None;
                loop {
                    thread::sleep(Duration::from_secs(2));
                    let Some(home) = settings_home(&config_path_for_theme) else {
                        continue;
                    };
                    let settings_path = home.join("settings.yaml");
                    let system_dark = theme_app
                        .get_webview_window("main")
                        .and_then(|w| w.theme().ok())
                        .map(|t| t == tauri::Theme::Dark)
                        .unwrap_or(false);
                    let section = read_ui_theme_section(&settings_path);
                    let snapshot = resolve_ui_theme(&section, system_dark);
                    let key = format!(
                        "{}:{}:{}",
                        snapshot.preference, snapshot.mode, snapshot.tokens.bg
                    );
                    if last.as_deref() == Some(key.as_str()) {
                        continue;
                    }
                    last = Some(key);
                    let _ = theme_app.emit("dsh-ui-theme", &snapshot);
                    if let Some(window) = theme_app.get_webview_window("main") {
                        if let Some(color) = parse_window_color(&snapshot.tokens.bg) {
                            let _ = window.set_background_color(Some(color));
                        }
                    }
                }
            });

            let tray_state = setup_tray(app.handle(), exiting.clone())?;
            app.manage(tray_state);

            // 自动更新：后台检查 GitHub Release，发现新版本 emit dsh-update-available（payload 为新版本号），失败仅记日志
            let updater_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match latest_release_tag().await {
                    Ok(tag) => {
                        let is_new = parse_release_tag(&tag)
                            .map(|v| v > updater_app.package_info().version)
                            .unwrap_or(false);
                        if is_new {
                            let _ = updater_app.emit("dsh-update-available", tag);
                        }
                    }
                    Err(e) => {
                        eprintln!("update check failed: {e}");
                    }
                }
            });

            emit_window_state(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_pending_deeplinks,
            ack_deeplink,
            restart,
            install_dsh,
            update_dsh,
            open_log_directory,
            get_config,
            set_config,
            open_external,
            get_autostart,
            set_autostart,
            get_desktop_settings,
            set_desktop_settings,
            get_projects,
            add_project,
            update_project,
            remove_project,
            set_project_pinned,
            mark_project_read,
            archive_project_chats,
            create_project_worktree,
            show_project_in_finder,
            get_profiles,
            get_active_profile,
            select_profile,
            register_shortcut,
            unregister_shortcut,
            get_shortcuts,
            unregister_all_shortcuts,
            check_update,
            install_update,
            request_notification_permission,
            get_ui_theme,
            window_action,
            open_paths,
            import_paths
        ])
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            // 文件拖放：把真实路径通过 dsh-file-drop 事件转给前端（dsh web 经桥接订阅）
            if label == "main" {
                if let WindowEvent::Resized(_) = event {
                    emit_window_state(window.app_handle());
                    return;
                }
                if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop {
                    paths, position, ..
                }) = event
                {
                    let payload = FileDropPayload {
                        id: format!("drop-{}", NEXT_DROP_ID.fetch_add(1, Ordering::Relaxed)),
                        paths: paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect(),
                        kind: process::classify_drop(paths).to_string(),
                        position: DropPosition {
                            x: position.x,
                            y: position.y,
                        },
                        action: "open".to_string(),
                    };
                    let _ = window.emit("dsh-file-drop", payload);
                    return;
                }
                // 系统主题变化：透传给前端（启动页监听 dsh-theme）
                if let WindowEvent::ThemeChanged(theme) = event {
                    let theme = match theme {
                        tauri::Theme::Dark => "dark",
                        _ => "light",
                    };
                    let _ = window.emit("dsh-theme", theme);
                    return;
                }
            }
            // main 窗口：关闭到托盘（非真正退出时仅隐藏）
            if label == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let exiting = window.app_handle().state::<AppState>().exiting.clone();
                    if exiting.load(Ordering::Relaxed) {
                        return; // 真正退出，放行关闭
                    }
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building DSH Desktop")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                // Command+W 关闭到托盘后，点击 Dock 图标重新唤起主窗口。
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                return;
            }

            if let RunEvent::Exit = event {
                let _ = app.state::<AppState>().tx.send(ManagerMessage::Shutdown);
            }
        });
}

/// 创建系统托盘：常驻后台，菜单按运行阶段动态更新状态与可用性。
fn setup_tray(app: &tauri::AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<TrayState> {
    let status = MenuItem::with_id(app, TRAY_STATUS, "DSH: 检测中", false, None::<&str>)?;
    let copy_url = MenuItem::with_id(app, TRAY_COPY_URL, "复制 Web UI 地址", false, None::<&str>)?;
    let open_browser =
        MenuItem::with_id(app, TRAY_OPEN_BROWSER, "用浏览器打开", false, None::<&str>)?;
    let stop_dsh = MenuItem::with_id(app, TRAY_STOP_DSH, "停止 dsh", false, None::<&str>)?;
    let restart_dsh = MenuItem::with_id(app, TRAY_RESTART_DSH, "重启 dsh", false, None::<&str>)?;
    let show_main = MenuItem::with_id(app, TRAY_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT, "退出", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &sep1,
            &copy_url,
            &open_browser,
            &stop_dsh,
            &restart_dsh,
            &sep2,
            &show_main,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).ok())
        .expect("缺少托盘图标");

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("DSH Desktop")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TRAY_COPY_URL => {
                let state = app.state::<AppState>();
                let snapshot = state.inner.lock().unwrap().snapshot();
                if let Some(url) = snapshot.url {
                    let _ = app.clipboard().write_text(url);
                }
            }
            TRAY_OPEN_BROWSER => {
                let state = app.state::<AppState>();
                let snapshot = state.inner.lock().unwrap().snapshot();
                if let Some(url) = snapshot.url {
                    let _ = open_with_system(&url);
                }
            }
            TRAY_STOP_DSH => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Stop);
            }
            TRAY_RESTART_DSH => {
                let state = app.state::<AppState>();
                let _ = state.tx.send(ManagerMessage::Start);
            }
            TRAY_SHOW_MAIN => {
                let state = app.state::<AppState>();
                state.start_in_tray.store(false, Ordering::Relaxed);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            TRAY_QUIT => {
                let app = app.clone();
                let exiting = exiting.clone();
                app.dialog()
                    .message("退出后将停止当前 dsh 会话，确认退出？")
                    .title("退出 DSH Desktop")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                    .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
                        "退出".into(),
                        "取消".into(),
                    ))
                    .show(move |confirmed| {
                        if !confirmed {
                            return;
                        }
                        exiting.store(true, Ordering::Relaxed);
                        let state = app.state::<AppState>();
                        let _ = state.tx.send(ManagerMessage::Stop);
                        thread::sleep(Duration::from_millis(300));
                        app.exit(0);
                    });
            }
            _ => {}
        })
        .build(app)?;

    Ok(TrayState {
        tray,
        status,
        copy_url,
        open_browser,
        stop_dsh,
        restart_dsh,
    })
}

/// 托盘句柄与菜单项需要保活，供状态更新时修改文案与可用性。
struct TrayState {
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    status: tauri::menu::MenuItem<tauri::Wry>,
    copy_url: tauri::menu::MenuItem<tauri::Wry>,
    open_browser: tauri::menu::MenuItem<tauri::Wry>,
    stop_dsh: tauri::menu::MenuItem<tauri::Wry>,
    restart_dsh: tauri::menu::MenuItem<tauri::Wry>,
}

impl DshManager {
    fn run(mut self) {
        loop {
            match self.rx.recv_timeout(POLL_INTERVAL) {
                Ok(ManagerMessage::Start) => {
                    self.lifecycle.reset_restarts();
                    self.handle_start();
                }
                Ok(ManagerMessage::Stop) => {
                    self.cleanup_child();
                    self.set_phase(RuntimePhase::Stopped, "已停止".to_string(), None);
                }
                Ok(ManagerMessage::Shutdown) => {
                    self.cleanup_child();
                    break;
                }
                Ok(ManagerMessage::Ready { generation, url }) => {
                    if self.lifecycle.accept_ready(generation) {
                        self.set_phase(
                            RuntimePhase::Ready,
                            format!("DSH 已就绪: {url}"),
                            Some(url.clone()),
                        );
                        notifications::show(
                            &self.app,
                            "DSH 已就绪",
                            &format!("DeepSeek Harness 已启动：{url}"),
                            Some(notifications::NotificationAction {
                                kind: "focus".into(),
                                session_id: None,
                                url: Some(url.clone()),
                                path: None,
                            }),
                        );
                        self.open_window(url);
                    }
                }
                Ok(ManagerMessage::ReadyTimeout { generation }) => {
                    if self.lifecycle.is_current(generation) {
                        self.fail_generation(
                            generation,
                            "DSH 未输出 URL line，等待超时".to_string(),
                        );
                    }
                }
                Ok(ManagerMessage::ReadinessError { generation, error }) => {
                    if self.lifecycle.is_current(generation) {
                        self.fail_generation(generation, format!("DSH 就绪输出无效: {error}"));
                    }
                }
                Ok(ManagerMessage::InstallFinished { result }) => match result {
                    Ok(()) => {
                        self.append_log("[desktop] DSH 安装完成");
                        self.notify("DSH 安装完成", "DeepSeek Harness 安装成功，正在启动…");
                        self.lifecycle.reset_restarts();
                        self.handle_start();
                    }
                    Err(error) => self.fail(format!("DSH 安装失败: {error}")),
                },
                Ok(ManagerMessage::Unhealthy { generation }) => {
                    if self.lifecycle.is_current(generation) {
                        self.fail_generation(generation, "DSH 健康检查连续失败".to_string());
                    }
                }
                Ok(ManagerMessage::Healthy { generation }) => {
                    if self.lifecycle.is_current(generation) {
                        self.commit_profile_healthy();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(exit) = self.take_exit() {
                        // 看门狗：运行中（starting/ready）意外退出时自动重启，超过上限才转 failed
                        let generation = self.lifecycle.generation();
                        match self.lifecycle.on_unexpected_exit(generation) {
                            RestartDecision::Ignore => {}
                            RestartDecision::Restart { attempt, limit } => {
                                self.append_log(&format!(
                                    "[desktop] DSH 进程意外退出 ({exit})，{attempt}/{limit} 自动重启"
                                ));
                                self.rollback_profile("进程意外退出自动重启");
                                self.handle_start();
                            }
                            RestartDecision::Fail => {
                                self.fail_generation(
                                    generation,
                                    format!("DSH 进程已退出 ({exit})"),
                                );
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.cleanup_child();
                    break;
                }
            }
        }
    }

    fn handle_start(&mut self) {
        self.failed_generation = None;
        self.cleanup_child();
        self.cleanup_stale_dsh_web();

        let config = config::load(&self.config_path).effective(|k| std::env::var(k).ok());
        let (node, entry) = match resolve_dsh(&config) {
            Some(pair) => pair,
            None => {
                let node_found = resolve_node(&config).is_some();
                let message = if node_found {
                    "未检测到 DSH"
                } else {
                    "未检测到 Node.js，请先安装 Node.js"
                };
                self.update_detection(node_found, false);
                self.set_phase(RuntimePhase::MissingDsh, message.to_string(), None);
                return;
            }
        };

        self.update_detection(true, true);
        let mut version_cmd = Command::new(&node);
        version_cmd
            .env("PATH", effective_path())
            .arg(&entry)
            .arg("--version");
        let version = version_cmd
            .output()
            .ok()
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if text.is_empty() {
                    String::from_utf8_lossy(&output.stderr)
                        .trim()
                        .to_string()
                        .into()
                } else {
                    text.into()
                }
            })
            .filter(|value| !value.is_empty());
        self.inner.lock().unwrap().dsh_version = version;
        self.set_phase(RuntimePhase::Starting, "正在启动 DSH...".to_string(), None);
        if let Err(error) = self.start(node, entry, config.dsh_home) {
            self.fail(error);
        }
    }

    fn start(&mut self, node: PathBuf, entry: PathBuf, home: Option<String>) -> Result<(), String> {
        self.cleanup_child();

        let workspace = std::env::var("HOME").unwrap_or_else(|_| ".".into());

        let generation = self.lifecycle.begin_start();

        // 解析 profile home：配置优先，否则 ~/.dsh；启动前读取 active profile 并保存本次启动上下文
        let profile_home = match &home {
            Some(home) => PathBuf::from(home),
            None => dirs::home_dir()
                .map(|dir| dir.join(".dsh"))
                .ok_or_else(|| "无法解析 DSH profile 目录".to_string())?,
        };
        // 丢弃上一次启动残留的上下文，避免陈旧 context 影响日志/回滚。
        self.startup_context = None;
        let startup_context = profiles::begin_startup(&self.profile_state_path, &profile_home)?;
        let active = startup_context.active.clone();
        self.startup_context = Some(startup_context);

        // 装配内嵌插件（best-effort）并生成 --patch overlay：任何失败只记日志，不影响 dsh 启动
        let mut log = |line: &str| self.append_log(line);
        let overlay = embedded::prepare_overlay(
            embedded::plugins_resource_dir(self.app.path().resource_dir().ok().as_deref())
                .as_deref(),
            Some(&profile_home),
            self.app.path().app_data_dir().ok().as_deref(),
            cfg!(debug_assertions),
            &mut log,
        );

        let args = dsh_web_args(&active, overlay.as_deref());
        let mut cmd = Command::new(&node);
        cmd.arg(&entry)
            .args(&args)
            .env("PATH", effective_path())
            .env("NO_COLOR", "1")
            .current_dir(&workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // 独立进程组：便于优雅退出时对整棵进程树发信号
            cmd.process_group(0);
        }
        if let Some(home) = &home {
            cmd.env("DSH_HOME", home);
        }
        cmd.env("DSH_DESKTOP_MANAGED", "1");
        cmd.env("DSH_DESKTOP_PROFILE", &active);
        let profile_dir = profile_home.join("profiles").join(&active);
        cmd.env("DSH_DESKTOP_PROFILE_DIR", &profile_dir);
        if let Some(state_dir) = self.app.path().app_data_dir().ok() {
            cmd.env("DSH_DESKTOP_STATE_DIR", &state_dir);
        }
        if let Some(host_token) = self.host_token.lock().ok().and_then(|guard| guard.clone()) {
            cmd.env("DSH_DESKTOP_HOST_TOKEN", host_token);
        }

        let mut child = cmd
            .spawn()
            .map_err(|error| format!("无法启动 dsh: {error}"))?;
        self.append_log(&format!(
            "[desktop] 启动 dsh: {} {} {}",
            node.display(),
            entry.display(),
            args.join(" ")
        ));

        let stdout = child.stdout.take().expect("stdout 已开启管道");
        let stderr = child.stderr.take().expect("stderr 已开启管道");
        self.spawn_url_reader(stdout, generation, "stdout");
        self.spawn_reader(stderr, "stderr");
        self.child = Some(child);

        self.spawn_health_checker();

        Ok(())
    }

    /// 健康检查通过后，把当前 active profile 提交为 last_known_good；失败仅记日志。
    fn commit_profile_healthy(&mut self) {
        let Some(context) = self.startup_context.as_ref() else {
            return;
        };
        match profiles::mark_healthy(&self.profile_state_path, &context.active) {
            Ok(_) => {
                self.append_log(&format!(
                    "[desktop] profile \"{}\" 健康检查通过，已提交",
                    context.active
                ));
                self.startup_context = None;
            }
            Err(error) => {
                self.append_log(&format!(
                    "[desktop] 提交 profile \"{}\" 健康状态失败: {error}",
                    context.active
                ));
            }
        }
    }

    /// 本次启动失败/自动重启前调用：回滚到 last_known_good 并清空启动上下文。
    fn rollback_profile(&mut self, reason: &str) {
        let Some(context) = self.startup_context.take() else {
            return;
        };
        match profiles::rollback_startup(&self.profile_state_path) {
            Ok(state) => {
                self.append_log(&format!(
                    "[desktop] {reason}，回滚 profile \"{}\" -> \"{}\"",
                    context.active, state.active
                ));
            }
            Err(error) => {
                self.append_log(&format!(
                    "[desktop] 回滚 profile \"{}\" 失败: {error}",
                    context.active
                ));
            }
        }
    }

    fn open_window(&self, url: String) {
        if let Some(window) = self.app.get_webview_window("main") {
            if let Ok(mut url) = tauri::Url::parse(&url) {
                if let Some(token) = self.host_token.lock().ok().and_then(|guard| guard.clone()) {
                    url.query_pairs_mut()
                        .append_pair(HOST_TOKEN_QUERY_KEY, &token);
                }
                let _ = window.navigate(url);
            }
            if !self.start_in_tray.load(Ordering::Relaxed) {
                let _ = window.show();
                let _ = window.set_focus();
            }
            emit_window_state(&self.app);
        }
    }

    fn fail(&mut self, message: String) {
        self.rollback_profile("启动失败");
        self.cleanup_child();
        self.append_log(&format!("[desktop] 失败: {message}"));
        self.notify("DSH 启动失败", &message);
        self.set_phase(RuntimePhase::Failed, message, None);
    }

    /// 同一 generation 的启动失败只落地一次，避免 reader/health/exit 重复上报。
    fn fail_generation(&mut self, generation: u64, message: String) {
        if self.failed_generation == Some(generation) {
            self.append_log(&format!("[desktop] 忽略重复的启动失败: {message}"));
            return;
        }
        self.failed_generation = Some(generation);
        self.fail(message);
    }

    /// 优雅停止子进程：unix 下先 SIGTERM 等宽限期，再 SIGKILL；Windows 直接 TerminateProcess。
    fn cleanup_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            self.lifecycle.invalidate();
            #[cfg(unix)]
            {
                let pid = child.id() as i32;
                // 子进程以进程组启动（start 中 process_group(0)），对负 pid 发 SIGTERM 可整组清理
                unsafe { libc::kill(-pid, libc::SIGTERM) };
                let deadline = Instant::now() + GRACE_PERIOD;
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    if Instant::now() >= deadline {
                        unsafe { libc::kill(-pid, libc::SIGKILL) };
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 清理崩溃/异常退出后残留的本应用 dsh web 实例。
    fn cleanup_stale_dsh_web(&self) {
        let Ok(app_data_dir) = self.app.path().app_data_dir() else {
            return;
        };
        let count = process::cleanup_stale_dsh_web(&app_data_dir);
        if count > 0 {
            self.append_log(&format!("[desktop] 已清理 {} 个旧 dsh web 实例", count));
        }
    }

    fn take_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(exit_summary(&status))
    }

    fn spawn_reader(&self, stream: impl std::io::Read + Send + 'static, label: &'static str) {
        let app = self.app.clone();
        let inner = self.inner.clone();
        let log_path = self.log_path.clone();
        thread::spawn(move || {
            append_stream(stream, &app, &inner, &log_path, label);
        });
    }

    fn spawn_url_reader(
        &self,
        stream: impl std::io::Read + Send + 'static,
        generation: u64,
        label: &'static str,
    ) {
        let app = self.app.clone();
        let inner = self.inner.clone();
        let log_path = self.log_path.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let mut parser = process::ReadinessParser::new();
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                let line = line.trim_end();
                match parser.push_line(line) {
                    Ok(Some(url)) => {
                        let _ = tx.send(ManagerMessage::Ready { generation, url });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        append_line(&app, &inner, &log_path, &format!("[{label}] {line}"));
                        let _ = tx.send(ManagerMessage::ReadinessError { generation, error });
                        return;
                    }
                }
                append_line(&app, &inner, &log_path, &format!("[{label}] {line}"));
            }
            if let Err(error) = parser.finalize() {
                let _ = tx.send(ManagerMessage::ReadinessError { generation, error });
            }
        });
    }

    fn spawn_health_checker(&self) {
        let inner = self.inner.clone();
        let tx = self.tx.clone();
        let generation = self.lifecycle.generation();
        thread::spawn(move || {
            let started = Instant::now();
            let url = loop {
                if let Some(url) = inner.lock().unwrap().url.clone() {
                    break url;
                }
                if started.elapsed() >= READY_TIMEOUT {
                    let _ = tx.send(ManagerMessage::ReadyTimeout { generation });
                    return;
                }
                thread::sleep(POLL_INTERVAL);
            };

            let mut failures = 0u32;
            let mut healthy_sent = false;
            loop {
                if is_health_ready(&url) {
                    failures = 0;
                    if !healthy_sent {
                        healthy_sent = true;
                        let _ = tx.send(ManagerMessage::Healthy { generation });
                    }
                } else {
                    failures += 1;
                    if failures >= 3 {
                        let _ = tx.send(ManagerMessage::Unhealthy { generation });
                        return;
                    }
                }
                thread::sleep(POLL_INTERVAL);
            }
        });
    }

    fn update_detection(&self, node_found: bool, dsh_installed: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.node_found = node_found;
        inner.dsh_installed = dsh_installed;
    }

    fn set_phase(&self, phase: RuntimePhase, message: String, url: Option<String>) {
        *self.managed_dsh_url.lock().unwrap() = if phase == RuntimePhase::Ready {
            url.as_deref().and_then(|url| tauri::Url::parse(url).ok())
        } else {
            None
        };
        {
            let mut inner = self.inner.lock().unwrap();
            inner.phase = phase;
            inner.message = message;
            inner.url = url;
        }
        emit_status(&self.app, &self.inner);
    }

    fn append_log(&self, line: &str) {
        append_line(&self.app, &self.inner, &self.log_path, line);
    }

    /// 发送系统原生通知（就绪 / 失败 / 安装完成等关键节点）。
    fn notify(&self, title: &str, body: &str) {
        notifications::show(&self.app, title, body, None);
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> RuntimeSnapshot {
    state.inner.lock().unwrap().snapshot()
}

#[tauri::command]
fn get_pending_deeplinks(state: State<AppState>) -> Vec<DeepLinkPayload> {
    let mut inner = state.inner.lock().unwrap();
    inner.prune_expired_pending_deeplinks();
    inner.pending_deeplinks.iter().cloned().collect()
}

#[tauri::command]
fn ack_deeplink(state: State<AppState>, id: String) -> Result<(), String> {
    state
        .inner
        .lock()
        .unwrap()
        .pending_deeplinks
        .retain(|item| item.id != id);
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WindowStateSnapshot {
    maximized: bool,
}

fn emit_window_state(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = app.emit(
            "dsh-window-state",
            WindowStateSnapshot {
                maximized: window.is_maximized().unwrap_or(false),
            },
        );
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ShortcutSnapshot {
    shortcut: String,
    registered: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct DropPosition {
    x: f64,
    y: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct FileDropPayload {
    id: String,
    paths: Vec<String>,
    kind: String,
    position: DropPosition,
    action: String,
}

static NEXT_DROP_ID: AtomicU64 = AtomicU64::new(0);

fn emit_file_drop(window: &tauri::WebviewWindow, paths: Vec<PathBuf>, action: &str) {
    let id = format!("drop-{}", NEXT_DROP_ID.fetch_add(1, Ordering::Relaxed));
    let kind = process::classify_drop(&paths);
    let payload = FileDropPayload {
        id,
        paths: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        kind: kind.to_string(),
        position: DropPosition { x: 0.0, y: 0.0 },
        action: action.to_string(),
    };
    let _ = window.emit("dsh-file-drop", payload);
}

#[tauri::command]
fn open_paths(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("未提供文件路径".to_string());
    }
    emit_file_drop(
        &window,
        paths.into_iter().map(PathBuf::from).collect(),
        "open",
    );
    Ok(())
}

#[tauri::command]
fn import_paths(window: tauri::WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("未提供目录路径".to_string());
    }
    emit_file_drop(
        &window,
        paths.into_iter().map(PathBuf::from).collect(),
        "import",
    );
    Ok(())
}

/// 无边框窗口控制：minimize / maximize（切换）/ close / toggle-visible（显示↔隐藏）。
#[tauri::command]
fn window_action(window: tauri::WebviewWindow, action: String) -> Result<(), String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => window.close().map_err(|e| e.to_string()),
        "toggle-visible" => {
            if window.is_visible().unwrap_or(false) {
                window.hide().map_err(|e| e.to_string())
            } else {
                if window.is_minimized().unwrap_or(false) {
                    window.unminimize().map_err(|e| e.to_string())?;
                }
                window.show().map_err(|e| e.to_string())?;
                let _ = window.set_focus();
                Ok(())
            }
        }
        other => Err(format!("未知窗口动作: {other}")),
    }
}

/// #rrggbb → tauri::window::Color
fn parse_window_color(hex: &str) -> Option<tauri::window::Color> {
    let value = hex.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(tauri::window::Color(r, g, b, 255))
}

#[tauri::command]
fn get_ui_theme(state: State<AppState>) -> UiThemeSnapshot {
    let system_dark = state
        .app
        .get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .map(|t| t == tauri::Theme::Dark)
        .unwrap_or(false);
    let home = settings_home(&state.config_path);
    let section = match home {
        Some(home) => read_ui_theme_section(&home.join("settings.yaml")),
        None => theme::UiThemeSection::default(),
    };
    resolve_ui_theme(&section, system_dark)
}

#[tauri::command]
fn get_profiles(state: State<AppState>) -> Vec<profiles::DshProfileSummary> {
    let home = settings_home(&state.config_path).unwrap_or_default();
    profiles::list_profiles(&home)
}

#[tauri::command]
fn get_active_profile(state: State<AppState>) -> profiles::ProfileState {
    profiles::load_state(&state.profile_state_path)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ProfileSelectionResult {
    profile: String,
    restart_required: bool,
}

#[tauri::command]
fn select_profile(state: State<AppState>, name: String) -> Result<ProfileSelectionResult, String> {
    let home = settings_home(&state.config_path).unwrap_or_default();
    let result = profiles::select_profile(&state.profile_state_path, &home, &name)?;
    Ok(ProfileSelectionResult {
        profile: result.pending.unwrap_or(name),
        restart_required: true,
    })
}

#[tauri::command]
fn restart(state: State<AppState>) -> Result<(), String> {
    state
        .tx
        .send(ManagerMessage::Start)
        .map_err(|_| "进程管理器已退出".to_string())
}

#[tauri::command]
fn install_dsh(state: State<AppState>) -> Result<(), String> {
    let phase = state.inner.lock().unwrap().phase.clone();
    if matches!(phase, RuntimePhase::Installing) {
        return Ok(());
    }

    let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());
    // 无 Node/npm 环境无法执行全局安装：前端在缺 Node 时不显示安装按钮，这里兜底返回清晰错误
    if resolve_npm(&config).is_none() {
        return Err("未检测到 npm，无法安装 DSH（请先安装 Node.js）".to_string());
    }

    {
        let mut inner = state.inner.lock().unwrap();
        inner.phase = RuntimePhase::Installing;
        inner.message = "正在安装 DSH...".to_string();
        inner.logs.clear();
    }
    emit_status(&state.app, &state.inner);

    let app = state.app.clone();
    let inner = state.inner.clone();
    let tx = state.tx.clone();
    let log_path = state.log_dir.join("install.log");
    thread::spawn(move || {
        let result = run_install(&config, &log_path, &app, &inner);
        let _ = tx.send(ManagerMessage::InstallFinished { result });
    });
    Ok(())
}

#[tauri::command]
fn update_dsh(state: State<AppState>) -> Result<(), String> {
    if matches!(state.inner.lock().unwrap().phase, RuntimePhase::Installing) {
        return Ok(());
    }
    let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());
    let npm = resolve_npm(&config).ok_or_else(|| "未检测到 npm，无法更新 DSH".to_string())?;
    {
        let mut inner = state.inner.lock().unwrap();
        inner.phase = RuntimePhase::Installing;
        inner.message = "正在更新 DSH...".to_string();
    }
    emit_status(&state.app, &state.inner);
    let app = state.app.clone();
    let inner = state.inner.clone();
    let tx = state.tx.clone();
    let log_path = state.log_dir.join("update.log");
    thread::spawn(move || {
        append_line(
            &app,
            &inner,
            &log_path,
            "[desktop] 执行: npm install -g @deepseek-ai/dsh@latest",
        );
        let mut command = Command::new(&npm);
        command
            .env("PATH", effective_path())
            .args(["install", "-g", "@deepseek-ai/dsh@latest"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| format!("无法启动 npm: {error}"));
        let result = child.and_then(|mut child| {
            if let Some(stdout) = child.stdout.take() {
                append_stream(stdout, &app, &inner, &log_path, "npm");
            }
            if let Some(stderr) = child.stderr.take() {
                append_stream(stderr, &app, &inner, &log_path, "npm");
            }
            child
                .wait()
                .map_err(|error| format!("npm 更新中断: {error}"))
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!("npm 更新失败 (exit {:?})", status.code()))
                    }
                })
        });
        let _ = tx.send(ManagerMessage::InstallFinished { result });
    });
    Ok(())
}

#[tauri::command]
fn open_log_directory(state: State<AppState>) -> Result<(), String> {
    open_with_system(&state.log_dir.to_string_lossy())
}

fn validate_config_path_setting(
    label: &str,
    value: &str,
    require_file: bool,
) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{label} 路径无效"));
    }
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(format!("{label} 必须是绝对路径"));
    }
    if require_file && !path.is_file() {
        return Err(format!("{label} 文件不存在: {value}"));
    }
    if !require_file && path.exists() && !path.is_dir() {
        return Err(format!("{label} 不是目录: {value}"));
    }
    Ok(())
}

fn validate_config_paths(config: &DshConfig) -> Result<(), String> {
    if let Some(value) = &config.dsh_bin {
        validate_config_path_setting("DSH_BIN", value, true)?;
    }
    if let Some(value) = &config.dsh_node {
        validate_config_path_setting("DSH_NODE", value, true)?;
    }
    if let Some(value) = &config.dsh_home {
        validate_config_path_setting("DSH_HOME", value, false)?;
    }
    Ok(())
}

/// 桥接面打开目标校验：仅允许 http(s) URL，或已存在的绝对文件/目录。
fn validate_open_external_target(target: &str) -> Result<(), String> {
    if target.trim().is_empty() || target.chars().any(char::is_control) {
        return Err("打开目标无效".to_string());
    }
    if let Ok(url) = tauri::Url::parse(target) {
        if matches!(url.scheme(), "http" | "https") {
            return Ok(());
        }
        return Err(format!("不允许打开非 HTTP(S) URL: {target}"));
    }
    let path = Path::new(target);
    if path.is_absolute() && path.exists() {
        return Ok(());
    }
    Err(format!("只允许打开已存在的绝对文件或目录: {target}"))
}

#[tauri::command]
fn get_config(state: State<AppState>) -> DshConfig {
    let stored = config::load(&state.config_path);
    stored.effective(|k| std::env::var(k).ok())
}

#[tauri::command]
fn set_config(state: State<AppState>, mut config: DshConfig) -> Result<(), String> {
    validate_config_paths(&config)?;
    // shortcuts 由壳侧快捷键注册表统一管理；set_config 只负责路径配置，
    // 因此始终保留现有注册记录，避免一次配置保存清空持久化快捷键。
    config.shortcuts = config::load(&state.config_path).shortcuts;
    config::save(&state.config_path, &config)
}

/// 用系统默认应用打开目标（URL / 文件 / 目录）。供本地窗口与桥接脚本共用。
#[tauri::command]
fn open_external(target: String) -> Result<(), String> {
    validate_open_external_target(&target)?;
    open_with_system(&target)
}

/// 查询开机自启状态（autostart 插件，macOS LaunchAgent / 其他平台系统自启）。
#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 设置开机自启开关（供 dsh 插件设置面板经桥接调用）。
#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

/// 查询开机自启与自启后窗口模式（OS 启停 + 壳侧持久化的启动模式）。
#[tauri::command]
fn get_desktop_settings(
    state: State<AppState>,
    app: AppHandle,
) -> Result<desktop_settings::DesktopSettings, String> {
    let autostart = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    let startup_mode = desktop_settings::read_startup_mode(&state.desktop_settings_path)
        .or_else(|| {
            settings_home(&state.config_path).map(|home| {
                desktop_settings::startup_mode(&desktop_settings::read_desktop_section(
                    &home.join("settings.yaml"),
                ))
            })
        })
        .unwrap_or(desktop_settings::StartupMode::Normal);
    Ok(desktop_settings::DesktopSettings {
        autostart,
        startup_mode,
    })
}

/// 设置开机自启开关与自启后窗口模式（供 dsh 插件设置面板经桥接调用）。
#[tauri::command]
fn set_desktop_settings(
    state: State<AppState>,
    app: AppHandle,
    settings: desktop_settings::DesktopSettings,
) -> Result<(), String> {
    let enabled = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    if enabled != settings.autostart {
        if settings.autostart {
            app.autolaunch().enable().map_err(|e| e.to_string())?;
        } else {
            app.autolaunch().disable().map_err(|e| e.to_string())?;
        }
    }
    desktop_settings::save_desktop_settings(&state.desktop_settings_path, &settings)
}

/// 读取本地项目列表（项目列表与右键菜单状态的唯一状态源）。
#[tauri::command]
fn get_projects(state: State<AppState>) -> Vec<projects::ProjectEntry> {
    projects::load(&state.projects_path)
}

/// 把本地目录加入项目列表；名称默认取目录名，路径重复时拒绝。
#[tauri::command]
fn add_project(state: State<AppState>, path: String) -> Result<projects::ProjectEntry, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("目录不存在: {path}"));
    }
    let mut entries = projects::load(&state.projects_path);
    if entries.iter().any(|p| p.path == path) {
        return Err("项目已在列表中".to_string());
    }
    let now = projects::now_millis();
    let entry = projects::ProjectEntry {
        id: projects::make_id(&path),
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        path,
        pinned: false,
        archived_chats: false,
        unread_chats: 0,
        read_at: None,
        created_at: now,
        updated_at: now,
    };
    entries.push(entry.clone());
    projects::save(&state.projects_path, &entries)?;
    Ok(entry)
}

/// 编辑项目（保留置顶 / 聊天状态等字段，只更新名称与路径）。
#[tauri::command]
fn update_project(
    state: State<AppState>,
    project: projects::ProjectEntry,
) -> Result<projects::ProjectEntry, String> {
    if project.name.trim().is_empty() {
        return Err("项目名称不能为空".to_string());
    }
    if !Path::new(&project.path).is_dir() {
        return Err(format!("目录不存在: {}", project.path));
    }
    let mut entries = projects::load(&state.projects_path);
    let Some(found) = entries.iter_mut().find(|p| p.id == project.id) else {
        return Err("项目不存在".to_string());
    };
    found.name = project.name.trim().to_string();
    found.path = project.path;
    found.updated_at = projects::now_millis();
    let entry = found.clone();
    projects::save(&state.projects_path, &entries)?;
    Ok(entry)
}

/// 从项目列表移除（不删除磁盘上的目录）。
#[tauri::command]
fn remove_project(state: State<AppState>, id: String) -> Result<(), String> {
    let mut entries = projects::load(&state.projects_path);
    let before = entries.len();
    entries.retain(|p| p.id != id);
    if entries.len() == before {
        return Err("项目不存在".to_string());
    }
    projects::save(&state.projects_path, &entries)
}

fn mutate_project(
    state: &AppState,
    id: &str,
    apply: impl FnOnce(&mut projects::ProjectEntry),
) -> Result<projects::ProjectEntry, String> {
    let mut entries = projects::load(&state.projects_path);
    let Some(entry) = entries.iter_mut().find(|p| p.id == id) else {
        return Err("项目不存在".to_string());
    };
    apply(entry);
    entry.updated_at = projects::now_millis();
    let result = entry.clone();
    projects::save(&state.projects_path, &entries)?;
    Ok(result)
}

/// 置顶 / 取消置顶项目。
#[tauri::command]
fn set_project_pinned(
    state: State<AppState>,
    id: String,
    pinned: bool,
) -> Result<projects::ProjectEntry, String> {
    mutate_project(&state, &id, |entry| entry.pinned = pinned)
}

/// 全部标为已读：清空未读计数并记录时间。
#[tauri::command]
fn mark_project_read(state: State<AppState>, id: String) -> Result<projects::ProjectEntry, String> {
    mutate_project(&state, &id, |entry| {
        entry.unread_chats = 0;
        entry.read_at = Some(projects::now_millis());
    })
}

/// 归档 / 恢复该项目下的会话聊天。
#[tauri::command]
fn archive_project_chats(
    state: State<AppState>,
    id: String,
    archived: bool,
) -> Result<projects::ProjectEntry, String> {
    mutate_project(&state, &id, |entry| entry.archived_chats = archived)
}

/// 在项目目录旁创建永久 git 工作树，并把工作树作为新项目加入列表。
#[tauri::command]
fn create_project_worktree(
    state: State<AppState>,
    id: String,
) -> Result<projects::ProjectWorktreeResult, String> {
    let entries = projects::load(&state.projects_path);
    let project = entries
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| "项目不存在".to_string())?;
    let (target, branch) = projects::create_worktree(&project, None)?;

    let mut entries = projects::load(&state.projects_path);
    if entries.iter().any(|p| p.path == target.to_string_lossy()) {
        return Err("工作树已在项目列表中".to_string());
    }
    let now = projects::now_millis();
    let entry = projects::ProjectEntry {
        id: projects::make_id(&target.to_string_lossy()),
        name: target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: target.to_string_lossy().into_owned(),
        pinned: false,
        archived_chats: false,
        unread_chats: 0,
        read_at: None,
        created_at: now,
        updated_at: now,
    };
    entries.push(entry.clone());
    projects::save(&state.projects_path, &entries)?;
    Ok(projects::ProjectWorktreeResult {
        entry,
        target: target.to_string_lossy().into_owned(),
        branch,
    })
}

/// 在系统文件管理器中定位项目目录。
#[tauri::command]
fn show_project_in_finder(state: State<AppState>, id: String) -> Result<(), String> {
    let entries = projects::load(&state.projects_path);
    let project = entries
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "项目不存在".to_string())?;
    projects::reveal_in_finder(&project.path)
}

/// 保存快捷键注册表到 config.json（保留其余配置字段）。
fn persist_shortcuts(config_path: &Path, shortcuts: &[String]) {
    let mut config = config::load(config_path);
    config.shortcuts = shortcuts.to_vec();
    let _ = config::save(config_path, &config);
}

/// 注册系统级全局快捷键（如 CmdOrCtrl+Shift+D），按下时 emit `dsh-shortcut`。
fn register_shortcut_internal(
    state: &AppState,
    shortcut: String,
) -> Result<ShortcutSnapshot, String> {
    let s = Shortcut::from_str(&shortcut).map_err(|e| e.to_string())?;
    if state.shortcuts.lock().unwrap().contains_key(&shortcut) {
        return Ok(ShortcutSnapshot {
            shortcut,
            registered: true,
        });
    }
    let app = state.app.clone();
    let trigger = shortcut.clone();
    app.global_shortcut()
        .on_shortcut(s, move |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app.emit("dsh-shortcut", trigger.clone());
            }
        })
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("already registered") || message.contains("already in use") {
                format!("快捷键 {shortcut} 已被其他应用占用")
            } else {
                format!("注册快捷键 {shortcut} 失败: {message}")
            }
        })?;
    let shortcuts = {
        let mut map = state.shortcuts.lock().unwrap();
        map.insert(shortcut.clone(), s);
        map.keys().cloned().collect::<Vec<_>>()
    };
    persist_shortcuts(&state.config_path, &shortcuts);
    Ok(ShortcutSnapshot {
        shortcut,
        registered: true,
    })
}

#[tauri::command]
fn register_shortcut(state: State<AppState>, shortcut: String) -> Result<ShortcutSnapshot, String> {
    register_shortcut_internal(&state, shortcut)
}

/// 注销已注册的全局快捷键。
#[tauri::command]
fn unregister_shortcut(state: State<AppState>, shortcut: String) -> Result<(), String> {
    let app = state.app.clone();
    if state.shortcuts.lock().unwrap().contains_key(&shortcut) {
        app.global_shortcut()
            .unregister(shortcut.as_str())
            .map_err(|e| e.to_string())?;
    }
    state.shortcuts.lock().unwrap().remove(&shortcut);
    let shortcuts = state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    persist_shortcuts(&state.config_path, &shortcuts);
    Ok(())
}

/// 查询当前已注册的全局快捷键。
#[tauri::command]
fn get_shortcuts(state: State<AppState>) -> Vec<ShortcutSnapshot> {
    state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .map(|shortcut| ShortcutSnapshot {
            shortcut,
            registered: true,
        })
        .collect()
}

/// 注销全部已注册的全局快捷键并清空持久化记录。
#[tauri::command]
fn unregister_all_shortcuts(state: State<AppState>) -> Result<(), String> {
    let shortcuts = state
        .shortcuts
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    if !shortcuts.is_empty() {
        state
            .app
            .global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())?;
    }
    state.shortcuts.lock().unwrap().clear();
    persist_shortcuts(&state.config_path, &[]);
    Ok(())
}

/// 查询 GitHub 最新 release 的 tag（如 v0.1.0）。失败返回 Err（网络/限流/解析）。
fn update_http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

async fn latest_release_tag() -> Result<String, String> {
    let client = update_http_client(UPDATE_API_TIMEOUT)?;
    let resp = client
        .get("https://api.github.com/repos/NoelOrin/dsh-desktop/releases/latest")
        .header("User-Agent", "dsh-desktop")
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("检查更新失败: HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析发布信息失败: {e}"))?;
    json.get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "检查更新失败: 响应缺少 tag_name".to_string())
}

/// 解析 GitHub release tag（如 v0.1.0）为 semver 版本。
fn parse_release_tag(tag: &str) -> Result<semver::Version, String> {
    semver::Version::parse(tag.trim_start_matches('v'))
        .map_err(|e| format!("解析远程版本 {tag} 失败: {e}"))
}

/// 检查 GitHub Release 是否有新版本，返回新版本号（无则 None）。
/// 仅供 dsh 插件"检查更新"经桥接调用。
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    let tag = latest_release_tag().await?;
    let remote = parse_release_tag(&tag)?;
    if remote > app.package_info().version {
        Ok(Some(tag))
    } else {
        Ok(None)
    }
}

/// 静默下载最新版安装包到应用缓存目录，返回本地路径（由用户手动运行安装）。
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<String, String> {
    let tag = latest_release_tag().await?;
    let remote = parse_release_tag(&tag)?;
    if remote <= app.package_info().version {
        return Err("没有可用更新".to_string());
    }

    // 当前平台的安装包扩展名
    let ext = if cfg!(target_os = "macos") {
        ".dmg"
    } else if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ".AppImage"
    };

    // 取最新 release 的资产下载地址
    let client = update_http_client(UPDATE_API_TIMEOUT)?;
    let resp = client
        .get("https://api.github.com/repos/NoelOrin/dsh-desktop/releases/latest")
        .header("User-Agent", "dsh-desktop")
        .send()
        .await
        .map_err(|e| format!("获取发布信息失败: {e}"))?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析发布信息失败: {e}"))?;
    let asset = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .map(|n| n.ends_with(ext))
                    .unwrap_or(false)
            })
        })
        .ok_or_else(|| format!("发布中未找到 {ext} 安装包"))?;
    let name = asset["name"]
        .as_str()
        .ok_or_else(|| "资产缺少 name".to_string())?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| "资产缺少下载地址".to_string())?;

    // 下载到 app_cache_dir/updates/<name>
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let updates_dir = cache_dir.join("updates");
    std::fs::create_dir_all(&updates_dir).map_err(|e| format!("创建更新目录失败: {e}"))?;
    let safe_name = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .ok_or_else(|| format!("资产名称无效: {name}"))?;
    let dest = updates_dir.join(safe_name);

    let mut resp = update_http_client(UPDATE_DOWNLOAD_TIMEOUT)?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status()));
    }
    let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败: {e}"))?;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下载中断: {e}"))? {
        file.write_all(&chunk)
            .map_err(|e| format!("写入失败: {e}"))?;
    }
    file.sync_all().map_err(|e| format!("同步失败: {e}"))?;

    // Linux AppImage 需要可执行权限
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置执行权限失败: {e}"))?;
    }

    let dest_string = dest.to_string_lossy().into_owned();
    notifications::show(
        &app,
        "更新下载完成",
        &format!("安装包已保存到 {dest_string}"),
        Some(notifications::NotificationAction {
            kind: "open_update".into(),
            session_id: None,
            url: None,
            path: Some(dest_string.clone()),
        }),
    );
    Ok(dest_string)
}

/// 请求系统通知权限，返回 granted / prompt / denied。
#[tauri::command]
fn request_notification_permission(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_notification::PermissionState;
    let state = app
        .notification()
        .request_permission()
        .map_err(|e| e.to_string())?;
    Ok(match state {
        PermissionState::Granted => "granted".to_string(),
        PermissionState::Prompt => "prompt".to_string(),
        PermissionState::PromptWithRationale => "prompt".to_string(),
        PermissionState::Denied => "denied".to_string(),
    })
}

fn emit_status(app: &AppHandle, inner: &Arc<Mutex<Inner>>) {
    let snapshot = inner.lock().unwrap().snapshot();
    let _ = app.emit("dsh-status", &snapshot);
    update_tray(app, &snapshot);
}

fn tray_phase_label(phase: &RuntimePhase) -> &'static str {
    match phase {
        RuntimePhase::Detecting => "检测中",
        RuntimePhase::MissingDsh => "未安装",
        RuntimePhase::Installing => "安装中",
        RuntimePhase::Starting => "启动中",
        RuntimePhase::Ready => "已就绪",
        RuntimePhase::Failed => "失败",
        RuntimePhase::Stopped => "已停止",
    }
}

fn update_tray(app: &AppHandle, snapshot: &RuntimeSnapshot) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let label = tray_phase_label(&snapshot.phase);
    let _ = state.status.set_text(format!("DSH: {label}"));
    let ready = matches!(snapshot.phase, RuntimePhase::Ready) && snapshot.url.is_some();
    let _ = state.copy_url.set_enabled(ready);
    let _ = state.open_browser.set_enabled(ready);
    let running = matches!(snapshot.phase, RuntimePhase::Starting | RuntimePhase::Ready);
    let _ = state.stop_dsh.set_enabled(running);
    let _ = state
        .restart_dsh
        .set_enabled(!matches!(snapshot.phase, RuntimePhase::Installing));
    let url_hint = snapshot
        .url
        .as_deref()
        .map(|url| format!(" ({url})"))
        .unwrap_or_default();
    let _ = state
        .tray
        .set_tooltip(Some(format!("DSH Desktop - {label}{url_hint}")));
}

fn rotate_log_if_needed(log_path: &Path) {
    let _guard = LOG_LOCK.lock().unwrap();
    let Ok(meta) = std::fs::metadata(log_path) else {
        return;
    };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let old = log_path.with_extension("log.1");
    let older = log_path.with_extension("log.2");
    let _ = std::fs::remove_file(&older);
    let _ = std::fs::rename(&old, &older);
    let _ = std::fs::rename(log_path, &old);
}

fn append_line(app: &AppHandle, inner: &Arc<Mutex<Inner>>, log_path: &Path, line: &str) {
    rotate_log_if_needed(log_path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{line}");
    }
    {
        let mut inner = inner.lock().unwrap();
        inner.logs.push_back(line.to_string());
        while inner.logs.len() > MAX_LOGS {
            inner.logs.pop_front();
        }
    }
    let _ = app.emit("dsh-log", line);
}

fn append_stream(
    stream: impl std::io::Read + Send + 'static,
    app: &AppHandle,
    inner: &Arc<Mutex<Inner>>,
    log_path: &Path,
    label: &'static str,
) {
    let reader = BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        append_line(app, inner, log_path, &format!("[{label}] {line}"));
    }
}

fn run_install(
    config: &DshConfig,
    log_path: &Path,
    app: &AppHandle,
    inner: &Arc<Mutex<Inner>>,
) -> Result<(), String> {
    let npm = resolve_npm(config)
        .ok_or_else(|| "未检测到 npm，无法安装 DSH（请先安装 Node.js）".to_string())?;
    append_line(
        app,
        inner,
        log_path,
        "[desktop] 执行: npm install -g @deepseek-ai/dsh",
    );
    let mut cmd = Command::new(&npm);
    cmd.env("PATH", effective_path())
        .arg("install")
        .arg("-g")
        .arg("@deepseek-ai/dsh")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|error| format!("无法启动 npm ({}): {error}", npm.display()))?;

    if let Some(stdout) = child.stdout.take() {
        append_stream(stdout, app, inner, log_path, "npm");
    }
    if let Some(stderr) = child.stderr.take() {
        append_stream(stderr, app, inner, log_path, "npm");
    }

    let status = child
        .wait()
        .map_err(|error| format!("npm 安装中断: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("npm 安装失败 (exit {:?})", status.code()))
    }
}

fn resolve_dsh(config: &DshConfig) -> Option<(PathBuf, PathBuf)> {
    let node = resolve_node(config)?;
    let entry = config
        .dsh_bin
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_in_path("dsh"))
        .or_else(|| npm_global_dsh(config))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/dsh"))
                .filter(|path| path.is_file())
        })?;
    Some((node, entry))
}

fn resolve_node(config: &DshConfig) -> Option<PathBuf> {
    config
        .dsh_node
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_in_path("node"))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/node"))
                .filter(|path| path.is_file())
        })
}

/// 解析可用的 npm：PATH 优先，其次 node 同目录；找不到返回 None（不再兜底为字面量 "npm"）。
fn resolve_npm(config: &DshConfig) -> Option<PathBuf> {
    find_in_path("npm").or_else(|| {
        resolve_node(config)
            .and_then(|node| node.parent().map(|dir| dir.join("npm")))
            .filter(|path| path.is_file())
    })
}

/// 定位 npm 全局安装的 dsh：先找真实入口 lib/bin.js，再回退到 bin 软链。
fn npm_global_dsh(config: &DshConfig) -> Option<PathBuf> {
    let prefix = npm_global_prefix(config)?;
    let modules = if cfg!(windows) {
        prefix.join("node_modules")
    } else {
        prefix.join("lib").join("node_modules")
    };
    let entry = modules.join("@deepseek-ai/dsh/lib/bin.js");
    if entry.is_file() {
        return Some(entry);
    }
    let bin = if cfg!(windows) {
        prefix.join("dsh")
    } else {
        prefix.join("bin").join("dsh")
    };
    bin.is_file().then_some(bin)
}

/// 通过 npm prefix -g 查询 npm 全局安装前缀（GUI 启动时 PATH 可能不含 npm bin）。
fn npm_global_prefix(config: &DshConfig) -> Option<PathBuf> {
    let npm = resolve_npm(config)?;
    let mut command = Command::new(&npm);
    command.env("PATH", effective_path()).args(["prefix", "-g"]);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8(output.stdout).ok()?;
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }
    Some(PathBuf::from(prefix))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn settings_home(config_path: &Path) -> Option<PathBuf> {
    let config = config::load(config_path).effective(|k| std::env::var(k).ok());
    config
        .dsh_home
        .map(PathBuf::from)
        .or_else(|| std::env::var("DSH_HOME").ok().map(PathBuf::from))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(format!("{h}/.dsh")))
        })
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = effective_path();
    let entries = std::env::split_paths(&path).collect::<Vec<_>>();
    find_in_path_entries(&entries, name)
}

fn find_in_path_entries(entries: &[PathBuf], name: &str) -> Option<PathBuf> {
    for dir in entries {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 合并 GUI 进程自身、系统全局 PATH 与用户 shell PATH，保证桌面启动时也能找到 node/npm/dsh。
fn effective_path() -> String {
    static CACHE: OnceLock<String> = OnceLock::new();

    CACHE
        .get_or_init(|| {
            let mut entries = env_path_entries();
            append_unique_path_entries(&mut entries, platform_system_path_entries());
            append_unique_path_entries(&mut entries, user_shell_path_entries());
            append_unique_path_entries(&mut entries, common_path_entries());
            std::env::join_paths(entries)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|_| std::env::var("PATH").unwrap_or_default())
        })
        .clone()
}

fn env_path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

fn parse_path_entries(path: &str) -> Vec<PathBuf> {
    std::env::split_paths(path)
        .filter(|entry| !entry.as_os_str().is_empty())
        .collect()
}

fn append_unique_path_entries(entries: &mut Vec<PathBuf>, extras: Vec<PathBuf>) {
    for entry in extras {
        if !entries.iter().any(|existing| existing == &entry) {
            entries.push(entry);
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_system_path_entries() -> Vec<PathBuf> {
    let mut entries = Vec::new();
    if let Ok(output) = Command::new("/usr/libexec/path_helper").arg("-s").output() {
        if output.status.success() {
            if let Some(path) = parse_path_helper_path(&output.stdout) {
                append_unique_path_entries(&mut entries, parse_path_entries(&path));
            }
        }
    }
    for file in macos_path_files() {
        append_unique_path_entries(&mut entries, read_path_file_entries(&file));
    }
    entries
}

#[cfg(target_os = "macos")]
fn macos_path_files() -> Vec<PathBuf> {
    let mut files = vec![PathBuf::from("/etc/paths")];
    if let Ok(entries) = std::fs::read_dir("/etc/paths.d") {
        let mut extra: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        extra.sort();
        files.extend(extra);
    }
    files
}

#[cfg(target_os = "macos")]
fn read_path_file_entries(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn parse_path_helper_path(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("PATH=") {
            let value = value.split(';').next().unwrap_or(value).trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value);
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn platform_system_path_entries() -> Vec<PathBuf> {
    read_environment_path_file(Path::new("/etc/environment"))
}

#[cfg(target_os = "windows")]
fn platform_system_path_entries() -> Vec<PathBuf> {
    windows_registry_path_entries()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_system_path_entries() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn read_environment_path_file(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| parse_exported_path_value(&content))
        .map(|path| parse_path_entries(&path))
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", test))]
fn parse_exported_path_value(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some(value) = line.strip_prefix("PATH=") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_registry_path_entries() -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for key in [
        "HKCU\\Environment",
        "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
    ] {
        append_unique_path_entries(&mut entries, windows_registry_path_value(key));
    }
    entries
}

#[cfg(target_os = "windows")]
fn windows_registry_path_value(key: &str) -> Vec<PathBuf> {
    let reg = std::env::var("SystemRoot")
        .map(|root| Path::new(&root).join("System32").join("reg.exe"))
        .ok()
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\reg.exe"));
    let Ok(output) = Command::new(reg)
        .args(["query", key, "/v", "Path"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(value) = parse_windows_registry_path_line(line) {
            append_unique_path_entries(
                &mut entries,
                parse_path_entries(&expand_environment_vars(&value)),
            );
        }
    }
    entries
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows_registry_path_line(line: &str) -> Option<String> {
    let line = line.trim();
    if !line.contains("Path") {
        return None;
    }
    let value = line
        .split_whitespace()
        .skip(2)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .trim_matches('"')
        .to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "windows")]
fn expand_environment_vars(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            result.push_str(&std::env::var(name).unwrap_or_else(|_| format!("%{name}%")));
            rest = &after[end + 1..];
        } else {
            result.push('%');
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

#[cfg(not(windows))]
fn user_shell_path_entries() -> Vec<PathBuf> {
    shell_path()
        .map(|path| parse_path_entries(&path))
        .unwrap_or_default()
}

#[cfg(windows)]
fn user_shell_path_entries() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(windows))]
fn shell_path() -> Option<String> {
    let candidates = std::env::var("SHELL")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .into_iter()
        .chain(
            [PathBuf::from("/bin/zsh"), PathBuf::from("/bin/bash")]
                .into_iter()
                .filter(|path| path.is_file()),
        );

    for shell in candidates {
        if let Some(path) = read_shell_path(&shell) {
            return Some(path);
        }
    }
    None
}

#[cfg(not(windows))]
fn read_shell_path(shell: &Path) -> Option<String> {
    // 输出重定向到临时文件，避免 shell 派生出的后台进程持有 stdout 管道，
    // 使父进程在子 shell 退出后仍无限阻塞读取。
    let output_path = std::env::temp_dir().join(format!(
        "dsh-shell-path-{}-{}.txt",
        std::process::id(),
        SHELL_PATH_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&output_path)
        .ok()?;
    let mut command = Command::new(shell);
    command
        .args(["-l", "-c", "printf '%s\\n' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::null())
        .env("TERM", "dumb")
        .env("COLORTERM", "");
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + Duration::from_millis(1500);

    let exited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.success()),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break None,
        }
    };

    let path = std::fs::read_to_string(&output_path)
        .ok()
        .and_then(|output| {
            output
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
        });
    let _ = std::fs::remove_file(&output_path);
    if exited == Some(true) {
        path.filter(|path| !path.is_empty())
    } else {
        None
    }
}

#[cfg(windows)]
fn shell_path() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn common_path_entries() -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

#[cfg(target_os = "linux")]
fn common_path_entries() -> Vec<PathBuf> {
    [
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/local/sbin",
        "/usr/sbin",
        "/sbin",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn common_path_entries() -> Vec<PathBuf> {
    Vec::new()
}

fn is_health_ready(url: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], parse_port(url))),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    if stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .is_err()
    {
        return false;
    }
    let request = "GET /dsh-desktop/health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n";
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    line.starts_with("HTTP/1.0 200") || line.starts_with("HTTP/1.1 200")
}

fn parse_port(url: &str) -> u16 {
    let authority = url.split('/').nth(2).unwrap_or(url);
    authority
        .rsplit(':')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3080)
}

fn exit_summary(status: &std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "unknown".into())
}

fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(target).status();
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(target).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(target)
        .status();
    status
        .map(|_| ())
        .map_err(|error| format!("无法打开 {target}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_deeplink_payload(id: &str, url: &str, received_at: &str) -> DeepLinkPayload {
        DeepLinkPayload {
            id: id.to_string(),
            url: url.to_string(),
            raw: url.to_string(),
            received_at: received_at.to_string(),
            source: "deep_link".to_string(),
            args: Vec::new(),
            cwd: String::new(),
        }
    }

    #[test]
    fn pending_deeplinks_prune_expired_entries() {
        let mut inner = Inner::default();
        let now = unix_millis();
        inner
            .pending_deeplinks
            .push_back(test_deeplink_payload("dl-1", "dsh-desktop://old", "1"));
        inner.pending_deeplinks.push_back(test_deeplink_payload(
            "dl-2",
            "dsh-desktop://new",
            &now.saturating_sub(1).to_string(),
        ));

        inner.prune_expired_pending_deeplinks();

        assert_eq!(inner.pending_deeplinks.len(), 1);
        assert_eq!(inner.pending_deeplinks[0].id, "dl-2");
    }

    #[test]
    fn pending_deeplinks_deduplicate_same_payload() {
        let mut inner = Inner::default();
        let now = unix_millis().to_string();
        let payload = test_deeplink_payload("dl-1", "dsh-desktop://session/1", &now);

        assert!(enqueue_pending_deeplink(&mut inner, &payload));
        assert!(!enqueue_pending_deeplink(&mut inner, &payload));
        assert_eq!(inner.pending_deeplinks.len(), 1);
    }

    #[test]
    fn pending_deeplinks_drop_oldest_beyond_limit() {
        let mut inner = Inner::default();
        let now = unix_millis();
        for index in 0..(MAX_PENDING_DEEPLINKS + 5) {
            let payload = test_deeplink_payload(
                &format!("dl-{index}"),
                &format!("dsh-desktop://session/{index}"),
                &now.to_string(),
            );
            assert!(enqueue_pending_deeplink(&mut inner, &payload));
        }

        assert_eq!(inner.pending_deeplinks.len(), MAX_PENDING_DEEPLINKS);
        assert!(inner
            .pending_deeplinks
            .iter()
            .all(|payload| !payload.id.ends_with("dl-0")));
        assert_eq!(
            inner
                .pending_deeplinks
                .back()
                .map(|payload| payload.id.as_str()),
            Some("dl-132")
        );
    }

    #[test]
    fn missing_dsh_serializes_as_missing() {
        // 前端契约（RuntimePhase）使用 "missing"，后端序列化必须一致
        assert_eq!(
            serde_json::to_string(&RuntimePhase::MissingDsh).unwrap(),
            "\"missing\""
        );
    }

    #[test]
    fn other_phases_serialize_in_snake_case() {
        for (phase, expected) in [
            (RuntimePhase::Detecting, "detecting"),
            (RuntimePhase::Installing, "installing"),
            (RuntimePhase::Starting, "starting"),
            (RuntimePhase::Ready, "ready"),
            (RuntimePhase::Failed, "failed"),
            (RuntimePhase::Stopped, "stopped"),
        ] {
            assert_eq!(
                serde_json::to_string(&phase).unwrap(),
                format!("\"{expected}\"")
            );
        }
    }

    #[test]
    fn tray_labels_cover_all_phases() {
        assert_eq!(tray_phase_label(&RuntimePhase::Detecting), "检测中");
        assert_eq!(tray_phase_label(&RuntimePhase::Ready), "已就绪");
        assert_eq!(tray_phase_label(&RuntimePhase::Stopped), "已停止");
    }

    #[test]
    fn parse_port_handles_health_url_path() {
        assert_eq!(parse_port("http://127.0.0.1:62359"), 62359);
        assert_eq!(
            parse_port("http://127.0.0.1:62359/dsh-desktop/health"),
            62359
        );
        assert_eq!(parse_port("http://[::1]:62359/dsh-desktop/health"), 62359);
    }

    #[test]
    fn append_unique_path_entries_dedups_preserving_order() {
        let mut entries = vec![PathBuf::from("/a")];
        append_unique_path_entries(
            &mut entries,
            vec![
                PathBuf::from("/b"),
                PathBuf::from("/a"),
                PathBuf::from("/b"),
            ],
        );
        assert_eq!(entries, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn find_in_path_prefers_earlier_entry() {
        let dir =
            std::env::temp_dir().join(format!("dsh-desktop-path-test-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let node = bin.join("node");
        std::fs::write(&node, b"").unwrap();

        let result = find_in_path_entries(&[bin.clone(), PathBuf::from("/usr/bin")], "node");
        assert_eq!(result, Some(node));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_path_helper_path_extracts_value() {
        assert_eq!(
            parse_path_helper_path(b"PATH=\"/a:/b\"; export PATH;\n"),
            Some("/a:/b".to_string())
        );
    }

    #[test]
    fn parse_exported_path_value_parses_common_forms() {
        assert_eq!(
            parse_exported_path_value("PATH=\"/a:/b\"\n"),
            Some("/a:/b".to_string())
        );
        assert_eq!(
            parse_exported_path_value("export PATH=/a:/b\n"),
            Some("/a:/b".to_string())
        );
        assert_eq!(parse_exported_path_value("NO_PATH=/x\n"), None);
    }

    #[test]
    fn parse_windows_registry_path_line_extracts_value() {
        assert_eq!(
            parse_windows_registry_path_line("    Path    REG_EXPAND_SZ    C:\\node;C:\\Windows"),
            Some("C:\\node;C:\\Windows".to_string())
        );
        assert_eq!(
            parse_windows_registry_path_line("HKEY_CURRENT_USER\\Environment"),
            None
        );
    }

    #[test]
    fn navigation_policy_allows_shell_urls() {
        for url in [
            "tauri://localhost",
            "http://localhost:5173",
            "http://127.0.0.1:5173",
        ] {
            let parsed = tauri::Url::parse(url).unwrap();
            assert!(is_shell_url(&parsed), "应允许 {url}");
        }
    }

    #[test]
    fn navigation_policy_rejects_unmanaged_loopback_urls() {
        for url in [
            "https://example.com",
            "http://0.0.0.0:8080",
            "http://127.0.0.1:49321",
            "http://localhost:5174",
            "file:///tmp/index.html",
            "about:blank",
        ] {
            let parsed = tauri::Url::parse(url).unwrap();
            assert!(!is_shell_url(&parsed), "本地页面应拒绝 {url}");
        }
        assert!(is_external_url(
            &tauri::Url::parse("https://example.com").unwrap()
        ));
        assert!(!is_external_url(
            &tauri::Url::parse("file:///tmp/index.html").unwrap()
        ));
    }

    #[test]
    fn managed_dsh_url_matches_exact_origin() {
        let managed = tauri::Url::parse("http://127.0.0.1:49321").unwrap();
        assert!(is_managed_dsh_url(
            &tauri::Url::parse("http://127.0.0.1:49321/settings").unwrap(),
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
            &tauri::Url::parse("http://127.0.0.1:49321").unwrap(),
            None
        ));
    }

    #[test]
    fn open_external_validates_targets() {
        assert!(validate_open_external_target("https://example.com").is_ok());
        assert!(validate_open_external_target("http://127.0.0.1:8080").is_ok());
        assert!(validate_open_external_target("file:///tmp/a").is_err());
        assert!(validate_open_external_target("/tmp/does-not-exist").is_err());

        let dir = std::env::temp_dir().join(format!("dsh-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file.txt");
        std::fs::write(&file, "ok").unwrap();
        assert!(validate_open_external_target(&file.to_string_lossy()).is_ok());
        assert!(validate_open_external_target(&dir.to_string_lossy()).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_paths_require_absolute_and_existing_binaries() {
        let dir = std::env::temp_dir().join(format!("dsh-config-path-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("bin");
        std::fs::write(&bin, "ok").unwrap();

        assert!(validate_config_paths(&DshConfig {
            dsh_bin: Some(bin.to_string_lossy().into_owned()),
            dsh_node: None,
            dsh_home: Some(dir.to_string_lossy().into_owned()),
            shortcuts: vec![],
        })
        .is_ok());
        assert!(validate_config_paths(&DshConfig {
            dsh_bin: Some("dsh".into()),
            dsh_node: None,
            dsh_home: None,
            shortcuts: vec![],
        })
        .is_err());
        assert!(validate_config_paths(&DshConfig {
            dsh_bin: None,
            dsh_node: None,
            dsh_home: Some(dir.join("missing").to_string_lossy().into_owned()),
            shortcuts: vec![],
        })
        .is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dsh_web_args_uses_active_profile_with_overlay() {
        let overlay = std::env::temp_dir().join("dsh-overlay.yml");
        let args = dsh_web_args("workbench", Some(&overlay));

        assert_eq!(args.first().map(String::as_str), Some("--profile"));
        assert_eq!(args.get(1).map(String::as_str), Some("workbench"));
        assert!(args.iter().any(|arg| arg == "--patch"));
        assert!(args
            .iter()
            .any(|arg| arg == &overlay.to_string_lossy().to_string()));
        assert!(!args.iter().any(|arg| arg == "web"));
        assert_eq!(
            args,
            vec![
                "--profile".to_string(),
                "workbench".to_string(),
                "--patch".to_string(),
                overlay.to_string_lossy().into_owned(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "0".to_string(),
            ]
        );
    }

    #[test]
    fn dsh_web_args_omits_patch_without_overlay() {
        let args = dsh_web_args("workbench", None);

        assert_eq!(args.first().map(String::as_str), Some("--profile"));
        assert_eq!(args.get(1).map(String::as_str), Some("workbench"));
        assert!(!args.iter().any(|arg| arg == "--patch"));
        assert!(!args.iter().any(|arg| arg == "web"));
        assert_eq!(
            args,
            vec![
                "--profile".to_string(),
                "workbench".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "0".to_string(),
            ]
        );
    }
}
