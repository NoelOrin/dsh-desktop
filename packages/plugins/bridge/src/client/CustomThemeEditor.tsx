/** 自定义主题：创建/复制/编辑/导入导出，编辑时实时预览。 */

import {
  Button,
  IconCopyOutline16,
  IconDownloadOutline16,
  IconEditOutline16,
  IconFolderOpenOutline16,
  IconPlusOutline16,
  IconTrashOutline16,
  Input,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { useRef, useState, useSyncExternalStore } from "react";
import {
  duplicateThemeFamily,
  ensureUniqueThemeId,
  getReservedThemeIds,
  normalizeImportedThemeFamily,
  parseThemeFamilyJson,
  replaceCustomTheme,
  serializeThemeFamily,
  slugifyThemeId,
  type ThemeFamily,
  type ThemeSeeds,
} from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { sliderFillStyle } from "./slider";
import type { ThemeStore } from "./theme-store";

function blankSeeds(): ThemeSeeds {
  return { accent: "#4176e6", background: "#ffffff", foreground: "#0f1115", contrast: 46 };
}

function blankFamily(name: string, id: string): ThemeFamily {
  return { id, name, origin: "custom", light: blankSeeds(), dark: blankSeeds() };
}

const SEED_FIELDS: Array<"accent" | "background" | "foreground"> = [
  "accent",
  "background",
  "foreground",
];

export function CustomThemeEditor({
  store,
  t,
}: {
  store: ThemeStore;
  t: (key: string) => string;
}): JSX.Element {
  const settings = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
  );
  const [draft, setDraft] = useState<ThemeFamily | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const existingIds = new Set([
    ...getReservedThemeIds(),
    ...settings.customThemes.map((item) => item.id),
  ]);

  const save = (family: ThemeFamily): void => {
    store.setCustomThemes(replaceCustomTheme(settings.customThemes, family));
  };

  const create = (): void => {
    const id = ensureUniqueThemeId(slugifyThemeId(t("custom.newName")), existingIds);
    setDraft(blankFamily(t("custom.newName"), id));
  };

  const exportFamily = (family: ThemeFamily): void => {
    const blob = new Blob([serializeThemeFamily(family)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${family.id}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const importFamily = (file: File | undefined): void => {
    if (file === undefined) return;
    void file.text().then((raw) => {
      try {
        const family = normalizeImportedThemeFamily(parseThemeFamilyJson(raw), existingIds);
        save(family);
      } catch {
        // 非法 JSON 静默失败
      }
    });
  };

  return (
    <SettingsSection
      title={t("custom.title")}
      actions={
        <div className={css.actions}>
          <Button type="button" variant="outline" icon={<IconPlusOutline16 />} onClick={create}>
            {t("custom.create")}
          </Button>
          <Button
            type="button"
            variant="outline"
            icon={<IconFolderOpenOutline16 />}
            onClick={() => fileRef.current?.click()}
          >
            {t("custom.import")}
          </Button>
          <input
            ref={fileRef}
            type="file"
            accept="application/json,.json"
            hidden
            onChange={(event) => {
              importFamily(event.currentTarget.files?.[0]);
              event.currentTarget.value = "";
            }}
          />
        </div>
      }
    >
      {settings.customThemes.map((family) => (
        <div key={family.id} className={css.themeRow}>
          <span className={css.themeRowName}>{family.name}</span>
          <div className={css.themeRowActions}>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              icon={<IconEditOutline16 />}
              aria-label={t("custom.edit")}
              title={t("custom.edit")}
              onClick={() => setDraft(family)}
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              icon={<IconCopyOutline16 />}
              aria-label={t("custom.copy")}
              title={t("custom.copy")}
              onClick={() => {
                const dup = duplicateThemeFamily(family, existingIds);
                save(dup);
              }}
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              icon={<IconDownloadOutline16 />}
              aria-label={t("custom.export")}
              title={t("custom.export")}
              onClick={() => exportFamily(family)}
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              icon={<IconTrashOutline16 />}
              aria-label={t("custom.remove")}
              title={t("custom.remove")}
              onClick={() =>
                store.setCustomThemes(settings.customThemes.filter((item) => item.id !== family.id))
              }
            />
          </div>
        </div>
      ))}
      {draft !== null ? (
        <div className={css.editor}>
          <label className={css.field} htmlFor="custom-theme-name">
            <span>{t("custom.name")}</span>
            <Input
              className={css.input}
              id="custom-theme-name"
              value={draft.name}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  name: e.currentTarget.value,
                  id: slugifyThemeId(e.currentTarget.value),
                })
              }
            />
          </label>
          {(["light", "dark"] as const).map((mode) => (
            <fieldset key={mode} className={css.editorHalf}>
              <legend className={css.colorLabel}>
                {mode === "light" ? t("custom.lightHalf") : t("custom.darkHalf")}
              </legend>
              <div className={css.colorGrid}>
                {SEED_FIELDS.map((field) => (
                  <label key={field} className={css.colorField}>
                    <input
                      type="color"
                      className={css.colorSwatch}
                      value={draft[mode][field]}
                      onChange={(e) => {
                        const next = {
                          ...draft,
                          [mode]: { ...draft[mode], [field]: e.currentTarget.value },
                        };
                        setDraft(next);
                        store.previewFamily(next);
                      }}
                    />
                    <span className={css.colorMeta}>
                      <span className={css.colorLabel}>{t(`custom.${field}`)}</span>
                      <span className={css.colorValue}>{draft[mode][field]}</span>
                    </span>
                  </label>
                ))}
              </div>
              <label className={css.field}>
                <span className={css.rowHead}>
                  <span>{t("custom.contrast")}</span>
                  <span className={css.value}>{draft[mode].contrast}</span>
                </span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={draft[mode].contrast}
                  className={css.range}
                  style={sliderFillStyle(draft[mode].contrast, 0, 100)}
                  onChange={(e) => {
                    const next = {
                      ...draft,
                      [mode]: { ...draft[mode], contrast: Number(e.currentTarget.value) },
                    };
                    setDraft(next);
                    store.previewFamily(next);
                  }}
                />
              </label>
            </fieldset>
          ))}
          <div className={css.editorActions}>
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setDraft(null);
                store.previewFamily(null);
              }}
            >
              {t("custom.cancel")}
            </Button>
            <Button
              type="button"
              onClick={() => {
                save(draft);
                setDraft(null);
                store.previewFamily(null);
              }}
            >
              {t("custom.save")}
            </Button>
          </div>
        </div>
      ) : null}
    </SettingsSection>
  );
}
