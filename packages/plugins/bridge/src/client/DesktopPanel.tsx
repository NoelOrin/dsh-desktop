/** 桌面设置节：运行状态、路径配置、维护工具与开机自启。 */
import {
  Button,
  IconDownloadOutline16,
  IconFolderOpenOutline16,
  IconRefreshOutline16,
  Input,
  StateDot,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { useEffect, useRef, useState } from "react";
import css from "./desktop.module.css";
import {
  type DshConfig,
  getBridge,
  type RuntimeSnapshot,
  type StartupMode,
  type Translate,
} from "./runtime";
import { SettingsPage, SettingsSection } from "./settings-layout";
import { SegmentedField, type SegmentOption, ToggleField } from "./ui/controls";

const MAX_LOGS = 500;
const SHOW_LOGS = new Set(["installing", "starting", "failed"]);

function StartupSettings({ t }: { t: Translate }): JSX.Element {
  const [autostart, setAutostart] = useState(false);
  const [startupMode, setStartupMode] = useState<StartupMode>("normal");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      setLoading(false);
      return;
    }
    bridge.desktop
      .get()
      .then((settings) => {
        if (disposed) return;
        setAutostart(settings.autostart);
        setStartupMode(settings.startup_mode);
      })
      .catch((e: unknown) => setError(`读取系统自启状态失败: ${String(e)}`))
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const apply = async (nextAutostart: boolean, nextMode: StartupMode) => {
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      return;
    }
    try {
      await bridge.desktop.set({
        autostart: nextAutostart,
        startup_mode: nextMode,
      });
      setAutostart(nextAutostart);
      setStartupMode(nextMode);
      setError(null);
    } catch (e) {
      setError(`系统自启设置失败: ${String(e)}`);
    }
  };

  const modeOptions: Array<SegmentOption<StartupMode>> = [
    { value: "normal", label: t("autostart.mode.normal") },
    { value: "tray", label: t("autostart.mode.tray") },
    { value: "minimized", label: t("autostart.mode.minimized") },
  ];

  return (
    <div className={css.startupStack}>
      <ToggleField
        id="startup-enabled"
        checked={autostart}
        disabled={loading}
        onChange={(next) => void apply(next, startupMode)}
        title={t("autostart.title")}
        description={t("autostart.desc")}
      />
      <div className={css.modeBlock}>
        <SegmentedField<StartupMode>
          label={t("autostart.mode")}
          value={startupMode}
          options={modeOptions}
          disabled={loading}
          onChange={(nextMode) => void apply(autostart, nextMode)}
        />
      </div>
      {error ? <p className={css.messageError}>{error}</p> : null}
    </div>
  );
}

