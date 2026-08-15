import {
  Button,
  IconPlusOutline16,
  IconTrashOutline16,
  Input,
  Toast,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { useEffect, useState, useSyncExternalStore } from "react";
import { injectPluginCss } from "../../client-kit/inject";
import {
  getBridge,
  type SettingsScopeLike,
  type ShortcutSnapshot,
  type ShortcutsSettings,
  type Translate,
} from "./client/runtime";
import { SettingsPage, SettingsSection } from "./client/settings-layout";
import css from "./client/shortcuts.module.css";
import {
  normalizeShortcutsSettings,
  SHORTCUT_PRESET_IDS,
  SHORTCUTS_STORAGE_KEY,
  type ShortcutPresetId,
  type ShortcutPresetSettings,
} from "./shared/settings";

injectPluginCss("@dsh-desktop/plugin-shortcuts", "@dsh-desktop/plugin-shortcuts/ui");

const DEFAULT_SHORTCUTS_SETTINGS = normalizeShortcutsSettings(undefined);
let localShortcutsSettings = readLocalShortcutsSettings();
const localSettingsListeners = new Set<() => void>();
let localSettingsSnapshot: { status: "ready"; value: ShortcutsSettings } = {
  status: "ready",
  value: localShortcutsSettings,
};

function readLocalShortcutsSettings(): ShortcutsSettings {
  if (typeof localStorage === "undefined") {
    return normalizeShortcutsSettings(undefined);
  }
  try {
    const raw = localStorage.getItem(SHORTCUTS_STORAGE_KEY);
    return normalizeShortcutsSettings(raw ? JSON.parse(raw) : undefined);
  } catch {
    return normalizeShortcutsSettings(undefined);
  }
}

function createLocalSettingsScope(): SettingsScopeLike<ShortcutsSettings> {
  return {
    getSnapshot() {
      return localSettingsSnapshot;
    },
    subscribe(listener) {
      localSettingsListeners.add(listener);
      return () => {
        localSettingsListeners.delete(listener);
      };
    },
    async set(field, value) {
      // presets 为按 id 稀疏更新的记录，需要与现有值深合并，避免覆盖其他预设。
      const next =
        field === "presets" && value && typeof value === "object" && !Array.isArray(value)
          ? {
              ...localShortcutsSettings,
              presets: {
                ...localShortcutsSettings.presets,
                ...(value as Partial<Record<ShortcutPresetId, ShortcutPresetSettings>>),
              },
            }
          : { ...localShortcutsSettings, [field]: value };
      localShortcutsSettings = normalizeShortcutsSettings(next);
      if (typeof localStorage !== "undefined") {
        try {
          localStorage.setItem(SHORTCUTS_STORAGE_KEY, JSON.stringify(localShortcutsSettings));
        } catch {
          // 本地存储不可用时仍保留当前会话内的设置。
        }
      }
      localSettingsSnapshot = { status: "ready", value: localShortcutsSettings };
      for (const listener of localSettingsListeners) listener();
    },
  };
}

interface EscapeHint {
  seq: number;
  text: string;
}

let escapeHint: EscapeHint | null = null;
let escapeHintSeq = 0;
const escapeHintListeners = new Set<() => void>();

function showEscapeHint(text: string): void {
  escapeHintSeq += 1;
  escapeHint = { seq: escapeHintSeq, text };
  for (const listener of escapeHintListeners) listener();
}

function hideEscapeHint(): void {
  if (!escapeHint) return;
  escapeHint = null;
  for (const listener of escapeHintListeners) listener();
}

function getEscapeHint(): EscapeHint | null {
  return escapeHint;
}

function subscribeEscapeHint(listener: () => void): () => void {
  escapeHintListeners.add(listener);
  return () => {
    escapeHintListeners.delete(listener);
  };
}

function EscapeHintHost(): JSX.Element | null {
  const hint = useSyncExternalStore(subscribeEscapeHint, getEscapeHint, getEscapeHint);
  if (!hint) return null;
  return <Toast key={hint.seq} text={hint.text} onDone={hideEscapeHint} />;
}

interface SessionListSnapshot {
  current: string | undefined;
  byId: Record<string, { running: boolean }>;
}

interface SessionsLike {
  list: {
    getSnapshot(): SessionListSnapshot;
  };
  binding(sessionId: string):
    | {
        session: {
          cancel(): Promise<unknown>;
        };
      }
    | undefined;
}

interface WorkspacesLike {
  /** 新建会话（复用当前工作区空白会话或新建；无工作区时进入新建会话视图）。 */
  startSession(workspaceId?: string): void;
}

/**
 * client 面所需服务的本地结构类型。dsh client 服务（slots / locale / settingsScope /
 * connection / remote）由 dsh 生态注入，这里只声明本插件用到的面。
 */
interface ClientContextLike {
  effect(effect: () => unknown, label?: string): void;
  sessions: SessionsLike;
  workspaces: WorkspacesLike;
  locale: {
    bind(ns: string): Translate;
    register(ns: string, locale: string, dict: Record<string, string>): unknown;
  };
  slots: {
    inject(key: string, callback: () => unknown): unknown;
    register(options: unknown, component: unknown): unknown;
  };
}

/** 所需服务（cordis fiber inject）；client 面入口只注册设置节。 */
export const inject = ["slots", "locale", "sessions", "workspaces"];

function isCurrentConversationRunning(sessions: SessionsLike): boolean {
  const snapshot = sessions.list.getSnapshot();
  return Boolean(snapshot.current && snapshot.byId[snapshot.current]?.running);
}

function stopCurrentConversation(sessions: SessionsLike): void {
  if (!isCurrentConversationRunning(sessions)) return;
  const currentId = sessions.list.getSnapshot().current;
  if (!currentId) return;
  const binding = sessions.binding(currentId);
  if (!binding) return;
  void binding.session.cancel().catch((error: unknown) => {
    console.error("[shortcuts] 停止当前对话失败", error);
  });
}

/** 执行常用动作预设对应的网页侧动作。 */
function runPresetAction(presetId: ShortcutPresetId, ctx: ClientContextLike): void {
  switch (presetId) {
    case "toggleWindow": {
      const bridge = getBridge();
      if (!bridge) return;
      void bridge.windowAction("toggle-visible").catch((error: unknown) => {
        console.error("[shortcuts] 切换主窗口可见性失败", error);
      });
      break;
    }
    case "stopConversation":
      stopCurrentConversation(ctx.sessions);
      break;
    case "newConversation":
      ctx.workspaces.startSession();
      break;
  }
}

function StopShortcutSettings({
  scope,
  t,
}: {
  scope: SettingsScopeLike<ShortcutsSettings>;
  t: Translate;
}): JSX.Element {
  const snapshot = useSyncExternalStore(
    (listener) => scope.subscribe(listener),
    () => scope.getSnapshot(),
    () => scope.getSnapshot(),
  );
  const settings = snapshot.value ?? DEFAULT_SHORTCUTS_SETTINGS;
  const [error, setError] = useState<string | null>(null);

  const setEnabled = async (enabled: boolean) => {
    try {
      await scope.set("doubleEscapeStopEnabled", enabled);
      setError(null);
    } catch (e) {
      setError(`${t("stop.error")}: ${String(e)}`);
    }
  };

  const setTimeoutMs = async (raw: number) => {
    if (!Number.isFinite(raw)) return;
    const next = Math.min(5000, Math.max(200, Math.round(raw)));
    try {
      await scope.set("doubleEscapeStopTimeoutMs", next);
      setError(null);
    } catch (e) {
      setError(`${t("stop.error")}: ${String(e)}`);
    }
  };

  return (
    <div className={css.stopSettings}>
      <div className={css.settingRow}>
        <div className={css.settingText}>
          <span className={css.settingTitle}>{t("stop.title")}</span>
          <span className={css.settingDesc}>{t("stop.desc")}</span>
        </div>
        <label className={css.toggle}>
          <input
            id="shortcuts-stop-enabled"
            type="checkbox"
            checked={settings.doubleEscapeStopEnabled}
            onChange={(event) => void setEnabled(event.currentTarget.checked)}
          />
        </label>
      </div>
      <div className={css.timeoutRow}>
        <label htmlFor="shortcuts-stop-timeout">{t("stop.timeout")}</label>
        <Input
          className={css.timeoutInput}
          id="shortcuts-stop-timeout"
          type="number"
          min={200}
          max={5000}
          step={100}
          value={settings.doubleEscapeStopTimeoutMs}
          disabled={!settings.doubleEscapeStopEnabled}
          onChange={(event) => void setTimeoutMs(Number(event.currentTarget.value))}
        />
        <span className={css.unit}>{t("stop.timeoutUnit")}</span>
      </div>
      {error ? <p className={css.messageError}>{error}</p> : null}
    </div>
  );
}

function PresetShortcutInput({
  value,
  disabled,
  onCommit,
}: {
  value: string;
  disabled: boolean;
  onCommit: (value: string) => void;
}): JSX.Element {
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  return (
    <Input
      className={css.presetInput}
      value={draft}
      disabled={disabled}
      spellCheck={false}
      onChange={(event) => setDraft(event.currentTarget.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
      }}
      onBlur={() => onCommit(draft)}
    />
  );
}

/** 常用动作预设区块：把动作绑定到系统级全局快捷键（经壳侧注册并持久化）。 */
function PresetShortcuts({
  scope,
  t,
}: {
  scope: SettingsScopeLike<ShortcutsSettings>;
  t: Translate;
}): JSX.Element {
  const snapshot = useSyncExternalStore(
    (listener) => scope.subscribe(listener),
    () => scope.getSnapshot(),
    () => scope.getSnapshot(),
  );
  const settings = snapshot.value ?? DEFAULT_SHORTCUTS_SETTINGS;
  const [loaded, setLoaded] = useState(false);
  const [busyId, setBusyId] = useState<ShortcutPresetId | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // 装载后对齐：已启用但未注册的预设补注册（壳侧重启会从 config.json 恢复已注册项）。
  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) {
      setLoaded(true);
      return;
    }
    bridge.shortcuts
      .list()
      .then(async (items) => {
        if (disposed) return;
        setLoaded(true);
        const current = scope.getSnapshot().value ?? DEFAULT_SHORTCUTS_SETTINGS;
        const registered = new Set(items.map((item) => item.shortcut));
        for (const id of SHORTCUT_PRESET_IDS) {
          const preset = current.presets[id];
          if (!preset?.enabled || registered.has(preset.shortcut)) continue;
          try {
            await bridge.shortcuts.register(preset.shortcut);
          } catch (e) {
            console.error("[shortcuts] 预设快捷键注册失败", preset.shortcut, e);
          }
        }
      })
      .catch((e: unknown) => {
        if (!disposed) {
          setError(`${t("error.read")}: ${String(e)}`);
          setLoaded(true);
        }
      });
    return () => {
      disposed = true;
    };
  }, [scope.getSnapshot, t]);

  const togglePreset = async (id: ShortcutPresetId, enabled: boolean) => {
    const preset = settings.presets[id];
    if (!preset) return;
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    if (enabled) {
      const conflict = SHORTCUT_PRESET_IDS.some(
        (other) =>
          other !== id &&
          settings.presets[other]?.enabled &&
          settings.presets[other].shortcut === preset.shortcut,
      );
      if (conflict) {
        setError(t("preset.error.duplicate"));
        return;
      }
    }
    setBusyId(id);
    try {
      if (enabled) {
        await bridge.shortcuts.register(preset.shortcut);
      } else {
        await bridge.shortcuts.unregister(preset.shortcut);
      }
      await scope.set("presets", { [id]: { ...preset, enabled } });
      setError(null);
      setNotice(enabled ? t("preset.added") : t("preset.removed"));
    } catch (e) {
      setError(`${t(enabled ? "error.register" : "error.remove")}: ${String(e)}`);
    } finally {
      setBusyId(null);
    }
  };

  const updateShortcut = async (id: ShortcutPresetId, raw: string) => {
    const preset = settings.presets[id];
    if (!preset) return;
    const next = raw.trim();
    if (!next || next === preset.shortcut) return;
    const conflict = SHORTCUT_PRESET_IDS.some(
      (other) => other !== id && settings.presets[other]?.shortcut === next,
    );
    if (conflict) {
      setError(t("preset.error.duplicate"));
      return;
    }
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    setBusyId(id);
    let disposeNext: (() => void) | undefined;
    let oldRemoved = false;
    try {
      if (preset.enabled) {
        disposeNext = await bridge.shortcuts.register(next);
        try {
          await bridge.shortcuts.unregister(preset.shortcut);
          oldRemoved = true;
        } catch (error) {
          try {
            disposeNext();
          } catch {
            // 回滚失败时保留原始 unregister 错误。
          }
          throw error;
        }
      }
      try {
        await scope.set("presets", { [id]: { ...preset, shortcut: next } });
      } catch (error) {
        if (preset.enabled) {
          if (oldRemoved) {
            try {
              await bridge.shortcuts.register(preset.shortcut);
            } catch {
              // 回滚旧键失败时保留原始错误；设置区会提示用户检查系统快捷键。
            }
          }
          try {
            disposeNext?.();
          } catch {
            // 回滚失败时保留原始持久化错误。
          }
        }
        throw error;
      }
      setError(null);
      if (preset.enabled) setNotice(t("preset.updated"));
    } catch (e) {
      setError(`${t("error.register")}: ${String(e)}`);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className={css.presetList}>
      <div className={css.presetIntro}>
        <span className={css.settingTitle}>{t("preset.title")}</span>
        <span className={css.settingDesc}>{t("preset.desc")}</span>
      </div>
      {SHORTCUT_PRESET_IDS.map((id) => {
        const preset = settings.presets[id];
        if (!preset) return null;
        return (
          <div key={id} className={css.presetRow}>
            <div className={css.settingText}>
              <span className={css.settingTitle}>{t(`preset.${id}.title`)}</span>
              <span className={css.settingDesc}>{t(`preset.${id}.desc`)}</span>
            </div>
            <div className={css.presetControls}>
              <PresetShortcutInput
                value={preset.shortcut}
                disabled={busyId === id}
                onCommit={(value) => void updateShortcut(id, value)}
              />
              <span className={css.presetStatus}>
                {preset.enabled ? t("preset.status.on") : t("preset.status.off")}
              </span>
              <label className={css.toggle}>
                <input
                  type="checkbox"
                  checked={preset.enabled}
                  disabled={busyId === id}
                  onChange={(event) => void togglePreset(id, event.currentTarget.checked)}
                />
              </label>
            </div>
          </div>
        );
      })}
      {!loaded ? <p className={css.empty}>{t("status.loading")}</p> : null}
      {error ? <p className={css.messageError}>{error}</p> : null}
      {notice ? <p className={css.messageInfo}>{notice}</p> : null}
    </div>
  );
}

