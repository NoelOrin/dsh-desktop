/** 排版：界面/代码字号与字体族。 */
import { Input } from "@deepseek-ai/dsh-client-ui-primitives";
import {
  MAX_CODE_FONT_SIZE,
  MAX_INTERFACE_FONT_SIZE,
  MIN_CODE_FONT_SIZE,
  MIN_INTERFACE_FONT_SIZE,
  type ThemeSettings,
} from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { sliderFillStyle } from "./slider";

export function TypographySection({
  settings,
  t,
  onChange,
}: {
  settings: ThemeSettings;
  t: (key: string) => string;
  onChange: (
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
  ) => void;
}): JSX.Element {
  return (
    <SettingsSection title={t("type.title")} description={t("type.desc")}>
      {renderRange(
        "type.interfaceSize",
        settings.fontSizeInterface,
        MIN_INTERFACE_FONT_SIZE,
        MAX_INTERFACE_FONT_SIZE,
        (value) => onChange({ fontSizeInterface: value }),
      )}
      {renderRange(
        "type.codeSize",
        settings.fontSizeCode,
        MIN_CODE_FONT_SIZE,
        MAX_CODE_FONT_SIZE,
        (value) => onChange({ fontSizeCode: value }),
      )}
      {renderText("type.sans", settings.fontFamilySans, (value) =>
        onChange({ fontFamilySans: value }),
      )}
      {renderText("type.code", settings.fontFamilyCode, (value) =>
        onChange({ fontFamilyCode: value }),
      )}
      {renderText("type.composer", settings.fontFamilyComposer, (value) =>
        onChange({ fontFamilyComposer: value }),
      )}
      {renderText("type.terminal", settings.fontFamilyTerminal, (value) =>
        onChange({ fontFamilyTerminal: value }),
      )}
    </SettingsSection>
  );

  function renderRange(
    labelKey: string,
    value: number,
    min: number,
    max: number,
    apply: (value: number) => void,
  ): JSX.Element {
    return (
      <label className={css.field}>
        <span className={css.rowHead}>
          <span>{t(labelKey)}</span>
          <span className={css.value}>{value}px</span>
        </span>
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          className={css.range}
          style={sliderFillStyle(value, min, max)}
          aria-label={t(labelKey)}
          onChange={(event) => apply(Number(event.currentTarget.value))}
        />
      </label>
    );
  }

  function renderText(
    labelKey: string,
    value: string,
    apply: (value: string) => void,
  ): JSX.Element {
    const inputId = `typography-${labelKey.replaceAll(".", "-")}`;
    return (
      <label className={css.field} htmlFor={inputId}>
        <span>{t(labelKey)}</span>
        <Input
          className={css.input}
          id={inputId}
          value={value}
          placeholder={t("type.default")}
          onChange={(event) => apply(event.currentTarget.value)}
        />
      </label>
    );
  }
}
