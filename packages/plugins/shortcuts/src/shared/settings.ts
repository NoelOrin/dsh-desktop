/** shortcuts 插件设置分节：双击 Esc 停止当前对话 + 常用动作全局快捷键预设。 */

export const DEFAULT_DOUBLE_ESCAPE_TIMEOUT_MS = 1000;
export const SHORTCUTS_STORAGE_KEY = "@dsh-desktop/plugin-shortcuts:settings";

/** 常用动作预设 id：每个预设把系统级全局快捷键绑定到一个网页侧动作。 */
export type ShortcutPresetId = "toggleWindow" | "stopConversation" | "newConversation";

/** 常用动作预设配置：开关 + 绑定的全局快捷键字符串。 */
export interface ShortcutPresetSettings {
  /** 是否启用（启用即注册全局快捷键）。 */
  enabled: boolean;
  /** 全局快捷键字符串（如 CmdOrCtrl+Shift+D）。 */
  shortcut: string;
}

export interface ShortcutsSettings {
  doubleEscapeStopEnabled: boolean;
  doubleEscapeStopTimeoutMs: number;
  /** 常用动作预设：id → 开关与快捷键。 */
  presets: Record<ShortcutPresetId, ShortcutPresetSettings>;
}

export const SHORTCUT_PRESET_IDS: ShortcutPresetId[] = [
  "toggleWindow",
  "stopConversation",
  "newConversation",
];

/** 各预设的默认全局快捷键（用户可改；默认关闭，避免抢占系统级组合键）。 */
export const DEFAULT_PRESET_SHORTCUTS: Record<ShortcutPresetId, string> = {
  toggleWindow: "CmdOrCtrl+Shift+Space",
  stopConversation: "CmdOrCtrl+Shift+.",
  newConversation: "CmdOrCtrl+Shift+N",
};

function defaultPresetSettings(): Record<ShortcutPresetId, ShortcutPresetSettings> {
  return {
    toggleWindow: { enabled: false, shortcut: DEFAULT_PRESET_SHORTCUTS.toggleWindow },
    stopConversation: { enabled: false, shortcut: DEFAULT_PRESET_SHORTCUTS.stopConversation },
    newConversation: { enabled: false, shortcut: DEFAULT_PRESET_SHORTCUTS.newConversation },
  };
}

export function normalizeShortcutsSettings(
  value: Partial<ShortcutsSettings> | undefined,
): ShortcutsSettings {
  const defaults = defaultPresetSettings();
  const incoming = value?.presets ?? {};
  const presets = SHORTCUT_PRESET_IDS.reduce<Record<ShortcutPresetId, ShortcutPresetSettings>>(
    (acc, id) => {
      const item = incoming[id];
      acc[id] = {
        enabled: item?.enabled ?? defaults[id].enabled,
        shortcut: item?.shortcut?.trim() || defaults[id].shortcut,
      };
      return acc;
    },
    {} as Record<ShortcutPresetId, ShortcutPresetSettings>,
  );
  return {
    doubleEscapeStopEnabled: value?.doubleEscapeStopEnabled ?? true,
    doubleEscapeStopTimeoutMs: value?.doubleEscapeStopTimeoutMs ?? DEFAULT_DOUBLE_ESCAPE_TIMEOUT_MS,
    presets,
  };
}
