/** 排版设置：界面/代码字号与字体族。 */
import { Button, Input } from "@deepseek-ai/dsh-client-ui-primitives";
import {
  DEFAULT_CODE_FONT_SIZE,
  DEFAULT_INTERFACE_FONT_SIZE,
  MAX_CODE_FONT_SIZE,
  MAX_INTERFACE_FONT_SIZE,
  MIN_CODE_FONT_SIZE,
  MIN_INTERFACE_FONT_SIZE,
  type ThemeSettings,
} from "../shared/theme";
import css from "./appearance.module.css";
import { SettingsSection } from "./settings-layout";
import { SliderField } from "./ui/controls";

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
    <SettingsSection
      headingId="appearance-type-heading"
      title={t("type.title")}
      description={t("type.desc")}
      actions={
        <Button
          type="button"
          variant="ghost"
          onClick={() =>
            onChange({
              fontFamilySans: "",
              fontFamilyCode: "",
              fontSizeInterface: DEFAULT_INTERFACE_FONT_SIZE,
              fontSizeCode: DEFAULT_CODE_FONT_SIZE,
              fontFamilyComposer: "",
              fontFamilyTerminal: "",
            })
          }
        >
          {t("type.reset")}
        </Button>
      }
    >
      <div className={css.typographyGrid}>
        <SliderField
          id="type-interface-size"
          label={t("type.interfaceSize")}
          value={settings.fontSizeInterface}
          display={`${settings.fontSizeInterface}px`}
          min={MIN_INTERFACE_FONT_SIZE}
          max={MAX_INTERFACE_FONT_SIZE}
          onChange={(value) => onChange({ fontSizeInterface: value })}
        />
        <SliderField
          id="type-code-size"
          label={t("type.codeSize")}
          value={settings.fontSizeCode}
          display={`${settings.fontSizeCode}px`}
          min={MIN_CODE_FONT_SIZE}
          max={MAX_CODE_FONT_SIZE}
          onChange={(value) => onChange({ fontSizeCode: value })}
        />
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
      </div>
    </SettingsSection>
  );

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