function ShortcutsPanel({
  scope,
  t,
}: {
  scope: SettingsScopeLike<ShortcutsSettings>;
  t: Translate;
}): JSX.Element {
  const settingsSnapshot = useSyncExternalStore(
    (listener) => scope.subscribe(listener),
    () => scope.getSnapshot(),
    () => scope.getSnapshot(),
  );
  const settings = settingsSnapshot.value ?? DEFAULT_SHORTCUTS_SETTINGS;
  const [shortcuts, setShortcuts] = useState<ShortcutSnapshot[]>([]);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      setLoading(false);
      return;
    }
    try {
      setShortcuts(await bridge.shortcuts.list());
      setError(null);
    } catch (e) {
      setError(`${t("error.read")}: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      setLoading(false);
      return;
    }
    bridge.shortcuts
      .list()
      .then((items) => {
        if (!disposed) {
          setShortcuts(items);
          setError(null);
        }
      })
      .catch((e: unknown) => {
        if (!disposed) setError(`${t("error.read")}: ${String(e)}`);
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [t]);

  const add = async () => {
    const shortcut = draft.trim();
    if (!shortcut || loading) return;
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      await bridge.shortcuts.register(shortcut);
      setDraft("");
      setNotice(t("added"));
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.register")}: ${String(e)}`);
    }
  };

  const remove = async (shortcut: string) => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      await bridge.shortcuts.unregister(shortcut);
      setNotice(t("removed"));
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.remove")}: ${String(e)}`);
    }
  };

  const clear = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    if (!window.confirm(t("clear.confirm"))) return;
    const previousPresets = Object.fromEntries(
      SHORTCUT_PRESET_IDS.map((id) => [id, settings.presets[id]]),
    ) as ShortcutsSettings["presets"];
    const disabledPresets = Object.fromEntries(
      SHORTCUT_PRESET_IDS.map((id) => [id, { ...settings.presets[id], enabled: false }]),
    ) as ShortcutsSettings["presets"];
    try {
      // 先停用并持久化预设，再注销系统快捷键：若注销失败，设置状态不会和
      // 下次装载时仍启用的预设冲突，用户可直接重试。
      await scope.set("presets", disabledPresets);
      try {
        await bridge.shortcuts.unregisterAll();
      } catch (error) {
        try {
          await scope.set("presets", previousPresets);
        } catch {
          // 回滚本地状态失败时保留原始错误。
        }
        throw error;
      }
      setNotice(t("cleared"));
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.clear")}: ${String(e)}`);
    }
  };

  return (
    <div className={css.stack}>
      <StopShortcutSettings scope={scope} t={t} />
      <PresetShortcuts scope={scope} t={t} />
      <div className={css.addRow}>
        <Input
          className={css.addInput}
          value={draft}
          placeholder={t("add.placeholder")}
          disabled={loading}
          onChange={(event) => {
            setDraft(event.currentTarget.value);
            setNotice(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") void add();
          }}
        />
        <Button
          type="button"
          icon={<IconPlusOutline16 />}
          disabled={loading || !draft.trim()}
          onClick={() => void add()}
        >
          {t("add.action")}
        </Button>
      </div>

      {shortcuts.length > 0 ? (
        <ul className={css.list} aria-label={t("nav")}>
          {shortcuts.map((item) => (
            <li key={item.shortcut} className={css.row}>
              <div className={css.rowText}>
                <span className={css.shortcutName}>{item.shortcut}</span>
                <span className={css.status}>{t("status.registered")}</span>
              </div>
              <div className={css.rowActions}>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  icon={<IconTrashOutline16 />}
                  onClick={() => void remove(item.shortcut)}
                >
                  {t("row.remove")}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className={css.empty}>{loading ? t("status.loading") : t("list.empty")}</p>
      )}

      {shortcuts.length > 0 ? (
        <div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            icon={<IconTrashOutline16 />}
            disabled={loading}
            onClick={() => void clear()}
          >
            {t("clear.action")}
          </Button>
        </div>
      ) : null}

      {error ? <p className={css.messageError}>{error}</p> : null}
      {notice ? <p className={css.messageInfo}>{notice}</p> : null}
    </div>
  );
}

/** 设置节入口：把“快捷键”注册到 dsh WebUI 设置面板。 */
export function apply(ctx: ClientContextLike): void {
  const NS = "settings.shortcuts";
  const t = ctx.locale.bind(NS);
  const settingsScope = createLocalSettingsScope();

  ctx.effect(() => {
    let lastEscapeAt = 0;
    let resetTimer: number | undefined;
    let settings = settingsScope.getSnapshot().value ?? DEFAULT_SHORTCUTS_SETTINGS;
    const unsubscribeSettings = settingsScope.subscribe(() => {
      settings = settingsScope.getSnapshot().value ?? DEFAULT_SHORTCUTS_SETTINGS;
      if (!settings.doubleEscapeStopEnabled) hideEscapeHint();
    });

    const onKeyDown = (event: KeyboardEvent) => {
      if (!settings.doubleEscapeStopEnabled) return;
      if (!isCurrentConversationRunning(ctx.sessions)) return;
      if (event.key !== "Escape" || event.repeat) return;
      const now = performance.now();
      if (now - lastEscapeAt <= settings.doubleEscapeStopTimeoutMs) {
        lastEscapeAt = 0;
        if (resetTimer !== undefined) window.clearTimeout(resetTimer);
        hideEscapeHint();
        stopCurrentConversation(ctx.sessions);
        return;
      }
      lastEscapeAt = now;
      if (resetTimer !== undefined) window.clearTimeout(resetTimer);
      showEscapeHint(t("doubleEscape.hint"));
      resetTimer = window.setTimeout(() => {
        lastEscapeAt = 0;
        hideEscapeHint();
        resetTimer = undefined;
      }, settings.doubleEscapeStopTimeoutMs);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      if (resetTimer !== undefined) window.clearTimeout(resetTimer);
      unsubscribeSettings();
    };
  }, "shortcuts: 双击 Esc 停止当前对话");

  // 常用动作预设：订阅全局快捷键按下并把字符串映射回对应动作。
  ctx.effect(() => {
    let settings = settingsScope.getSnapshot().value ?? DEFAULT_SHORTCUTS_SETTINGS;
    const unsubscribeSettings = settingsScope.subscribe(() => {
      settings = settingsScope.getSnapshot().value ?? DEFAULT_SHORTCUTS_SETTINGS;
    });
    const bridge = getBridge();
    if (!bridge) {
      return () => {
        unsubscribeSettings();
      };
    }
    let unlisten: (() => void) | undefined;
    void bridge
      .onShortcut((shortcut) => {
        const presetId = SHORTCUT_PRESET_IDS.find(
          (id) => settings.presets[id]?.enabled && settings.presets[id].shortcut === shortcut,
        );
        if (presetId) runPresetAction(presetId, ctx);
      })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch((error: unknown) => {
        console.error("[shortcuts] 全局快捷键分发监听失败", error);
      });
    return () => {
      unlisten?.();
      unsubscribeSettings();
    };
  }, "shortcuts: 常用动作全局快捷键分发");

  ctx.effect(
    () =>
      ctx.locale.register(NS, "zh", {
        nav: "快捷键",
        "nav.desc": "管理桌面全局快捷键",
        "add.placeholder": "CmdOrCtrl+Shift+D",
        "add.action": "添加",
        "list.empty": "暂无已注册快捷键",
        "status.registered": "已注册",
        "status.loading": "正在读取…",
        "row.remove": "移除",
        "clear.action": "全部移除",
        "clear.confirm": "确认移除全部快捷键？",
        added: "快捷键已注册",
        removed: "快捷键已移除",
        cleared: "快捷键已全部移除",
        "error.bridge": "桌面壳桥接不可用",
        "error.read": "读取快捷键失败",
        "error.register": "注册快捷键失败",
        "error.remove": "移除快捷键失败",
        "error.clear": "移除快捷键失败",
        "stop.title": "停止当前对话",
        "stop.desc": "连按两次 Esc 停止当前对话",
        "stop.timeout": "判定窗口",
        "stop.timeoutUnit": "毫秒",
        "stop.error": "保存快捷键设置失败",
        "doubleEscape.hint": "再次按 Esc 终止当前对话",
        "preset.title": "常用动作",
        "preset.desc": "把常用操作绑定到系统级全局快捷键，任意应用中按下即生效",
        "preset.toggleWindow.title": "显示 / 隐藏主窗口",
        "preset.toggleWindow.desc": "在任意应用中切换主窗口的显示与隐藏",
        "preset.stopConversation.title": "停止当前对话",
        "preset.stopConversation.desc": "停止 dsh 当前正在运行的对话",
        "preset.newConversation.title": "新建对话",
        "preset.newConversation.desc": "打开 dsh 并新建一个对话",
        "preset.added": "预设快捷键已启用",
        "preset.removed": "预设快捷键已停用",
        "preset.updated": "预设快捷键已更新",
        "preset.error.duplicate": "该快捷键已分配给其他动作",
        "preset.status.on": "已启用",
        "preset.status.off": "未启用",
      }),
    "bridge: 快捷键中文字典",
  );
  ctx.effect(
    () =>
      ctx.locale.register(NS, "en", {
        nav: "Shortcuts",
        "nav.desc": "Manage desktop global shortcuts",
        "add.placeholder": "CmdOrCtrl+Shift+D",
        "add.action": "Add",
        "list.empty": "No shortcuts registered",
        "status.registered": "Registered",
        "status.loading": "Loading…",
        "row.remove": "Remove",
        "clear.action": "Remove all",
        "clear.confirm": "Remove all shortcuts?",
        added: "Shortcut registered",
        removed: "Shortcut removed",
        cleared: "All shortcuts removed",
        "error.bridge": "Desktop bridge unavailable",
        "error.read": "Failed to read shortcuts",
        "error.register": "Failed to register shortcut",
        "error.remove": "Failed to remove shortcut",
        "error.clear": "Failed to remove shortcuts",
        "stop.title": "Stop current conversation",
        "stop.desc": "Press Esc twice to stop the current conversation",
        "stop.timeout": "Detection window",
        "stop.timeoutUnit": "ms",
        "stop.error": "Failed to save shortcut settings",
        "doubleEscape.hint": "Press Esc again to stop",
        "preset.title": "Common actions",
        "preset.desc": "Bind common actions to system-wide shortcuts, active in any app",
        "preset.toggleWindow.title": "Show / hide main window",
        "preset.toggleWindow.desc": "Toggle main window visibility from any app",
        "preset.stopConversation.title": "Stop current conversation",
        "preset.stopConversation.desc": "Stop the running dsh conversation",
        "preset.newConversation.title": "New conversation",
        "preset.newConversation.desc": "Open dsh and start a new conversation",
        "preset.added": "Preset shortcut enabled",
        "preset.removed": "Preset shortcut disabled",
        "preset.updated": "Preset shortcut updated",
        "preset.error.duplicate": "This shortcut is already assigned to another action",
        "preset.status.on": "Enabled",
        "preset.status.off": "Disabled",
      }),
    "shortcuts: English dictionary",
  );

  ctx.slots.inject("shell.overlay", () =>
    (() => {
      try {
        return ctx.slots.register(
          {
            name: "shell.overlay",
            id: "shortcuts-escape-hint",
            order: 0,
          },
          () => <EscapeHintHost />,
        );
      } catch (error) {
        console.error("[shortcuts] shell.overlay 注册失败", error);
        return () => {};
      }
    })(),
  );

  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "shortcuts",
        order: 80,
        label: () => t("nav"),
        locale: NS,
        children: {},
      },
      () => (
        <SettingsPage>
          <SettingsSection
            headingId="shortcuts-heading"
            title={t("nav")}
            description={t("nav.desc")}
          >
            <ShortcutsPanel scope={settingsScope} t={t} />
          </SettingsSection>
        </SettingsPage>
      ),
    ),
  );
}