function StatusPanel({ t }: { t: Translate }): JSX.Element {
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const logsRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) return;
    bridge
      .getStatus()
      .then((s) => {
        if (disposed) return;
        setSnapshot(s);
        setLogs((s.logs ?? []).slice(-MAX_LOGS));
      })
      .catch((error: unknown) => {
        if (disposed) return;
        setSnapshot({
          phase: "failed",
          message: `无法读取运行状态: ${String(error)}`,
          url: null,
          dsh_installed: false,
          node_found: false,
          log_dir: null,
          logs: [],
        });
      });
    const un1 = bridge.onStatus((s) => {
      if (!disposed) setSnapshot(s);
    });
    const un2 = bridge.onLog((line) => {
      if (!disposed) setLogs((prev) => [...prev, line].slice(-MAX_LOGS));
    });
    return () => {
      disposed = true;
      void un1.then((u) => u()).catch(() => {});
      void un2.then((u) => u()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (logs.length && logsRef.current) {
      logsRef.current.scrollTop = logsRef.current.scrollHeight;
    }
  }, [logs]);

  const phase = snapshot?.phase ?? "detecting";
  const showLogs = SHOW_LOGS.has(phase);
  const phaseState =
    phase === "ready"
      ? "done"
      : phase === "failed"
        ? "error"
        : phase === "missing"
          ? "warning"
          : "ongoing";
  const phaseLabel = t(`status.phase.${phase}`) || phase;

  return (
    <div>
      <div className={css.statusSummary}>
        <StateDot state={phaseState} size={12} />
        <div className={css.statusMeta}>
          <div className={css.statusLine}>
            <span className={css.statusText}>{snapshot?.message ?? t("status.detecting")}</span>
            {snapshot?.url ? <code className={css.url}>{snapshot.url}</code> : null}
          </div>
          <fieldset className={css.metaRow} aria-label={t("status.detail")}>
            <span className={css.metaItem}>
              {t("status.phase")}
              <strong>{phaseLabel}</strong>
            </span>
            {snapshot ? (
              <span className={css.metaItem}>
                {t("status.node")}
                <strong>
                  {snapshot.node_found ? t("status.nodeFound") : t("status.nodeMissing")}
                </strong>
              </span>
            ) : null}
            {snapshot?.log_dir ? (
              <span className={css.metaItem}>
                {t("status.logs")}
                <strong>{logs.length}</strong>
              </span>
            ) : null}
          </fieldset>
        </div>
      </div>
      <div className={css.actions}>
        {phase === "missing" && snapshot?.node_found !== false ? (
          <Button
            type="button"
            variant="outline"
            icon={<IconDownloadOutline16 />}
            onClick={() =>
              void getBridge()
                ?.installDsh()
                .catch((e: unknown) => console.error(e))
            }
          >
            {t("status.install")}
          </Button>
        ) : null}
        {phase === "failed" ? (
          <>
            <Button
              type="button"
              variant="outline"
              icon={<IconRefreshOutline16 />}
              onClick={() =>
                void getBridge()
                  ?.restart()
                  .catch((e: unknown) => console.error(e))
              }
            >
              {t("status.retry")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              icon={<IconFolderOpenOutline16 />}
              onClick={() =>
                void getBridge()
                  ?.openLogDirectory()
                  .catch((e: unknown) => console.error(e))
              }
            >
              {t("status.openLogs")}
            </Button>
          </>
        ) : null}
      </div>
      {showLogs ? (
        <pre ref={logsRef} className={css.log}>
          {logs.length > 0 ? logs.join("\n") : t("status.noLogs")}
        </pre>
      ) : null}
    </div>
  );
}

