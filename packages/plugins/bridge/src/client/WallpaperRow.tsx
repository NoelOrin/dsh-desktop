/** 背景图设置：选图、预览、毛玻璃与像素化。 */
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
import { SliderField } from "./ui/controls";

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
    <SettingsSection
      headingId="appearance-wallpaper-heading"
      title={t("wallpaper.title")}
      description={t("wallpaper.desc")}
    >
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
        <img className={css.wallpaperPreview} src={wallpaperImage} alt={t("wallpaper.preview")} />
      ) : (
        <div className={css.wallpaperEmpty}>{t("wallpaper.empty")}</div>
      )}
      <div className={css.effectStack}>
        <SliderField
          id="wallpaper-blur"
          label={t("wallpaper.blur")}
          value={wallpaperBlur}
          display={`${wallpaperBlur}%`}
          min={MIN_WALLPAPER_EFFECT}
          max={MAX_WALLPAPER_EFFECT}
          step={WALLPAPER_EFFECT_STEP}
          onChange={(value) => setWallpaper({ wallpaperBlur: value })}
        />
        <SliderField
          id="wallpaper-pixelate"
          label={t("wallpaper.pixelate")}
          value={wallpaperPixelate}
          display={`${wallpaperPixelate}%`}
          min={MIN_WALLPAPER_EFFECT}
          max={MAX_WALLPAPER_EFFECT}
          step={WALLPAPER_EFFECT_STEP}
          onChange={(value) => setWallpaper({ wallpaperPixelate: value })}
        />
      </div>
      {hasImage ? (
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
      ) : null}
    </SettingsSection>
  );
}
