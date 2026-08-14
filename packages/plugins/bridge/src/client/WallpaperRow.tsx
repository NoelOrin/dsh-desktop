/** 背景图：选图 → data URL → 毛玻璃/像素化滑块。 */
import { useRef } from "react";
import {
  DEFAULT_WALLPAPER_EFFECT,
  encodeWallpaperFile,
  MAX_WALLPAPER_EFFECT,
  MIN_WALLPAPER_EFFECT,
  WALLPAPER_EFFECT_STEP,
} from "../shared/theme";
import css from "./appearance.module.css";

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
    <section aria-label={t("wallpaper.title")}>
      <h3 className={css.sectionTitle}>{t("wallpaper.title")}</h3>
      <p className={css.hint}>{t("wallpaper.desc")}</p>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button type="button" onClick={() => fileRef.current?.click()}>
          {t("wallpaper.choose")}
        </button>
        {hasImage ? (
          <button type="button" onClick={() => setWallpaper({ wallpaperImage: "" })}>
            {t("wallpaper.clear")}
          </button>
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
            style={{
              marginTop: 12,
              height: 120,
              borderRadius: 8,
              backgroundImage: `url("${wallpaperImage}")`,
              backgroundSize: "cover",
              backgroundPosition: "center",
            }}
            role="img"
            aria-label={t("wallpaper.title")}
          />
          {renderSlider("wallpaper.blur", wallpaperBlur, (value) =>
            setWallpaper({ wallpaperBlur: value }),
          )}
          {renderSlider("wallpaper.pixelate", wallpaperPixelate, (value) =>
            setWallpaper({ wallpaperPixelate: value }),
          )}
          <button
            type="button"
            onClick={() =>
              setWallpaper({
                wallpaperBlur: DEFAULT_WALLPAPER_EFFECT,
                wallpaperPixelate: DEFAULT_WALLPAPER_EFFECT,
              })
            }
          >
            {t("wallpaper.reset")}
          </button>
        </>
      ) : null}
    </section>
  );

  function renderSlider(
    labelKey: string,
    value: number,
    onChange: (value: number) => void,
  ): JSX.Element {
    return (
      <label style={{ display: "block", marginTop: 10 }}>
        <span style={{ fontSize: 13 }}>
          {t(labelKey)}：{value}%
        </span>
        <input
          type="range"
          min={MIN_WALLPAPER_EFFECT}
          max={MAX_WALLPAPER_EFFECT}
          step={WALLPAPER_EFFECT_STEP}
          value={value}
          style={{ width: "100%" }}
          aria-label={t(labelKey)}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    );
  }
}
