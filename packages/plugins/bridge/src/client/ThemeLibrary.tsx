/** 主题库：每个家族一张双半预览卡，点哪半用哪半。 */

import { IconCheckOutline16 } from "@deepseek-ai/dsh-client-ui-primitives";
import { useSyncExternalStore } from "react";
import { isBuiltinFamilyId, listThemeFamilies, type ThemeFamily } from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import type { ThemeStore } from "./theme-store";

export function familySwatch(
  family: ThemeFamily,
  mode: "light" | "dark",
): { background: string; foreground: string; accent: string } {
  const seeds = family[mode];
  return {
    background: seeds.background,
    foreground: seeds.foreground,
    accent: seeds.accent,
  };
}

export function ThemeLibrary({
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
  const families = listThemeFamilies(settings.customThemes);
  const activeLight = settings.activeLightThemeId;
  const activeDark = settings.activeDarkThemeId;

  return (
    <SettingsSection
      headingId="appearance-library-heading"
      title={t("library.title")}
      description={t("library.desc")}
    >
      <div className={css.themeGrid}>
        {families.map((family) => {
          const light = familySwatch(family, "light");
          const dark = familySwatch(family, "dark");
          const lightActive = activeLight === family.id;
          const darkActive = activeDark === family.id;
          return (
            <article key={family.id} className={css.themeCard}>
              <div className={css.themePreview}>
                <button
                  type="button"
                  className={`${css.themeHalf}${lightActive ? ` ${css.themeHalfActive}` : ""}`}
                  style={{
                    background: light.background,
                    color: light.foreground,
                  }}
                  aria-pressed={lightActive}
                  aria-label={`${family.name} ${t("library.lightHalf")}`}
                  title={t("library.lightHalf")}
                  onClick={() => store.setThemeHalf("light", family.id)}
                >
                  <span className={css.miniUi} aria-hidden="true">
                    <span className={css.miniDot} style={{ background: light.accent }} />
                    <span className={css.miniLines}>
                      <span className={css.miniLine} />
                      <span className={`${css.miniLine} ${css.miniLineShort}`} />
                    </span>
                  </span>
                  <span className={css.themeHalfLabel}>{t("library.lightHalf")}</span>
                  {lightActive ? (
                    <span className={css.themeBadge} aria-hidden="true">
                      <IconCheckOutline16 size={14} />
                    </span>
                  ) : null}
                </button>
                <button
                  type="button"
                  className={`${css.themeHalf}${darkActive ? ` ${css.themeHalfActive}` : ""}`}
                  style={{ background: dark.background, color: dark.foreground }}
                  aria-pressed={darkActive}
                  aria-label={`${family.name} ${t("library.darkHalf")}`}
                  title={t("library.darkHalf")}
                  onClick={() => store.setThemeHalf("dark", family.id)}
                >
                  <span className={css.miniUi} aria-hidden="true">
                    <span className={css.miniDot} style={{ background: dark.accent }} />
                    <span className={css.miniLines}>
                      <span className={css.miniLine} />
                      <span className={`${css.miniLine} ${css.miniLineShort}`} />
                    </span>
                  </span>
                  <span className={css.themeHalfLabel}>{t("library.darkHalf")}</span>
                  {darkActive ? (
                    <span className={css.themeBadge} aria-hidden="true">
                      <IconCheckOutline16 size={14} />
                    </span>
                  ) : null}
                </button>
              </div>
              <div className={css.themeCardFoot}>
                <span className={css.themeName}>{family.name}</span>
                <span className={css.themeMeta}>
                  {isBuiltinFamilyId(family.id) ? t("library.builtin") : t("library.custom")}
                </span>
              </div>
            </article>
          );
        })}
      </div>
    </SettingsSection>
  );
}
