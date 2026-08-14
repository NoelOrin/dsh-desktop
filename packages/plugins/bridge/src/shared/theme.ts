/** 主题与背景图共享模型（移植自 Deepseek-Harness-Desktop 的 ui-theme 包，纯函数，无外部依赖）。 */

// ── 类型 ──────────────────────────────────────────────────────────────────────

export type ThemeTokens = Record<string, string>;

export interface ThemeSeeds {
  accent: string;
  background: string;
  foreground: string;
  contrast: number;
  overrides?: Record<string, string>;
}

export interface ThemeFamily {
  id: string;
  name: string;
  origin: "builtin" | "custom";
  light: ThemeSeeds;
  dark: ThemeSeeds;
}

export const THEME_PREFERENCES = ["light", "dark", "system"] as const;
export type ThemePreference = (typeof THEME_PREFERENCES)[number];

/** ui-theme 命名空间持久化分节（与上游 dsh-client-ui-theme 的 schema 对齐）。 */
export interface ThemeSettings {
  preference: ThemePreference;
  activeLightThemeId: string;
  activeDarkThemeId: string;
  customThemes: ThemeFamily[];
  glassOpacity: number;
  wallpaperImage: string;
  wallpaperBlur: number;
  wallpaperPixelate: number;
  fontFamilySans: string;
  fontFamilyCode: string;
  fontSizeInterface: number;
  fontSizeCode: number;
  fontFamilyComposer: string;
  fontFamilyTerminal: string;
}

// ── 常量 ──────────────────────────────────────────────────────────────────────

export const DEFAULT_FAMILY_ID = "deepseek";
export const DEFAULT_PREFERENCE: ThemePreference = "system";
export const MIN_GLASS_OPACITY = 40;
export const MAX_GLASS_OPACITY = 100;
export const DEFAULT_GLASS_OPACITY = 80;
export const GLASS_OPACITY_STEP = 5;
export const MIN_INTERFACE_FONT_SIZE = 12;
export const MAX_INTERFACE_FONT_SIZE = 22;
export const DEFAULT_INTERFACE_FONT_SIZE = 16;
export const MIN_CODE_FONT_SIZE = 10;
export const MAX_CODE_FONT_SIZE = 20;
export const DEFAULT_CODE_FONT_SIZE = 13;
export const DEFAULT_CONTRAST = 46;
export const MIN_WALLPAPER_EFFECT = 0;
export const MAX_WALLPAPER_EFFECT = 100;
export const DEFAULT_WALLPAPER_EFFECT = 0;
export const WALLPAPER_EFFECT_STEP = 1;
export const MAX_WALLPAPER_DATA_URL_CHARS = 1_800_000;
export const MAX_WALLPAPER_EDGE = 1920;
export const WALLPAPER_LAYER_ID = "dsh-wallpaper";
export const WALLPAPER_INNER_ID = "dsh-wallpaper-inner";
export const WALLPAPER_ATTR = "data-dsh-wallpaper";
export const WALLPAPER_BLEED = 48;

export const THEME_SETTINGS_NAMESPACE = "ui-theme";
export const THEME_PREFERENCE_FIELD = "preference";
export const THEME_LIGHT_FAMILY_FIELD = "activeLightThemeId";
export const THEME_DARK_FAMILY_FIELD = "activeDarkThemeId";
export const THEME_CUSTOM_THEMES_FIELD = "customThemes";
export const THEME_GLASS_OPACITY_FIELD = "glassOpacity";
export const THEME_WALLPAPER_IMAGE_FIELD = "wallpaperImage";
export const THEME_WALLPAPER_BLUR_FIELD = "wallpaperBlur";
export const THEME_WALLPAPER_PIXELATE_FIELD = "wallpaperPixelate";

export const DEFAULT_THEME_SETTINGS: ThemeSettings = {
  preference: DEFAULT_PREFERENCE,
  activeLightThemeId: DEFAULT_FAMILY_ID,
  activeDarkThemeId: DEFAULT_FAMILY_ID,
  customThemes: [],
  glassOpacity: DEFAULT_GLASS_OPACITY,
  wallpaperImage: "",
  wallpaperBlur: DEFAULT_WALLPAPER_EFFECT,
  wallpaperPixelate: DEFAULT_WALLPAPER_EFFECT,
  fontFamilySans: "",
  fontFamilyCode: "",
  fontSizeInterface: DEFAULT_INTERFACE_FONT_SIZE,
  fontSizeCode: DEFAULT_CODE_FONT_SIZE,
  fontFamilyComposer: "",
  fontFamilyTerminal: "",
};

