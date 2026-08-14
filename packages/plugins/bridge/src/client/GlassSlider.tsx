/** 玻璃透明度：40–100，越低表面越通透。 */
import { GLASS_OPACITY_STEP, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY } from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { sliderFillStyle } from "./slider";

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
      title={t("glass.title")}
      description={t("glass.desc")}
      actions={<span className={css.value}>{value}%</span>}
    >
      <label className={css.field}>
        <span className={css.rowHead}>
          <span>{t("glass.opacity")}</span>
          <span className={css.value}>{value}%</span>
        </span>
        <input
          type="range"
          min={MIN_GLASS_OPACITY}
          max={MAX_GLASS_OPACITY}
          step={GLASS_OPACITY_STEP}
          value={value}
          className={css.range}
          style={sliderFillStyle(value, MIN_GLASS_OPACITY, MAX_GLASS_OPACITY)}
          aria-label={t("glass.opacity")}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    </SettingsSection>
  );
}
