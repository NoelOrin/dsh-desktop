/** 桌面与外观设置共用的轻量控件：开关、分段选择、滑块与设置行。 */
import type { ReactNode } from "react";
import { sliderFillStyle } from "../slider";
import css from "./controls.module.css";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  icon?: ReactNode;
}

export function SegmentedField<T extends string>({
  label,
  value,
  options,
  onChange,
  disabled = false,
}: {
  label: string;
  value: T;
  options: Array<SegmentOption<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
}): JSX.Element {
  return (
    <fieldset className={css.segmentedField}>
      <legend className={css.segmentedLabel}>{label}</legend>
      <div className={css.segmented}>
        {options.map((option) => {
          const active = option.value === value;
          return (
            <button
              key={option.value}
              type="button"
              className={`${css.segment}${active ? ` ${css.segmentActive}` : ""}`}
              aria-pressed={active}
              disabled={disabled}
              onClick={() => onChange(option.value)}
            >
              {option.icon}
              <span>{option.label}</span>
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

export function ToggleField({
  id,
  checked,
  onChange,
  title,
  description,
  disabled = false,
}: {
  id: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  title: string;
  description?: string;
  disabled?: boolean;
}): JSX.Element {
  return (
    <label className={css.toggleField} htmlFor={id}>
      <span className={css.toggleText}>
        <span className={css.toggleTitle}>{title}</span>
        {description ? <span className={css.toggleDesc}>{description}</span> : null}
      </span>
      <input
        id={id}
        type="checkbox"
        className={css.toggle}
        checked={checked}
        disabled={disabled}
        aria-label={title}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
    </label>
  );
}

export function SliderField({
  id,
  label,
  value,
  display,
  min,
  max,
  step = 1,
  onChange,
  disabled = false,
}: {
  id: string;
  label: string;
  value: number;
  display: string;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}): JSX.Element {
  return (
    <div className={css.sliderField}>
      <div className={css.sliderHeader}>
        <label htmlFor={id}>{label}</label>
        <span className={css.value}>{display}</span>
      </div>
      <input
        id={id}
        type="range"
        className={css.slider}
        style={sliderFillStyle(value, min, max)}
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
    </div>
  );
}

export function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <div className={css.settingRow}>
      <div className={css.settingText}>
        <span className={css.settingTitle}>{title}</span>
        {description ? <span className={css.settingDesc}>{description}</span> : null}
      </div>
      <div className={css.settingControl}>{children}</div>
    </div>
  );
}
