/** 玻璃透明度：40–100，越低表面越通透。 */
import { GLASS_OPACITY_STEP, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY } from "../shared/theme";

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
    <section aria-label={t("glass.title")}>
      <h3 style={{ fontSize: 15, fontWeight: 600, margin: "0 0 6px" }}>{t("glass.title")}</h3>
      <p style={{ fontSize: 12, opacity: 0.6, margin: "0 0 10px" }}>{t("glass.desc")}</p>
      <label style={{ display: "block" }}>
        <span style={{ fontSize: 13 }}>
          {t("glass.opacity")}：{value}%
        </span>
        <input
          type="range"
          min={MIN_GLASS_OPACITY}
          max={MAX_GLASS_OPACITY}
          step={GLASS_OPACITY_STEP}
          value={value}
          style={{ width: "100%" }}
          aria-label={t("glass.opacity")}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    </section>
  );
}
