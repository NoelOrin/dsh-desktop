/** 玻璃透明度：实时表面预览加滑块。 */
import { Button } from "@deepseek-ai/dsh-client-ui-primitives";
import { GLASS_OPACITY_STEP, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY } from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { SliderField } from "./ui/controls";

export function GlassSlider({
  value,
  t,
  onChange,
}: {
  value: number;
  t: (key: string) => string;
  onChange: (value: number) => void;
}): JSX.Element {
  return (
    <SettingsSection
      headingId="appearance-glass-heading"
      title={t("glass.title")}
      description={t("glass.desc")}
    >
      <div className={css.glassStage} role="img" aria-label={t("glass.preview")}>
        <div className={css.glassBackdrop} aria-hidden="true" />
        <div className={css.glassSurface} style={{ opacity: value / 100 }} aria-hidden="true">
          {t("glass.surface")}
        </div>
      </div>
      <SliderField
        id="glass-opacity"
        label={t("glass.opacity")}
        value={value}
        display={`${value}%`}
        min={MIN_GLASS_OPACITY}
        max={MAX_GLASS_OPACITY}
        step={GLASS_OPACITY_STEP}
        onChange={onChange}
      />
      <Button type="button" variant="ghost" onClick={() => onChange(100)}>
        {t("wallpaper.reset")}
      </Button>
    </SettingsSection>
  );
}
