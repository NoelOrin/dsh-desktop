/** ui-theme 设置快照存储：bind 上游命名空间，本地快照 + 防抖写回 Host。 */
import {
  clampWallpaperEffect,
  DEFAULT_THEME_SETTINGS,
  isWallpaperDataUrl,
  MAX_CODE_FONT_SIZE,
  MAX_GLASS_OPACITY,
  MAX_INTERFACE_FONT_SIZE,
  MIN_CODE_FONT_SIZE,
  MIN_GLASS_OPACITY,
  MIN_INTERFACE_FONT_SIZE,
  resolveThemeSettings,
  THEME_CUSTOM_THEMES_FIELD,
  THEME_DARK_FAMILY_FIELD,
  THEME_GLASS_OPACITY_FIELD,
  THEME_LIGHT_FAMILY_FIELD,
  THEME_PREFERENCE_FIELD,
  THEME_WALLPAPER_BLUR_FIELD,
  THEME_WALLPAPER_IMAGE_FIELD,
  THEME_WALLPAPER_PIXELATE_FIELD,
  type ThemeFamily,
  type ThemePreference,
  type ThemeSettings,
} from "../shared/theme";

/** settingsScope.bind 的最小形状（与 client.tsx 的 SettingsScopeLike 对齐）。 */
export interface ThemeScopeLike<T> {
  getSnapshot(): { status: "loading" | "ready" | "unavailable"; value: T | undefined };
  subscribe(listener: () => void): () => void;
  set(field: string, value: unknown): Promise<void>;
}

const clampInt = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, Math.round(value)));

/** 主题设置存储：快照镜像 + 防抖 Host 写。 */
export interface ThemeStore {
  getSnapshot(): ThemeSettings;
  getPreview(): ThemeFamily | null;
  subscribe(listener: () => void): () => void;
  setPreference(preference: ThemePreference): void;
  setThemeHalf(mode: "light" | "dark", familyId: string): void;
  setCustomThemes(customThemes: ThemeFamily[]): void;
  setGlassOpacity(value: number): void;
  setWallpaper(
    patch: Partial<Pick<ThemeSettings, "wallpaperImage" | "wallpaperBlur" | "wallpaperPixelate">>,
  ): void;
  setTypography(
    patch: Partial<
      Pick<
        ThemeSettings,
        | "fontFamilySans"
        | "fontFamilyCode"
        | "fontSizeInterface"
        | "fontSizeCode"
        | "fontFamilyComposer"
        | "fontFamilyTerminal"
      >
    >,
  ): void;
  previewFamily(family: ThemeFamily | null): void;
}

