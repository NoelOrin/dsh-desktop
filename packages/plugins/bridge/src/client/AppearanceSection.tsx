/** 外观设置节：主题偏好 + 主题库（背景图/玻璃/自定义主题/排版由 Task 5/6 追加）。 */
import { useSyncExternalStore } from "react";
import { DEFAULT_PREFERENCE, THEME_PREFERENCES, type ThemePreference } from "../shared/theme";
import css from "./appearance.module.css";
import { CustomThemeEditor } from "./CustomThemeEditor";
import { GlassSlider } from "./GlassSlider";
import { ThemeLibrary } from "./ThemeLibrary";
import { TypographySection } from "./TypographySection";
import type { ThemeStore } from "./theme-store";
import { WallpaperRow } from "./WallpaperRow";

const PREFERENCE_LABELS: Record<ThemePreference, string> = {
  light: "pref.light",
  dark: "pref.dark",
  system: "pref.system",
};

export function AppearanceSection({
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
  const setPreference = (preference: ThemePreference): void => {
    if (preference === DEFAULT_PREFERENCE || THEME_PREFERENCES.includes(preference)) {
      store.setPreference(preference);
    }
  };
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 28,
        padding: "8px 0",
        maxWidth: 640,
      }}
    >
      <section aria-label={t("pref.title")}>
        <h3 className={css.sectionTitle}>{t("pref.title")}</h3>
        <fieldset className={css.prefRow} aria-label={t("pref.title")}>
          {THEME_PREFERENCES.map((preference) => (
            <button
              key={preference}
              type="button"
              className={css.cube}
              data-active={settings.preference === preference}
              aria-pressed={settings.preference === preference}
              onClick={() => setPreference(preference)}
            >
              {t(PREFERENCE_LABELS[preference])}
            </button>
          ))}
        </fieldset>
      </section>
      <ThemeLibrary store={store} t={t} />
      <WallpaperRow
        wallpaperImage={settings.wallpaperImage}
        wallpaperBlur={settings.wallpaperBlur}
        wallpaperPixelate={settings.wallpaperPixelate}
        t={t}
        setWallpaper={(patch) => store.setWallpaper(patch)}
      />
      <GlassSlider
        value={settings.glassOpacity}
        t={t}
        onChange={(value) => store.setGlassOpacity(value)}
      />
      <CustomThemeEditor store={store} t={t} />
      <TypographySection
        settings={settings}
        t={t}
        onChange={(patch) => store.setTypography(patch)}
      />
    </div>
  );
}
