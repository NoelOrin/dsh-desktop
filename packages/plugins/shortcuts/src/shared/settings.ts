/** shortcuts 插件设置分节：当前只配置“双击 Esc 停止当前对话”。 */

export const DEFAULT_DOUBLE_ESCAPE_TIMEOUT_MS = 1000;
export const SHORTCUTS_STORAGE_KEY = "@dsh-desktop/plugin-shortcuts:settings";

export interface ShortcutsSettings {
  doubleEscapeStopEnabled: boolean;
  doubleEscapeStopTimeoutMs: number;
}

export function normalizeShortcutsSettings(
  value: Partial<ShortcutsSettings> | undefined,
): ShortcutsSettings {
  return {
    doubleEscapeStopEnabled: value?.doubleEscapeStopEnabled ?? true,
    doubleEscapeStopTimeoutMs: value?.doubleEscapeStopTimeoutMs ?? DEFAULT_DOUBLE_ESCAPE_TIMEOUT_MS,
  };
}
