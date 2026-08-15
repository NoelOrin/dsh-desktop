/** 颜色方案选择器：浅色 / 深色 / 跟随系统。 */
import {
  IconDarkOutline16,
  IconFollowsystemOutline16,
  IconLightOutline16,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { DEFAULT_PREFERENCE, THEME_PREFERENCES, type ThemePreference } from "../shared/theme";
import { SegmentedField, type SegmentOption } from "./ui/controls";

const OPTIONS: Array<SegmentOption<ThemePreference>> = [
  { value: "light", label: "pref.light", icon: <IconLightOutline16 /> },
  { value: "dark", label: "pref.dark", icon: <IconDarkOutline16 /> },
  { value: "system", label: "pref.system", icon: <IconFollowsystemOutline16 /> },
];

export function ColorSchemeTiles({
  preference,
  t,
  setTheme,
}: {
  preference: ThemePreference;
  t: (key: string) => string;
  setTheme: (id: ThemePreference) => void;
}): JSX.Element {
  return (
    <SegmentedField<ThemePreference>
      label={t("pref.label")}
      value={preference}
      options={OPTIONS.map((option) => ({
        ...option,
        label: t(option.label),
      }))}
      onChange={(next) => {
        if (next === DEFAULT_PREFERENCE || THEME_PREFERENCES.includes(next)) {
          setTheme(next);
        }
      }}
    />
  );
}
