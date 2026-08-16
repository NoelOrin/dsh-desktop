# AGENTS.md - packages/contracts（共享 IPC 类型）

前后端 IPC 契约的 TypeScript 类型与常量定义，供 `apps/shell`（启动页）使用。

## 内容

- `RuntimePhase` - phase 联合类型（snake_case，与 Rust `RuntimePhase` serde 一致）
- `RuntimeSnapshot` - `get_status` 返回（phase/message/url/dsh_installed/node_found/dsh_version/log_dir/logs）
- `DshConfig` - `get_config` / `set_config` 的配置结构（dsh_bin/dsh_node/dsh_home/remote_plugins_path/shortcuts/remote_plugins；字符串字段 null 表示未设置，shortcuts/remote_plugins 为数组）
- `RemotePluginPreset` / `RemotePluginSource` - 远程插件预设（id/url/enabled/group/source），启动时自动安装；`source=external` 表示外部目录固定预设
  - `RemotePluginPreset` 可选 `allow_build`：pnpm `--allow-build` 白名单包名数组
- `PluginOperationResult` / `InstalledPluginSummary` - 受管插件操作结果与已安装插件列表
- `StartupMode` - 启动模式（`normal` / `tray` / `minimized`）
- `DesktopMode` - 桌面界面模式（`compatibility` / `advanced`，Rust 启动时读取 `settings.yaml` 的 `dsh-desktop.mode`）
- `DshProfileSummary` / `DesktopProfileState` / `ProfileSelectionResult` - profile 发现与切换状态机契约（`web_capable` / active / pending / last_known_good / restart_required）
- `DesktopSettings` - `get_desktop_settings` / `set_desktop_settings` 的自启设置（autostart/startup_mode）
- `LanProxySettings` / `LanProxySnapshot` - `get_lan_proxy` / `set_lan_proxy` / `start_lan_proxy` / `stop_lan_proxy` 的局域网反向代理配置与运行状态（bind/port/target/running/url/urls/error）
- `ProjectEntry` / `ProjectWorktreeResult` - `get_projects` / `add_project` / `update_project` / `remove_project` / `set_project_pinned` / `mark_project_read` / `archive_project_chats` / `create_project_worktree` / `show_project_in_finder` 的项目列表契约
- `DeepLinkPayload` - `dsh-deeplink` / `get_pending_deeplinks` / `ack_deeplink` 的深链载荷（id/url/raw/received_at/source/args/cwd）
- `FileDropPayload` - `dsh-file-drop` / `open_paths` / `import_paths` 的拖放载荷（paths/kind/position/action）
- `ShortcutSnapshot` - `get_shortcuts` 返回的快捷键快照（shortcut/registered）
- `NotificationActionPayload` - `dsh-notification-action` 的通知动作载荷（kind/session_id/url/path）
- `UiThemeSnapshot` / `UiThemeTokens` - `get_ui_theme` / `dsh-ui-theme` 的主题快照
- `WindowState` / `WindowAction` - `window_action` / `dsh-window-state` 的窗口控制契约（`minimize` / `maximize` / `close` / `toggle-visible`）
- `COMMANDS` - 当前常量：`get_status` / `install_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config` / `open_external` / `get_ui_theme` / `window_action` / `get_desktop_settings` / `set_desktop_settings` / `get_projects` / `add_project` / `update_project` / `remove_project` / `set_project_pinned` / `mark_project_read` / `archive_project_chats` / `create_project_worktree` / `show_project_in_finder` / `get_profiles` / `get_active_profile` / `select_profile` / `get_remote_plugins` / `set_remote_plugins` / `sync_remote_plugins` / `get_installed_plugins` / `install_profile_plugin` / `remove_profile_plugin` / `update_profile_plugins` / `get_shortcuts` / `unregister_all_shortcuts` / `get_pending_deeplinks` / `ack_deeplink` / `request_notification_permission` / `update_dsh` / `open_paths` / `import_paths`
  - 局域网命令：`get_lan_proxy` / `set_lan_proxy` / `start_lan_proxy` / `stop_lan_proxy`
- `COMMANDS` 只列本地 main 窗口消费的 native 常量；`get_autostart` / `set_autostart` / `register_shortcut` / `unregister_shortcut` / `check_update` / `install_update` 等桥接侧命令不在该常量表，契约由 bridge 自持
- `EVENTS` - 事件名字符串常量：`dsh-status` / `dsh-log` / `dsh-file-drop` / `dsh-theme` / `dsh-deeplink` / `dsh-shortcut` / `dsh-update-available` / `dsh-ui-theme` / `dsh-window-state` / `dsh-notification-action`

## 边界

- 本包只负责 **native（桌面壳原生）IPC 契约**：Rust 后端与 main 窗口/启动页之间的命令与事件
- **桥接不属于本包**：dsh 插件调用 Tauri 壳能力的桥接命令契约，由 `packages/plugins/bridge` 自持，
  Rust 侧在 `apps/shell/src-tauri/capabilities/bridge.json` 声明，不并入本包；`get_autostart` / `set_autostart` / `get_desktop_settings` / `set_desktop_settings` / 快捷键 / `check_update` / `install_update` 等命令当前主要经桥接使用，桥接对象形状由 bridge 侧定义，本包只提供可复用类型

## 同步规则

修改任何 IPC 契约时，必须同步：`packages/contracts/src/index.ts`、
Rust 端 `apps/shell/src-tauri/src/lib.rs`（及 config.rs）的 serde 类型、
`apps/shell/src/main.ts`、根与子模块 `AGENTS.md`。
字段命名统一 snake_case（Rust serde `rename_all = "snake_case"`，TS 侧直接写 snake_case）。
