/** 插件管理设置节：远程插件分组预设、已安装插件、安装/更新/移除与同步输出。 */
import {
  Button,
  IconDownloadOutline16,
  IconLinkOutline16,
  IconPlusOutline16,
  IconRefreshOutline16,
  IconTrashOutline16,
  Input,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { useEffect, useState } from "react";
import css from "./desktop.module.css";
import {
  getBridge,
  type InstalledPluginSummary,
  type PluginOperationResult,
  type RemotePluginPreset,
  type Translate,
} from "./runtime";
import { SettingsPage, SettingsSection } from "./settings-layout";
import { ToggleField } from "./ui/controls";

const MAX_OUTPUT = 120;
const DEFAULT_GROUP = "default";

function makePresetId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function groupPresets(presets: RemotePluginPreset[]): Array<[string, RemotePluginPreset[]]> {
  const groups = new Map<string, RemotePluginPreset[]>();
  for (const preset of presets) {
    const group = preset.group || DEFAULT_GROUP;
    const list = groups.get(group) ?? [];
    list.push(preset);
    groups.set(group, list);
  }
  return Array.from(groups.entries());
}

export function PluginPanel({ t }: { t: Translate }): JSX.Element {
  const [presets, setPresets] = useState<RemotePluginPreset[]>([]);
  const [installed, setInstalled] = useState<InstalledPluginSummary[]>([]);
  const [activeProfile, setActiveProfile] = useState<string | null>(null);
  const [packageSpec, setPackageSpec] = useState("");
  const [url, setUrl] = useState("");
  const [group, setGroup] = useState("");
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [output, setOutput] = useState<string[]>([]);

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      setLoaded(true);
      return;
    }
    Promise.all([
      bridge.remotePlugins.list(),
      bridge.plugins.installed(),
      bridge.profiles.active(),
    ])
      .then(([presetList, installedList, profileState]) => {
        if (disposed) return;
        setPresets(presetList);
        setInstalled(installedList);
        setActiveProfile(profileState.active);
      })
      .catch((e: unknown) => {
        if (!disposed) setError(`读取插件状态失败: ${String(e)}`);
      })
      .finally(() => {
        if (!disposed) setLoaded(true);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const refreshInstalled = async () => {
    const bridge = getBridge();
    if (!bridge) return;
    setInstalled(await bridge.plugins.installed());
  };

  const persistPresets = async (next: RemotePluginPreset[]): Promise<boolean> => {
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      return false;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await bridge.remotePlugins.save(next);
      setPresets(saved);
      setNotice(t("presets.saved"));
      return true;
    } catch (e) {
      setError(`保存远程插件预设失败: ${String(e)}`);
      return false;
    } finally {
      setBusy(false);
    }
  };

  const addPreset = async () => {
    const nextUrl = url.trim();
    if (!nextUrl) {
      setError(t("presets.urlRequired"));
      return;
    }
    if (presets.some((preset) => preset.url === nextUrl)) {
      setError(t("presets.duplicate"));
      return;
    }
    const nextGroup = group.trim() || DEFAULT_GROUP;
    const ok = await persistPresets([
      ...presets,
      {
        id: makePresetId(),
        url: nextUrl,
        enabled: true,
        group: nextGroup,
        source: "local",
      },
    ]);
    if (ok) {
      setUrl("");
      setGroup("");
    }
  };

  const togglePreset = async (id: string, enabled: boolean) => {
    await persistPresets(
      presets.map((preset) =>
        preset.id === id && preset.source === "local" ? { ...preset, enabled } : preset,
      ),
    );
  };

  const removePreset = async (id: string) => {
    await persistPresets(presets.filter((preset) => preset.id !== id || preset.source !== "local"));
  };

  const toggleGroup = async (groupName: string, enabled: boolean) => {
    await persistPresets(
      presets.map((preset) =>
        preset.group === groupName && preset.source === "local" ? { ...preset, enabled } : preset,
      ),
    );
  };

  const removeGroup = async (groupName: string) => {
    await persistPresets(
      presets.filter((preset) => preset.group !== groupName || preset.source !== "local"),
    );
  };

  const runOperation = async (
    operation: () => Promise<PluginOperationResult>,
    successMessage: string,
  ): Promise<boolean> => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("operation.unavailable"));
      return false;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    setOutput([]);
    try {
      const result = await operation();
      setOutput(result.output.slice(-MAX_OUTPUT));
      if (result.ok) {
        setNotice(successMessage);
        await refreshInstalled();
        return true;
      } else {
        setError(`${t("operation.failed")}（exit ${result.exit_code ?? "?"}）`);
        return false;
      }
    } catch (e) {
      const message = String(e);
      setError(
        message.includes("当前只有一个插件操作可运行")
          ? t("operation.busy")
          : `${t("operation.failed")}: ${message}`,
      );
      return false;
    } finally {
      setBusy(false);
    }
  };

  const installPackage = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("operation.unavailable"));
      return;
    }
    const spec = packageSpec.trim();
    if (!spec) return;
    const ok = await runOperation(() => bridge.plugins.install(spec), t("install.success"));
    if (ok) setPackageSpec("");
  };

  const runSync = async (groupName?: string) => {
    const bridge = getBridge();
    if (!bridge) {
      setError("桌面壳桥接不可用");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    setOutput([]);
    try {
      const results = await bridge.plugins.sync(groupName);
      setOutput(results.flatMap((result) => result.output).slice(-MAX_OUTPUT));
      const failed = results.filter((result) => !result.ok).length;
      if (failed === 0) {
        setNotice(groupName ? t("presets.syncedGroup") : t("presets.synced"));
      } else {
        setError(`${failed} 个远程插件同步失败`);
      }
      await refreshInstalled();
    } catch (e) {
      setError(`同步远程插件失败: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsPage>
      <SettingsSection
        headingId="plugins-install-heading"
        title={t("install.title")}
        description={t("install.desc")}
      >
        <div className={css.toolText}>
          <span className={css.pluginMeta}>
            {t("install.profile")}: {activeProfile ?? "..."}
          </span>
        </div>
        <div className={css.formActions}>
          <Input
            className={css.input}
            id="plugin-package-spec"
            value={packageSpec}
            placeholder={t("install.placeholder")}
            disabled={busy}
            onChange={(event) => {
              setPackageSpec(event.currentTarget.value);
              setError(null);
            }}
          />
          <Button
            type="button"
            icon={<IconPlusOutline16 />}
            disabled={busy || !packageSpec.trim()}
            onClick={() => void installPackage()}
          >
            {t("install.button")}
          </Button>
        </div>
        <div className={css.toolActions}>
          <Button
            type="button"
            variant="outline"
            icon={<IconRefreshOutline16 />}
            disabled={busy}
            onClick={() => {
              const bridge = getBridge();
              if (bridge) void bridge.restart();
            }}
          >
            {t("restart")}
          </Button>
        </div>
        {busy ? <p className={css.messageInfo}>{t("operation.running")}</p> : null}
      </SettingsSection>
      <SettingsSection
        headingId="plugins-presets-heading"
        title={t("presets.title")}
        description={t("presets.desc")}
        actions={
          <Button
            type="button"
            variant="outline"
            icon={<IconDownloadOutline16 />}
            disabled={busy || !loaded}
            onClick={() => void runSync()}
          >
            {t("presets.sync")}
          </Button>
        }
      >
        <div className={css.block}>
          <div className={css.formActions}>
            <Input
              className={css.input}
              id="remote-plugin-url"
              value={url}
              placeholder="https://...tgz 或 git URL"
              disabled={busy}
              onChange={(event) => {
                setUrl(event.currentTarget.value);
                setError(null);
              }}
            />
            <Input
              className={css.input}
              id="remote-plugin-group"
              value={group}
              placeholder={t("presets.groupPlaceholder")}
              disabled={busy}
              onChange={(event) => {
                setGroup(event.currentTarget.value);
                setError(null);
              }}
            />
            <Button
              type="button"
              icon={<IconPlusOutline16 />}
              disabled={busy || !url.trim()}
              onClick={() => void addPreset()}
            >
              {t("presets.add")}
            </Button>
          </div>
          {presets.length > 0 ? (
            <div className={css.toolList}>
              {groupPresets(presets).map(([groupName, groupItems]) => {
                const localCount = groupItems.filter((preset) => preset.source === "local").length;
                const localEnabled = groupItems.filter(
                  (preset) => preset.source === "local" && preset.enabled,
                ).length;
                const externalCount = groupItems.length - localCount;
                const groupChecked = localCount > 0 && localEnabled === localCount;
                return (
                  <section key={groupName} className={css.pluginGroup}>
                    <div className={css.groupHead}>
                      <div className={css.toolText}>
                        <span className={css.groupTitle}>{groupName}</span>
                        <span className={css.groupMeta}>
                          {groupItems.length} 项
                          {externalCount > 0 ? ` · ${t("presets.external")} ${externalCount}` : ""}
                        </span>
                      </div>
                      <div className={css.toolActions}>
                        <Button
                          type="button"
                          variant="outline"
                          icon={<IconDownloadOutline16 />}
                          disabled={busy}
                          onClick={() => void runSync(groupName)}
                        >
                          {t("presets.syncGroup")}
                        </Button>
                        {localCount > 0 ? (
                          <ToggleField
                            id={`remote-group-${groupName}`}
                            checked={groupChecked}
                            disabled={busy}
                            onChange={(next) => void toggleGroup(groupName, next)}
                            title={t("presets.groupEnabled")}
                          />
                        ) : null}
                        {localCount > 0 ? (
                          <Button
                            type="button"
                            variant="ghost"
                            icon={<IconTrashOutline16 />}
                            disabled={busy}
                            onClick={() => void removeGroup(groupName)}
                          >
                            {t("presets.removeGroup")}
                          </Button>
                        ) : null}
                      </div>
                    </div>
                    <div className={css.toolList}>
                      {groupItems.map((preset) => {
                        const external = preset.source === "external";
                        return (
                          <div key={preset.id} className={css.remoteRow}>
                            <code className={css.remoteUrl}>
                              <IconLinkOutline16 />
                              <span>{preset.url}</span>
                            </code>
                            <div className={css.presetBadges}>
                              {external ? (
                                <span className={css.pluginBadge}>{t("presets.external")}</span>
                              ) : null}
                            </div>
                            <ToggleField
                              id={`remote-preset-${preset.id}`}
                              checked={preset.enabled}
                              disabled={busy || external}
                              onChange={(next) => void togglePreset(preset.id, next)}
                              title={preset.enabled ? t("presets.enabled") : t("presets.disabled")}
                            />
                            <Button
                              type="button"
                              variant="ghost"
                              icon={<IconTrashOutline16 />}
                              disabled={busy || external}
                              onClick={() => void removePreset(preset.id)}
                            >
                              {t("presets.remove")}
                            </Button>
                          </div>
                        );
                      })}
                    </div>
                  </section>
                );
              })}
            </div>
          ) : (
            <p className={css.messageInfo}>{t("presets.empty")}</p>
          )}
        </div>
      </SettingsSection>
      <SettingsSection
        headingId="plugins-installed-heading"
        title={t("installed.title")}
        description={t("installed.desc")}
        actions={
          <Button
            type="button"
            variant="outline"
            icon={<IconRefreshOutline16 />}
            disabled={busy || !loaded}
            onClick={() => {
              const bridge = getBridge();
              if (bridge) {
                void runOperation(() => bridge.plugins.update(), t("installed.updated"));
              }
            }}
          >
            {t("installed.update")}
          </Button>
        }
      >
        {installed.length > 0 ? (
          <div className={css.toolList}>
            {installed.map((plugin) => (
              <div key={plugin.name} className={css.pluginRow}>
                <div className={css.toolText}>
                  <span className={css.pluginName}>{plugin.name}</span>
                  <span className={css.pluginMeta}>
                    {plugin.version ? <code>{plugin.version}</code> : null}
                    <span className={plugin.bundle ? css.pluginBadge : undefined}>
                      {plugin.bundle ? t("installed.bundle") : t("installed.dependency")}
                    </span>
                  </span>
                </div>
                <div className={css.toolActions}>
                  <Button
                    type="button"
                    variant="ghost"
                    icon={<IconTrashOutline16 />}
                    disabled={busy}
                    onClick={() => {
                      const bridge = getBridge();
                      if (bridge) {
                        void runOperation(
                          () => bridge.plugins.remove(plugin.name),
                          t("installed.removed"),
                        );
                      }
                    }}
                  >
                    {t("installed.remove")}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p className={css.messageInfo}>{t("installed.empty")}</p>
        )}
      </SettingsSection>
      {output.length > 0 ? (
        <SettingsSection
          headingId="plugins-output-heading"
          title={t("output.title")}
          description={t("output.desc")}
        >
          <pre className={css.log}>{output.join("\n")}</pre>
        </SettingsSection>
      ) : null}
      {notice ? <p className={css.messageInfo}>{notice}</p> : null}
      {error ? <p className={css.messageError}>{error}</p> : null}
    </SettingsPage>
  );
}