export function createThemeStore(scope: ThemeScopeLike<ThemeSettings>): ThemeStore {
  let settings: ThemeSettings = { ...DEFAULT_THEME_SETTINGS, customThemes: [] };
  let preview: ThemeFamily | null = null;
  const listeners = new Set<() => void>();

  const pendingWrites = new Map<string, unknown>();
  let writeTimer: ReturnType<typeof setTimeout> | undefined;
  let inFlightWrites = 0;

  const publish = (): void => {
    for (const listener of [...listeners]) {
      try {
        listener();
      } catch {
        // 监听器异常不阻断后续
      }
    }
  };

  const adopt = (): void => {
    if (pendingWrites.size > 0 || inFlightWrites > 0) return;
    const snapshot = scope.getSnapshot();
    if (snapshot.status !== "ready" || snapshot.value === undefined) return;
    const next = resolveThemeSettings(snapshot.value);
    if (JSON.stringify(next) === JSON.stringify(settings)) return;
    settings = next;
    publish();
  };

  const queueWrite = (field: string, value: unknown): void => {
    pendingWrites.set(field, value);
    if (writeTimer !== undefined) clearTimeout(writeTimer);
    writeTimer = setTimeout(() => {
      void flushWrites();
    }, 300);
  };

  const flushWrites = async (): Promise<void> => {
    if (writeTimer !== undefined) {
      clearTimeout(writeTimer);
      writeTimer = undefined;
    }
    if (pendingWrites.size === 0) return;
    const writes = [...pendingWrites];
    pendingWrites.clear();
    for (const [field, value] of writes) {
      inFlightWrites += 1;
      await scope.set(field, value).catch(() => {});
      inFlightWrites -= 1;
    }
    if (inFlightWrites === 0 && pendingWrites.size === 0) adopt();
  };

  scope.subscribe(adopt);
  adopt();

  const store: ThemeStore = {
    getSnapshot: () => settings,
    getPreview: () => preview,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    setPreference: (preference) => {
      if (settings.preference === preference) return;
      settings = { ...settings, preference };
      queueWrite(THEME_PREFERENCE_FIELD, preference);
      publish();
    },
    setThemeHalf: (mode, familyId) => {
      const field = mode === "dark" ? THEME_DARK_FAMILY_FIELD : THEME_LIGHT_FAMILY_FIELD;
      if (settings[field] === familyId) return;
      settings = { ...settings, [field]: familyId };
      queueWrite(field, familyId);
      publish();
    },
    setCustomThemes: (customThemes) => {
      const next = customThemes.map((item) => ({ ...item, origin: "custom" as const }));
      settings = { ...settings, customThemes: next };
      queueWrite(THEME_CUSTOM_THEMES_FIELD, next);
      publish();
    },
    setGlassOpacity: (value) => {
      const next = clampInt(value, MIN_GLASS_OPACITY, MAX_GLASS_OPACITY);
      if (settings.glassOpacity === next) return;
      settings = { ...settings, glassOpacity: next };
      queueWrite(THEME_GLASS_OPACITY_FIELD, next);
      publish();
    },
    setWallpaper: (patch) => {
      const next: Pick<ThemeSettings, "wallpaperImage" | "wallpaperBlur" | "wallpaperPixelate"> = {
        wallpaperImage: settings.wallpaperImage,
        wallpaperBlur: settings.wallpaperBlur,
        wallpaperPixelate: settings.wallpaperPixelate,
      };
      if (patch.wallpaperImage !== undefined) {
        next.wallpaperImage =
          patch.wallpaperImage === "" || isWallpaperDataUrl(patch.wallpaperImage)
            ? patch.wallpaperImage
            : "";
      }
      if (patch.wallpaperBlur !== undefined) {
        next.wallpaperBlur = clampWallpaperEffect(patch.wallpaperBlur);
      }
      if (patch.wallpaperPixelate !== undefined) {
        next.wallpaperPixelate = clampWallpaperEffect(patch.wallpaperPixelate);
      }
      if (
        next.wallpaperImage === settings.wallpaperImage &&
        next.wallpaperBlur === settings.wallpaperBlur &&
        next.wallpaperPixelate === settings.wallpaperPixelate
      ) {
        return;
      }
      settings = { ...settings, ...next };
      if (patch.wallpaperImage !== undefined) {
        queueWrite(THEME_WALLPAPER_IMAGE_FIELD, next.wallpaperImage);
      }
      if (patch.wallpaperBlur !== undefined) {
        queueWrite(THEME_WALLPAPER_BLUR_FIELD, next.wallpaperBlur);
      }
      if (patch.wallpaperPixelate !== undefined) {
        queueWrite(THEME_WALLPAPER_PIXELATE_FIELD, next.wallpaperPixelate);
      }
      publish();
    },
    setTypography: (patch) => {
      const next = { ...patch };
      if (next.fontSizeInterface !== undefined) {
        next.fontSizeInterface = clampInt(
          next.fontSizeInterface,
          MIN_INTERFACE_FONT_SIZE,
          MAX_INTERFACE_FONT_SIZE,
        );
      }
      if (next.fontSizeCode !== undefined) {
        next.fontSizeCode = clampInt(next.fontSizeCode, MIN_CODE_FONT_SIZE, MAX_CODE_FONT_SIZE);
      }
      settings = { ...settings, ...next };
      for (const [field, value] of Object.entries(next)) queueWrite(field, value);
      publish();
    },
    previewFamily: (family) => {
      if (preview === family) return;
      preview = family;
      publish();
    },
  };

  return store;
}
