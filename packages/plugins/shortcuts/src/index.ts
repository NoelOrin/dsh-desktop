import type { Context } from "@deepseek-ai/cordis";
import type { SettingsProvider } from "@deepseek-ai/dsh-settings";
import { settingsNamespace } from "@deepseek-ai/dsh-settings";
import z from "@deepseek-ai/schemastery";

export const name = "shortcuts";

export const inject = ["settings"] as const;

const PRESET_SCHEMA = z.object({
  enabled: z.boolean(),
  shortcut: z.string(),
});

/** 注册快捷键设置 namespace，持久化交给 dsh settings。 */
export function apply(ctx: Context): void {
  (ctx as Context & { settings: SettingsProvider }).settings.register(
    settingsNamespace("dsh-desktop-shortcuts"),
    z.object({
      doubleEscapeStopEnabled: z.boolean().default(true),
      doubleEscapeStopTimeoutMs: z.number().default(1000),
      presets: z.object({
        toggleWindow: PRESET_SCHEMA.default({
          enabled: false,
          shortcut: "CmdOrCtrl+Shift+Space",
        }),
        stopConversation: PRESET_SCHEMA.default({
          enabled: false,
          shortcut: "CmdOrCtrl+Shift+.",
        }),
        newConversation: PRESET_SCHEMA.default({ enabled: false, shortcut: "CmdOrCtrl+Shift+N" }),
      }),
    }),
    { applies: "restart" },
  );
}