// ── 内置家族 ──────────────────────────────────────────────────────────────────

function seeds(
  accent: string,
  background: string,
  foreground: string,
  contrast = DEFAULT_CONTRAST,
): ThemeSeeds {
  return { accent, background, foreground, contrast };
}

function family(id: string, name: string, light: ThemeSeeds, dark: ThemeSeeds): ThemeFamily {
  return { id, name, origin: "builtin", light, dark };
}

/** 产品默认家族：不推导 token（空字典，样式表保持权威），种子仍驱动主题库色卡。 */
export const DEEPSEEK_FAMILY: ThemeFamily = family(
  DEFAULT_FAMILY_ID,
  "DeepSeek",
  seeds("#4176e6", "#ffffff", "#0f1115", 46),
  seeds("#6ea8ff", "#151517", "#f5f5f5", 41),
);

export const BUILTIN_THEME_FAMILIES: readonly ThemeFamily[] = Object.freeze([
  DEEPSEEK_FAMILY,
  family(
    "midnight",
    "午夜",
    seeds("#3b6fd4", "#f3f6fb", "#1a1f2b", 44),
    seeds("#6ea8ff", "#0b0d12", "#e8eef9", 48),
  ),
  family(
    "celadon",
    "青瓷",
    seeds("#0f766e", "#f3faf7", "#10211c", 44),
    seeds("#3dd6b5", "#071411", "#e7f6f1", 50),
  ),
  family(
    "violet",
    "暮紫",
    seeds("#7c3aed", "#f7f3fc", "#1c1524", 46),
    seeds("#c4a1ff", "#120e18", "#f3eefc", 50),
  ),
  family(
    "amber",
    "琥珀",
    seeds("#b45309", "#fbf6ee", "#1c1915", 48),
    seeds("#e2b15c", "#14100b", "#f6efe4", 52),
  ),
  family(
    "paper",
    "宣纸",
    seeds("#0f766e", "#f3efe6", "#1c1915", 50),
    seeds("#5eead4", "#1a1712", "#f6efe4", 48),
  ),
  family(
    "contrast",
    "对比",
    seeds("#111111", "#ffffff", "#050505", 68),
    seeds("#ffffff", "#050505", "#f5f5f5", 64),
  ),
]);

const BUILTIN_BY_ID = new Map(BUILTIN_THEME_FAMILIES.map((item) => [item.id, item]));

export function getBuiltinFamily(id: string): ThemeFamily | undefined {
  return BUILTIN_BY_ID.get(id);
}

export function isBuiltinFamilyId(id: string): boolean {
  return BUILTIN_BY_ID.has(id);
}

export function resolveThemeFamily(
  id: string,
  customThemes: ReadonlyArray<ThemeFamily>,
): ThemeFamily {
  return getBuiltinFamily(id) ?? customThemes.find((item) => item.id === id) ?? DEEPSEEK_FAMILY;
}

export function listThemeFamilies(customThemes: ReadonlyArray<ThemeFamily>): ThemeFamily[] {
  return [...BUILTIN_THEME_FAMILIES, ...customThemes];
}

export function getReservedThemeIds(): ReadonlySet<string> {
  return new Set(BUILTIN_BY_ID.keys());
}

// ── settings 分节解析 ─────────────────────────────────────────────────────────

export function isThemePreference(value: unknown): value is ThemePreference {
  return THEME_PREFERENCES.some((item) => item === value);
}

export function resolveThemeSettings(section: ThemeSettings | undefined): ThemeSettings {
  if (section === undefined) return { ...DEFAULT_THEME_SETTINGS, customThemes: [] };
  return { ...DEFAULT_THEME_SETTINGS, ...section, customThemes: section.customThemes ?? [] };
}

export function resolveMode(preference: ThemePreference, systemDark: boolean): "light" | "dark" {
  if (preference === "dark" || preference === "light") return preference;
  return systemDark ? "dark" : "light";
}

