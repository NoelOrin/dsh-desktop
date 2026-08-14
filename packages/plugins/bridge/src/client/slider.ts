/** 自定义 range 滑块的填充轨道样式（参考 Deepseek-Harness-Desktop ui-theme）。 */
import type { CSSProperties } from "react";

export function sliderFillStyle(value: number, min: number, max: number): CSSProperties {
  const span = max - min;
  const percent = span <= 0 ? 0 : Math.min(100, Math.max(0, ((value - min) / span) * 100));
  return { "--dsh-slider-fill": `${percent}%` } as CSSProperties;
}
