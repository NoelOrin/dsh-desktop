/** 自定义主题：创建/复制/编辑/导入导出（JSON），编辑时实时预览。 */
import { useState, useSyncExternalStore } from "react";
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
    <section aria-label={t("custom.title")}>
      <h3 className={css.sectionTitle}>{t("custom.title")}</h3>
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 10 }}>
        <button type="button" onClick={create}>
          {t("custom.create")}
        </button>
        <label style={{ cursor: "pointer", fontSize: 13 }}>
          {t("custom.import")}
          <input
            type="file"
            accept="application/json,.json"
            hidden
            onChange={(event) => importFamily(event.currentTarget.files?.[0])}
          />
        </label>
      </div>
      {settings.customThemes.map((family) => (
        <div
          key={family.id}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "6px 0",
            borderTop: "1px solid var(--dsw-alias-border-l1, #e3e6eb)",
          }}
        >
          <span style={{ flex: 1, fontSize: 13 }}>{family.name}</span>
          <button type="button" onClick={() => setDraft(family)}>
            {t("custom.edit")}
          </button>
          <button
            type="button"
            onClick={() => {
              const dup = duplicateThemeFamily(family, existingIds);
              save(dup);
            }}
          >
            {t("custom.copy")}
          </button>
          <button type="button" onClick={() => exportFamily(family)}>
            {t("custom.export")}
          </button>
          <button
            type="button"
            onClick={() =>
              store.setCustomThemes(settings.customThemes.filter((item) => item.id !== family.id))
            }
          >
            {t("custom.remove")}
          </button>
        </div>
      ))}
      {draft !== null ? (
        <div
          style={{
            border: "1px solid var(--dsw-alias-border-l1, #e3e6eb)",
            borderRadius: 8,
            padding: 12,
            marginTop: 10,
          }}
        >
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
            {t("custom.name")}
            <input
              value={draft.name}
              style={{ flex: 1, padding: "6px 8px" }}
              onInput={(e) =>
                setDraft({
                  ...draft,
                  name: e.currentTarget.value,
                  id: slugifyThemeId(e.currentTarget.value),
                })
              }
            />
          </label>
          {(["light", "dark"] as const).map((mode) => (
            <div key={mode}>
              <div style={{ fontSize: 13, fontWeight: 600, margin: "8px 0 4px" }}>
                {mode === "light" ? t("custom.lightHalf") : t("custom.darkHalf")}
              </div>
              {SEED_FIELDS.map((field) => (
                <label
                  key={field}
                  style={{
                    display: "flex",
                    gap: 6,
                    alignItems: "center",
                    fontSize: 13,
                    margin: "4px 0",
                  }}
                >
                  <span style={{ width: 80 }}>{t(`custom.${field}`)}</span>
                  <input
                    type="color"
                    value={draft[mode][field]}
                    style={{ width: 44, height: 26, padding: 0, border: "none" }}
                    onChange={(e) => {
                      const next = {
                        ...draft,
                        [mode]: { ...draft[mode], [field]: e.currentTarget.value },
                      };
                      setDraft(next);
                      store.previewFamily(next);
                    }}
                  />
                  <input
                    value={draft[mode][field]}
                    style={{ flex: 1, padding: "4px 6px", fontSize: 12 }}
                    onInput={(e) => {
                      const next = {
                        ...draft,
                        [mode]: { ...draft[mode], [field]: e.currentTarget.value },
                      };
                      setDraft(next);
                      store.previewFamily(next);
                    }}
                  />
                </label>
              ))}
              <label
                style={{
                  display: "flex",
                  gap: 6,
                  alignItems: "center",
                  fontSize: 13,
                  margin: "4px 0",
                }}
              >
                <span style={{ width: 80 }}>{t("custom.contrast")}</span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={draft[mode].contrast}
                  style={{ flex: 1 }}
                  onChange={(e) => {
                    const next = {
                      ...draft,
                      [mode]: { ...draft[mode], contrast: Number(e.currentTarget.value) },
                    };
                    setDraft(next);
                    store.previewFamily(next);
                  }}
                />
                <span style={{ fontSize: 12, width: 32, textAlign: "right" }}>
                  {draft[mode].contrast}
                </span>
              </label>
            </div>
          ))}
          <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
            <button
              type="button"
              onClick={() => {
                save(draft);
                setDraft(null);
                store.previewFamily(null);
              }}
            >
              {t("custom.save")}
            </button>
            <button
              type="button"
              onClick={() => {
                setDraft(null);
                store.previewFamily(null);
              }}
            >
              {t("custom.cancel")}
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
