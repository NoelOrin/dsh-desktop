/** 外观设置节：颜色方案 + 主题库 + 背景图 + 玻璃 + 自定义主题 + 排版。 */
import { useSyncExternalStore } from "react";
import { DEFAULT_PREFERENCE, THEME_PREFERENCES, type ThemePreference } from "../shared/theme";
import { ColorSchemeTiles } from "./ColorSchemeTiles";
import { CustomThemeEditor } from "./CustomThemeEditor";
import { GlassSlider } from "./GlassSlider";
import { SettingsPage, SettingsSection } from "./settings-layout";
import { ThemeLibrary } from "./ThemeLibrary";
import { TypographySection } from "./TypographySection";
import type { ThemeStore } from "./theme-store";
import { WallpaperRow } from "./WallpaperRow";

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
  const writeError = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getWriteError(),
    () => store.getWriteError(),
  );
  const setPreference = (preference: ThemePreference): void => {
    if (preference === DEFAULT_PREFERENCE || THEME_PREFERENCES.includes(preference)) {
      store.setPreference(preference);
    }
  };
  return (
    <SettingsPage>
      <SettingsSection
        headingId="appearance-scheme-heading"
        title={t("pref.title")}
        description={t("pref.desc")}
      >
        <ColorSchemeTiles preference={settings.preference} t={t} setTheme={setPreference} />
      </SettingsSection>

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
      {writeError ? (
        <p role="alert">
          {t("saveError")}: {writeError}
        </p>
      ) : null}
    </SettingsPage>
  );
}
