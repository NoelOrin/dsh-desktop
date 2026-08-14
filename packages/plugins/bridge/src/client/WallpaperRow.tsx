/** 背景图：选图 → data URL → 毛玻璃/像素化滑块。 */

import { Button } from "@deepseek-ai/dsh-client-ui-primitives";
import { useRef } from "react";
import {
  DEFAULT_WALLPAPER_EFFECT,
  encodeWallpaperFile,
  MAX_WALLPAPER_EFFECT,
  MIN_WALLPAPER_EFFECT,
  WALLPAPER_EFFECT_STEP,
} from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { sliderFillStyle } from "./slider";

export function WallpaperRow({
  wallpaperImage,
  wallpaperBlur,
  wallpaperPixelate,
  t,
  setWallpaper,
}: {
  wallpaperImage: string;
  wallpaperBlur: number;
  wallpaperPixelate: number;
  t: (key: string) => string;
  setWallpaper: (patch: {
    wallpaperImage?: string;
    wallpaperBlur?: number;
    wallpaperPixelate?: number;
  }) => void;
}): JSX.Element {
  const fileRef = useRef<HTMLInputElement>(null);
  const hasImage = wallpaperImage.length > 0;

  const pick = async (file: File | undefined): Promise<void> => {
    if (file === undefined) return;
    const encoded = await encodeWallpaperFile(file);
    if (encoded === null) return;
    setWallpaper({ wallpaperImage: encoded });
  };

  return (
    <SettingsSection title={t("wallpaper.title")} description={t("wallpaper.desc")}>
      <div className={css.wallpaperActions}>
        <Button type="button" variant="outline" onClick={() => fileRef.current?.click()}>
          {t("wallpaper.choose")}
        </Button>
        {hasImage ? (
          <Button
            type="button"
            variant="ghost"
            onClick={() => setWallpaper({ wallpaperImage: "" })}
          >
            {t("wallpaper.clear")}
          </Button>
        ) : null}
        <input
          ref={fileRef}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          hidden
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            void pick(file);
          }}
        />
      </div>
      {hasImage ? (
        <>
          <div
            className={css.wallpaperPreview}
            style={{ backgroundImage: `url("${wallpaperImage}")` }}
            role="img"
            aria-label={t("wallpaper.title")}
          />
          {renderSlider("wallpaper.blur", wallpaperBlur, (value) =>
            setWallpaper({ wallpaperBlur: value }),
          )}
          {renderSlider("wallpaper.pixelate", wallpaperPixelate, (value) =>
            setWallpaper({ wallpaperPixelate: value }),
          )}
          <Button
            type="button"
            variant="ghost"
            onClick={() =>
              setWallpaper({
                wallpaperBlur: DEFAULT_WALLPAPER_EFFECT,
                wallpaperPixelate: DEFAULT_WALLPAPER_EFFECT,
              })
            }
          >
            {t("wallpaper.reset")}
          </Button>
        </>
      ) : null}
    </SettingsSection>
  );

  function renderSlider(
    labelKey: string,
    value: number,
    onChange: (value: number) => void,
  ): JSX.Element {
    return (
      <label className={css.field}>
        <span className={css.rowHead}>
          <span>{t(labelKey)}</span>
          <span className={css.value}>{value}%</span>
        </span>
        <input
          type="range"
          min={MIN_WALLPAPER_EFFECT}
          max={MAX_WALLPAPER_EFFECT}
          step={WALLPAPER_EFFECT_STEP}
          value={value}
          className={css.range}
          style={sliderFillStyle(value, MIN_WALLPAPER_EFFECT, MAX_WALLPAPER_EFFECT)}
          aria-label={t(labelKey)}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    );
  }
}
