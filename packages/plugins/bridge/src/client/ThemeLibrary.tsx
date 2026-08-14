/** 主题库：每个家族一张卡（浅/深两半，点哪半用哪半）。 */
import { useSyncExternalStore } from "react";
import { isBuiltinFamilyId, listThemeFamilies, type ThemeFamily } from "../shared/theme";
import css from "./appearance.module.css";
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
    <section aria-label={t("library.title")}>
      <h3 className={css.sectionTitle}>{t("library.title")}</h3>
      <p className={css.hint}>{t("library.desc")}</p>
      <div className={css.library}>
        {families.map((family) => {
          const light = familySwatch(family, "light");
          const dark = familySwatch(family, "dark");
          return (
            <div key={family.id} className={css.card}>
              <div className={css.halves}>
                <button
                  type="button"
                  className={css.half}
                  data-active={activeLight === family.id}
                  style={{
                    background: light.background,
                    color: light.foreground,
                    borderRight: "1px solid rgba(0,0,0,0.08)",
                  }}
                  aria-label={`${family.name} 浅色`}
                  title={t("library.lightHalf")}
                  onClick={() => store.setThemeHalf("light", family.id)}
                >
                  <span style={{ color: light.accent, fontWeight: 700 }}>Aa</span>
                </button>
                <button
                  type="button"
                  className={css.half}
                  data-active={activeDark === family.id}
                  style={{ background: dark.background, color: dark.foreground }}
                  aria-label={`${family.name} 深色`}
                  title={t("library.darkHalf")}
                  onClick={() => store.setThemeHalf("dark", family.id)}
                >
                  <span style={{ color: dark.accent, fontWeight: 700 }}>Aa</span>
                </button>
              </div>
              <div className={css.caption}>
                <span>{family.name}</span>
                <span className={css.badge}>
                  {isBuiltinFamilyId(family.id) ? t("library.builtin") : t("library.custom")}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
