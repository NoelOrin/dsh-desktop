export type RuntimePhase =
  | "detecting"
  | "missing"
  | "installing"
  | "starting"
  | "ready"
  | "failed"
  | "stopped";

export type StartupMode = "normal" | "tray" | "minimized";

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

export type WindowAction = "minimize" | "maximize" | "close";

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