// ── 颜色推导（移植自参考 derive.ts）───────────────────────────────────────────

type Rgb = { r: number; g: number; b: number };

const STATUS_PALETTE = {
  light: { destructive: "#c53b2c", success: "#0a7d5d", warning: "#a95a00" },
  dark: { destructive: "#ff7b72", success: "#3fb950", warning: "#d29922" },
} as const;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function parseHexColor(hex: string): Rgb {
  const normalized = hex.replace("#", "");
  const expanded = normalized.length === 8 ? normalized.slice(0, 6) : normalized;
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
  };
}

function toHexColor({ r, g, b }: Rgb): string {
  const channel = (value: number) =>
    Math.round(clamp(value, 0, 255))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(r)}${channel(g)}${channel(b)}`;
}

function mixColors(left: string, right: string, ratio: number): string {
  const from = parseHexColor(left);
  const to = parseHexColor(right);
  const amount = clamp(ratio, 0, 1);
  return toHexColor({
    r: from.r + (to.r - from.r) * amount,
    g: from.g + (to.g - from.g) * amount,
    b: from.b + (to.b - from.b) * amount,
  });
}

function withAlpha(color: string, alpha: number): string {
  const { r, g, b } = parseHexColor(color);
  return `rgb(${r} ${g} ${b} / ${clamp(alpha, 0, 1).toFixed(3)})`;
}

function transformGammaChannel(channel: number): number {
  const normalized = channel / 255;
  return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
}

function getRelativeLuminance(color: string): number {
  const { r, g, b } = parseHexColor(color);
  return (
    0.2126 * transformGammaChannel(r) +
    0.7152 * transformGammaChannel(g) +
    0.0722 * transformGammaChannel(b)
  );
}

function getContrastRatio(left: string, right: string): number {
  const lighter = Math.max(getRelativeLuminance(left), getRelativeLuminance(right));
  const darker = Math.min(getRelativeLuminance(left), getRelativeLuminance(right));
  return (lighter + 0.05) / (darker + 0.05);
}

export function pickReadableText(background: string, candidates: readonly string[]): string {
  return (
    [...candidates].sort(
      (left, right) => getContrastRatio(background, right) - getContrastRatio(background, left),
    )[0] ??
    candidates[0] ??
    ""
  );
}

/** 由种子推导 `--dsw-alias-*` 变量字典（DeepSeek 家族返回空字典，样式表保持权威）。 */
export function deriveThemeTokens(seeds: ThemeSeeds): ThemeTokens {
  if (
    [DEEPSEEK_FAMILY.light, DEEPSEEK_FAMILY.dark].some(
      (item) =>
        item.accent === seeds.accent &&
        item.background === seeds.background &&
        item.foreground === seeds.foreground &&
        item.contrast === seeds.contrast,
    )
  ) {
    return {};
  }
  const contrastFactor = clamp(seeds.contrast / 100, 0, 1);
  const isDark = getRelativeLuminance(seeds.background) < getRelativeLuminance(seeds.foreground);
  const wash = isDark ? 0.14 + contrastFactor * 0.1 : 0.1 + contrastFactor * 0.08;
  const washStrong = isDark ? 0.26 + contrastFactor * 0.12 : 0.18 + contrastFactor * 0.1;
  const accentWash = mixColors(seeds.background, seeds.accent, wash);
  const accentWashStrong = mixColors(seeds.background, seeds.accent, washStrong);
  const card = mixColors(
    mixColors(
      seeds.background,
      seeds.foreground,
      isDark ? 0.02 + contrastFactor * 0.04 : 0.006 + contrastFactor * 0.016,
    ),
    seeds.accent,
    isDark ? 0.08 + contrastFactor * 0.05 : 0.05 + contrastFactor * 0.04,
  );
  const overlay = mixColors(
    mixColors(
      seeds.background,
      seeds.foreground,
      isDark ? 0.035 + contrastFactor * 0.05 : 0.012 + contrastFactor * 0.02,
    ),
    seeds.accent,
    isDark ? 0.1 + contrastFactor * 0.05 : 0.06 + contrastFactor * 0.04,
  );
  const layer2 = mixColors(
    mixColors(
      seeds.background,
      seeds.foreground,
      isDark ? 0.05 + contrastFactor * 0.05 : 0.02 + contrastFactor * 0.02,
    ),
    seeds.accent,
    isDark ? 0.12 + contrastFactor * 0.06 : 0.07 + contrastFactor * 0.04,
  );
  const mutedForeground = mixColors(
    seeds.foreground,
    seeds.background,
    0.38 - contrastFactor * 0.14,
  );
  const border = withAlpha(seeds.foreground, 0.08 + contrastFactor * 0.18);
  const input = withAlpha(seeds.foreground, 0.1 + contrastFactor * 0.2);
  const sidebar = mixColors(seeds.background, seeds.accent, washStrong);
  const accentHover = mixColors(seeds.accent, isDark ? "#ffffff" : "#000000", 0.18);
  const onAccent = pickReadableText(seeds.accent, [
    seeds.foreground,
    seeds.background,
    "#ffffff",
    "#0f1115",
  ]);
  const status = isDark ? STATUS_PALETTE.dark : STATUS_PALETTE.light;
  const tokens: ThemeTokens = {
    "--dsw-alias-bg-base": seeds.background,
    "--dsw-alias-bg-layer-1": card,
    "--dsw-alias-bg-layer-2": layer2,
    "--dsw-alias-bg-overlay": overlay,
    "--dsw-alias-label-primary": seeds.foreground,
    "--dsw-alias-label-secondary": mutedForeground,
    "--dsw-alias-label-primary-foreground": onAccent,
    "--dsw-alias-brand-primary": seeds.accent,
    "--dsw-alias-brand-primary-invert": onAccent,
    "--dsw-alias-brand-text": seeds.accent,
    "--dsw-alias-brand-primary-new-colorprimary-new-color": seeds.accent,
    "--dsw-alias-button-primary-fill": seeds.accent,
    "--dsw-alias-button-primary-hover": accentHover,
    "--dsw-alias-button-info-fill": seeds.accent,
    "--dsw-alias-button-info-hover": accentHover,
    "--dsw-alias-state-business-primary": seeds.accent,
    "--dsw-alias-state-business-tertiary": accentWash,
    "--dsw-alias-border-l1": border,
    "--dsw-alias-border-l2": input,
    "--dsw-alias-state-error-primary": status.destructive,
    "--dsw-alias-state-success-primary": status.success,
    "--dsw-alias-state-warn-primary": status.warning,
    "--dsw-specific-bubble": accentWash,
    "--dsw-specific-bubble-highlight": accentWashStrong,
    "--dsw-specific-sidebar-fill": sidebar,
    "--dsw-specific-sidebar-nav-item-active": accentWash,
    "--dsw-specific-sidebar-nav-item-active-accent": accentWashStrong,
    "--dsw-alias-interactive-bg-hover-accent": withAlpha(seeds.accent, isDark ? 0.22 : 0.14),
  };
  if (seeds.overrides) {
    for (const [name, value] of Object.entries(seeds.overrides)) {
      if (value) tokens[name] = value;
    }
  }
  return tokens;
}

// ── 自定义主题 CRUD（移植自参考 theme-family.ts）────────────────────────────

const HEX_COLOR = /^#(?:[0-9a-fA-F]{6})$/;

export function normalizeHexColor(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return HEX_COLOR.test(trimmed) ? trimmed.toLowerCase() : undefined;
}

export function slugifyThemeId(value: string): string {
  const slug = value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9\p{L}]+/gu, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "custom-theme";
}

export function ensureUniqueThemeId(baseId: string, existingIds: ReadonlySet<string>): string {
  if (!existingIds.has(baseId)) return baseId;
  let index = 2;
  while (existingIds.has(`${baseId}-${index}`)) index += 1;
  return `${baseId}-${index}`;
}

export function duplicateThemeFamily(
  family: ThemeFamily,
  existingIds: ReadonlySet<string>,
): ThemeFamily {
  const nextId = ensureUniqueThemeId(slugifyThemeId(`${family.id}-copy`), existingIds);
  const suffix =
    nextId === `${slugifyThemeId(family.id)}-copy`
      ? " Copy"
      : ` Copy ${nextId.split("-").at(-1) ?? ""}`;
  return canonicalizeThemeFamily(
    { ...family, id: nextId, name: family.name + suffix, origin: "custom" },
    "custom",
  );
}

export function normalizeImportedThemeFamily(
  family: ThemeFamily,
  existingIds: ReadonlySet<string>,
): ThemeFamily {
  const uniqueId = ensureUniqueThemeId(slugifyThemeId(family.id || family.name), existingIds);
  return canonicalizeThemeFamily({ ...family, id: uniqueId, origin: "custom" }, "custom");
}

export function replaceCustomTheme(
  customThemes: ReadonlyArray<ThemeFamily>,
  nextTheme: ThemeFamily,
): ThemeFamily[] {
  const canonical = canonicalizeThemeFamily(nextTheme, "custom");
  return [...customThemes.filter((item) => item.id !== canonical.id), canonical];
}

export function canonicalizeThemeFamily(
  family: ThemeFamily,
  origin: ThemeFamily["origin"] = family.origin,
): ThemeFamily {
  return {
    id: family.id,
    name: family.name,
    origin,
    light: canonicalizeSeeds(family.light),
    dark: canonicalizeSeeds(family.dark),
  };
}

function canonicalizeSeeds(s: ThemeSeeds): ThemeSeeds {
  const overrides = s.overrides
    ? Object.fromEntries(Object.entries(s.overrides).filter(([, value]) => value !== ""))
    : undefined;
  return {
    accent: s.accent,
    background: s.background,
    foreground: s.foreground,
    contrast: s.contrast,
    ...(overrides && Object.keys(overrides).length > 0 ? { overrides } : {}),
  };
}

export function serializeThemeFamily(family: ThemeFamily): string {
  return `${JSON.stringify(canonicalizeThemeFamily(family), null, 2)}\n`;
}

export function parseThemeFamilyJson(raw: string): ThemeFamily {
  const doc: unknown = JSON.parse(raw);
  if (typeof doc !== "object" || doc === null) throw new Error("主题 JSON 必须是对象");
  const value = doc as { id?: unknown; name?: unknown; light?: unknown; dark?: unknown };
  if (
    typeof value.id !== "string" ||
    typeof value.name !== "string" ||
    typeof value.light !== "object" ||
    value.light === null ||
    typeof value.dark !== "object" ||
    value.dark === null
  ) {
    throw new Error("主题 JSON 缺少 id/name/light/dark");
  }
  return doc as ThemeFamily;
}

// ── 背景图（移植自参考 wallpaper.ts）──────────────────────────────────────────

const ALLOWED_TYPES = new Set(["image/png", "image/jpeg", "image/jpg", "image/webp", "image/gif"]);
const DATA_URL = /^data:image\/(?:png|jpe?g|webp|gif);base64,[A-Za-z0-9+/=\s]+$/i;

export function clampWallpaperEffect(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_WALLPAPER_EFFECT;
  return Math.min(MAX_WALLPAPER_EFFECT, Math.max(MIN_WALLPAPER_EFFECT, Math.round(value)));
}

export function wallpaperBlurPx(percent: number): number {
  return (clampWallpaperEffect(percent) / MAX_WALLPAPER_EFFECT) * 40;
}

export function wallpaperPixelFactor(percent: number): number {
  const clamped = clampWallpaperEffect(percent);
  return clamped <= 0 ? 1 : 1 + (clamped / MAX_WALLPAPER_EFFECT) * 19;
}

export function isWallpaperDataUrl(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0) return false;
  if (value.length > MAX_WALLPAPER_DATA_URL_CHARS) return false;
  return DATA_URL.test(value);
}

export function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

export function downscaleWallpaper(dataUrl: string): Promise<string | null> {
  return new Promise((resolve) => {
    if (typeof Image === "undefined" || typeof document === "undefined") {
      resolve(null);
      return;
    }
    const image = new Image();
    let settled = false;
    const finish = (value: string | null): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      image.onload = null;
      image.onerror = null;
      resolve(value);
    };
    const timer = setTimeout(() => {
      finish(null);
    }, 200);
    const paint = (): void => {
      const width = image.naturalWidth || image.width;
      const height = image.naturalHeight || image.height;
      if (!width || !height) {
        finish(null);
        return;
      }
      const scale = Math.min(1, MAX_WALLPAPER_EDGE / Math.max(width, height));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(width * scale));
      canvas.height = Math.max(1, Math.round(height * scale));
      const context = canvas.getContext("2d");
      if (context === null) {
        finish(null);
        return;
      }
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      try {
        const jpeg = canvas.toDataURL("image/jpeg", 0.82);
        finish(isWallpaperDataUrl(jpeg) ? jpeg : null);
      } catch {
        finish(null);
      }
    };
    image.onload = paint;
    image.onerror = () => {
      finish(null);
    };
    image.src = dataUrl;
    if (image.complete) paint();
  });
}

export async function encodeWallpaperFile(file: File): Promise<string | null> {
  const named = /\.(png|jpe?g|webp|gif)$/i.test(file.name);
  if (!ALLOWED_TYPES.has(file.type) && !named) return null;
  let raw: string;
  try {
    raw = await readFileAsDataUrl(file);
  } catch {
    return null;
  }
  if (!raw.startsWith("data:image/")) return null;
  const resized = await downscaleWallpaper(raw);
  const next = resized ?? (isWallpaperDataUrl(raw) ? raw : null);
  if (next === null || next.length > MAX_WALLPAPER_DATA_URL_CHARS) return null;
  return next;
}

export function wallpaperCanvasSolidity(solidity: number): number {
  const kept = Math.min(100, Math.max(0, Math.round(solidity)));
  if (kept <= 40) return Math.round(kept * 0.375);
  if (kept <= 80) return Math.round(15 + (kept - 40) * 0.75);
  return Math.round(45 + (kept - 80) * 2.75);
}

/** 主要表面 token 半透明化，让背景透出来（玻璃透明度）。 */
export function mixWallpaperSurfaces(
  tokens: ThemeTokens,
  mode: "light" | "dark",
  solidity: number,
): ThemeTokens {
  const next: ThemeTokens = { ...tokens };
  const kept = Math.min(100, Math.max(0, Math.round(solidity)));
  const canvas = wallpaperCanvasSolidity(kept);
  const sidebar = Math.round((canvas + kept) / 2);
  const base =
    mode === "dark"
      ? "var(--dsw-static-neutral-bluish-950)"
      : "var(--dsw-static-neutral-bluish-00)";
  const raised =
    mode === "dark"
      ? "var(--dsw-static-neutral-bluish-875)"
      : "var(--dsw-static-neutral-bluish-00)";
  const surfaces: Record<string, { fallback: string; percent: number }> = {
    "--dsw-alias-bg-base": { fallback: base, percent: canvas },
    "--dsw-alias-bg-layer-1": { fallback: raised, percent: kept },
    "--dsw-alias-bg-layer-2": { fallback: raised, percent: kept },
    "--dsw-specific-sidebar-fill": { fallback: raised, percent: sidebar },
  };
  for (const [name, { fallback, percent }] of Object.entries(surfaces)) {
    const current = next[name];
    const solid = current !== undefined && !current.includes("color-mix") ? current : fallback;
    next[name] = `color-mix(in srgb, ${solid} ${percent}%, transparent)`;
  }
  return next;
}

// ── 排版（移植自参考 appearance-apply.ts）────────────────────────────────────

export const DEFAULT_SANS_STACK =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Helvetica Neue', Helvetica, Arial, sans-serif";
export const DEFAULT_CODE_STACK =
  "'SF Mono', 'JetBrains Mono', 'Fira Code', Consolas, 'Liberation Mono', Menlo, Courier, 'PingFang SC', 'Microsoft YaHei'";

export function quoteFontFamilyName(name: string): string {
  const bare = name.trim();
  if (bare.length === 0) return "";
  if (/^(["']).*\1$/.test(bare)) return bare;
  if (/^[a-zA-Z][a-zA-Z0-9-]*$/.test(bare)) return bare;
  return `"${bare.replaceAll('"', "")}"`;
}

export function cssFontFamilies(input: string): string | null {
  const families = input
    .split(",")
    .map(quoteFontFamilyName)
    .filter((name) => name.length > 0);
  return families.length > 0 ? families.join(", ") : null;
}

export function appearanceFontStack(custom: string, defaultStack: string): string {
  const families = cssFontFamilies(custom);
  return families === null ? defaultStack : `${families}, ${defaultStack}`;
}
