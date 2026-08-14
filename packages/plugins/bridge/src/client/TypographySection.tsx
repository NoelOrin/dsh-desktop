/** 排版：界面/代码字号与字体族。 */
import {
  MAX_CODE_FONT_SIZE,
  MAX_INTERFACE_FONT_SIZE,
  MIN_CODE_FONT_SIZE,
  MIN_INTERFACE_FONT_SIZE,
  type ThemeSettings,
} from "../shared/theme";

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
    <section aria-label={t("type.title")}>
      <h3 style={{ fontSize: 15, fontWeight: 600, margin: "0 0 6px" }}>{t("type.title")}</h3>
      <p style={{ fontSize: 12, opacity: 0.6, margin: "0 0 10px" }}>{t("type.desc")}</p>
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
    </section>
  );

  function renderRange(
    labelKey: string,
    value: number,
    min: number,
    max: number,
    apply: (value: number) => void,
  ): JSX.Element {
    return (
      <label style={{ display: "block", margin: "6px 0" }}>
        <span style={{ fontSize: 13 }}>
          {t(labelKey)}：{value}px
        </span>
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          style={{ width: "100%" }}
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
    return (
      <label style={{ display: "block", margin: "6px 0", fontSize: 13 }}>
        <span>{t(labelKey)}</span>
        <input
          value={value}
          placeholder={t("type.default")}
          style={{ width: "100%", padding: "6px 8px", marginTop: 4 }}
          onInput={(event) => apply(event.currentTarget.value)}
        />
      </label>
    );
  }
}
