export type RuntimePhase =
  | "detecting"
  | "missing"
  | "installing"
  | "starting"
  | "ready"
  | "failed"
  | "stopped";

export type StartupMode = "normal" | "tray" | "minimized";

/** dsh 桌面运行模式。 */
export type DesktopMode = "compatibility" | "advanced";

export interface DeepLinkPayload {
  id: string;
  url: string;
  raw: string;
  received_at: string;
  source: "deep_link" | "second_instance";
  args: string[];
  cwd: string;
}

export interface FileDropPayload {
  id: string;
  paths: string[];
  kind: "file" | "directory" | "mixed";
  position: { x: number; y: number };
  action: "open" | "import";
}

export interface ShortcutSnapshot {
  shortcut: string;
  registered: boolean;
}

export interface NotificationActionPayload {
  kind: "focus" | "open_session" | "open_update";
  session_id: string | null;
  url: string | null;
  path: string | null;
}

export interface RuntimeSnapshot {
  phase: RuntimePhase;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  dsh_version: string | null;
  log_dir: string | null;
  logs: string[];
}

export interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
  shortcuts: string[];
}

export interface DesktopSettings {
  autostart: boolean;
  startup_mode: StartupMode;
}

/** 壳侧维护的本地项目条目（项目列表与右键菜单操作的唯一状态源）。 */
export interface ProjectEntry {
  id: string;
  name: string;
  path: string;
  pinned: boolean;
  /** 该项目下的会话聊天是否已归档。 */
  archived_chats: boolean;
  unread_chats: number;
  /** 最近一次“全部标为已读”的时间（unix 毫秒）。 */
  read_at: number | null;
  created_at: number;
  updated_at: number;
}

/** 创建永久工作树的结果：工作树目录已作为新项目加入列表。 */
export interface ProjectWorktreeResult {
  entry: ProjectEntry;
  target: string;
  branch: string;
}

/** dsh profile 摘要（profile 发现结果）。 */
export interface DshProfileSummary {
  name: string;
  dir: string;
  exists: boolean;
  web_capable: boolean;
  problem: string | null;
}

/** profile 切换状态机快照（版本化，便于损坏恢复）。 */
export interface DesktopProfileState {
  version: 1;
  active: string;
  pending: string | null;
  last_known_good: string;
}

/** profile 选择结果。 */
export interface ProfileSelectionResult {
  profile: string;
  restart_required: boolean;
}

/** 受管 dsh 插件操作结果。 */
export interface PluginOperationResult {
  ok: boolean;
  exit_code: number | null;
  output: string[];
}

export interface UiThemeTokens {
  bg: string;
  fg: string;
  muted: string;
  accent: string;
  field: string;
  line: string;
  button_fg: string;
  scheme: "light" | "dark";
}

export interface UiThemeSnapshot {
  tokens: UiThemeTokens;
  preference: "light" | "dark" | "system";
  mode: "light" | "dark";
}

export type WindowAction = "minimize" | "maximize" | "close" | "toggle-visible";

export interface WindowState {
  maximized: boolean;
}

export const COMMANDS = {
  getStatus: "get_status",
  installDsh: "install_dsh",
  restart: "restart",
  openLogDirectory: "open_log_directory",
  getConfig: "get_config",
  setConfig: "set_config",
  openExternal: "open_external",
  getUiTheme: "get_ui_theme",
  windowAction: "window_action",
  getDesktopSettings: "get_desktop_settings",
  setDesktopSettings: "set_desktop_settings",
  getProjects: "get_projects",
  addProject: "add_project",
  updateProject: "update_project",
  removeProject: "remove_project",
  setProjectPinned: "set_project_pinned",
  markProjectRead: "mark_project_read",
  archiveProjectChats: "archive_project_chats",
  createProjectWorktree: "create_project_worktree",
  showProjectInFinder: "show_project_in_finder",
  getProfiles: "get_profiles",
  selectProfile: "select_profile",
  getActiveProfile: "get_active_profile",
  installProfilePlugin: "install_profile_plugin",
  removeProfilePlugin: "remove_profile_plugin",
  updateProfilePlugins: "update_profile_plugins",
  getShortcuts: "get_shortcuts",
  unregisterAllShortcuts: "unregister_all_shortcuts",
  getPendingDeepLinks: "get_pending_deeplinks",
  ackDeepLink: "ack_deeplink",
  requestNotificationPermission: "request_notification_permission",
  updateDsh: "update_dsh",
  openPaths: "open_paths",
  importPaths: "import_paths",
} as const;

export const EVENTS = {
  dshStatus: "dsh-status",
  dshLog: "dsh-log",
  dshFileDrop: "dsh-file-drop",
  dshTheme: "dsh-theme",
  dshDeeplink: "dsh-deeplink",
  dshShortcut: "dsh-shortcut",
  dshUpdateAvailable: "dsh-update-available",
  dshUiTheme: "dsh-ui-theme",
  dshWindowState: "dsh-window-state",
  dshNotificationAction: "dsh-notification-action",
} as const;