function ConfigPanel({ t }: { t: Translate }): JSX.Element {
  const [dshBin, setDshBin] = useState("");
  const [dshNode, setDshNode] = useState("");
  const [dshHome, setDshHome] = useState("");
  const dshShortcutsRef = useRef<string[]>([]);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    getBridge()
      ?.getConfig()
      .then((c) => {
        if (disposed) return;
        setDshBin(c.dsh_bin ?? "");
        setDshNode(c.dsh_node ?? "");
        setDshHome(c.dsh_home ?? "");
        dshShortcutsRef.current = c.shortcuts ?? [];
      })
      .catch((e: unknown) => {
        if (disposed) return;
        setError(`读取配置失败: ${String(e)}`);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const save = async () => {
    const config: DshConfig = {
      dsh_bin: dshBin || null,
      dsh_node: dshNode || null,
      dsh_home: dshHome || null,
      shortcuts: dshShortcutsRef.current,
    };
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      return;
    }
    try {
      await bridge.setConfig(config);
      setSaved(true);
      setError(null);
    } catch (e) {
      setSaved(false);
      setError(`保存失败: ${String(e)}`);
    }
  };

  const fields: Array<{
    id: string;
    label: string;
    env: string;
    placeholder: string;
    value: string;
    onChange: (value: string) => void;
  }> = [
    {
      id: "desktop-dsh-bin",
      label: "DSH 入口",
      env: "DSH_BIN",
      placeholder: "例如 /path/to/dsh",
      value: dshBin,
      onChange: setDshBin,
    },
    {
      id: "desktop-dsh-node",
      label: "Node 解释器",
      env: "DSH_NODE",
      placeholder: "例如 /path/to/node",
      value: dshNode,
      onChange: setDshNode,
    },
    {
      id: "desktop-dsh-home",
      label: "Harness 数据目录",
      env: "DSH_HOME",
      placeholder: "留空则继承环境变量",
      value: dshHome,
      onChange: setDshHome,
    },
  ];

  return (
    <div className={css.block}>
      <div className={css.configGrid}>
        {fields.map((field) => (
          <label key={field.id} className={css.field} htmlFor={field.id}>
            <span className={css.fieldLabel}>
              <span>{field.label}</span>
              <code>{field.env}</code>
            </span>
            <Input
              className={css.input}
              id={field.id}
              value={field.value}
              placeholder={field.placeholder}
              onChange={(event) => {
                field.onChange(event.currentTarget.value);
                setSaved(false);
                setError(null);
              }}
            />
          </label>
        ))}
      </div>
      <div className={css.formActions}>
        <Button type="button" onClick={() => void save()}>
          {t("config.save")}
        </Button>
        <Button
          type="button"
          variant="outline"
          icon={<IconRefreshOutline16 />}
          onClick={() =>
            void getBridge()
              ?.restart()
              .catch((e: unknown) => console.error(e))
          }
        >
          {t("config.restart")}
        </Button>
      </div>
      {error ? <p className={css.messageError}>{error}</p> : null}
      {saved ? <p className={css.messageInfo}>{t("config.saved")}</p> : null}
    </div>
  );
}

function ToolsPanel({ t }: { t: Translate }): JSX.Element {
  const [msg, setMsg] = useState<string | null>(null);

  const doCheckUpdate = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setMsg("桌面壳桥接不可用");
      return;
    }
    try {
      const version = await bridge.update.check();
      if (!version) {
        setMsg("当前已是最新版本");
        return;
      }
      if (window.confirm(`发现新版本 ${version}，是否静默下载？`)) {
        setMsg("正在下载更新，请稍候…");
        const path = await bridge.update.install();
        setMsg(`更新已下载：${path}`);
        if (window.confirm("更新已下载到本地，是否打开安装包？")) {
          await bridge.openExternal(path);
          setMsg(`已打开安装包：${path}`);
        } else {
          setMsg(`更新已下载（未打开）：${path}`);
        }
      }
    } catch (e) {
      setMsg(`检查/下载更新失败: ${String(e)}`);
    }
  };

  return (
    <div className={css.toolList}>
      <div className={css.toolRow}>
        <div className={css.toolText}>
          <span className={css.toolTitle}>{t("tools.openLogs.title")}</span>
          <span className={css.toolDesc}>{t("tools.openLogs.desc")}</span>
        </div>
        <div className={css.toolActions}>
          <Button
            type="button"
            variant="outline"
            icon={<IconFolderOpenOutline16 />}
            onClick={() =>
              void getBridge()
                ?.openLogDirectory()
                .catch((e: unknown) => console.error(e))
            }
          >
            {t("tools.openLogs.action")}
          </Button>
        </div>
      </div>
      <div className={css.toolRow}>
        <div className={css.toolText}>
          <span className={css.toolTitle}>{t("tools.checkUpdate.title")}</span>
          <span className={css.toolDesc}>{t("tools.checkUpdate.desc")}</span>
        </div>
        <div className={css.toolActions}>
          <Button
            type="button"
            variant="outline"
            icon={<IconDownloadOutline16 />}
            onClick={() => void doCheckUpdate()}
          >
            {t("tools.checkUpdate.action")}
          </Button>
        </div>
      </div>
      {msg ? <p className={css.messageInfo}>{msg}</p> : null}
    </div>
  );
}

export function DesktopPanel({ t }: { t: Translate }): JSX.Element {
  return (
    <SettingsPage>
      <SettingsSection
        headingId="desktop-status-heading"
        title={t("nav.status")}
        description={t("nav.status.desc")}
      >
        <StatusPanel t={t} />
      </SettingsSection>
      <SettingsSection
        headingId="desktop-config-heading"
        title={t("nav.config")}
        description={t("nav.config.desc")}
      >
        <ConfigPanel t={t} />
      </SettingsSection>
      <SettingsSection
        headingId="desktop-tools-heading"
        title={t("nav.tools")}
        description={t("nav.tools.desc")}
      >
        <ToolsPanel t={t} />
      </SettingsSection>
      <SettingsSection
        headingId="desktop-autostart-heading"
        title={t("nav.autostart")}
        description={t("nav.autostart.desc")}
      >
        <StartupSettings t={t} />
      </SettingsSection>
    </SettingsPage>
  );
}
