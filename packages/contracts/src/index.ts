export type RuntimePhase =
  | "detecting"
  | "missing"
  | "installing"
  | "starting"
  | "ready"
  | "failed"
  | "stopped";

export interface RuntimeSnapshot {
  phase: RuntimePhase;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  log_dir: string | null;
  logs: string[];
}

export interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
}

export const COMMANDS = {
  getStatus: "get_status",
  installDsh: "install_dsh",
  restart: "restart",
  openLogDirectory: "open_log_directory",
  getConfig: "get_config",
  setConfig: "set_config",
  openExternal: "open_external",
} as const;

export const EVENTS = {
  dshStatus: "dsh-status",
  dshLog: "dsh-log",
  dshFileDrop: "dsh-file-drop",
  dshTheme: "dsh-theme",
  dshDeeplink: "dsh-deeplink",
  dshShortcut: "dsh-shortcut",
  dshUpdateAvailable: "dsh-update-available",
} as const;
