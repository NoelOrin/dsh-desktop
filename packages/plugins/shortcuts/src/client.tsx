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
import { normalizeShortcutsSettings, SHORTCUTS_STORAGE_KEY } from "./shared/settings";

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
      localShortcutsSettings = normalizeShortcutsSettings({
        ...localShortcutsSettings,
        [field]: value,
      });
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

/**
 * client 面所需服务的本地结构类型。dsh client 服务（slots / locale / settingsScope /
 * connection / remote）由 dsh 生态注入，这里只声明本插件用到的面。
 */
interface ClientContextLike {
  effect(effect: () => unknown, label?: string): void;
  sessions: SessionsLike;
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
export const inject = ["slots", "locale", "sessions"];

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

function ShortcutsPanel({
  scope,
  t,
}: {
  scope: SettingsScopeLike<ShortcutsSettings>;
  t: Translate;
}): JSX.Element {
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
    try {
      await bridge.shortcuts.unregisterAll();
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
