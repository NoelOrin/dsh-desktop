
# 主题与背景图 + 无边框自绘标题栏（参照 Deepseek-Harness-Desktop）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 DSH Desktop（Tauri）中复刻参考仓库 [Deepseek-Harness-Desktop](https://github.com/ChisaAlter/Deepseek-Harness-Desktop) 的三块能力：① 主题与背景图（dsh WebUI 设置新增“外观”设置节：主题偏好 / 主题库 / 自定义主题 / 背景图 / 玻璃透明度 / 排版，持久化到 $DSH_HOME/settings.yaml 的 `ui-theme` 分节并实时生效；启动页与窗口背景跟随）；② 无边框窗口 + 自绘标题栏（可拖动、双击最大化，最小化/最大化/关闭按钮齐全，标题栏背景跟随主题）。

**Architecture:** 能力按既有边界分三层落地。① **dsh 插件域**（`packages/plugins/bridge` 的 client 面）：新增“外观”设置节，复用上游 `dsh-client-ui-theme` 已注册的 `ui-theme` settings 命名空间（只 bind 不注册），把主题推导成 `--dsw-alias-*` CSS 变量写到 `body`、切换 `body[data-ds-dark-theme]`、注入背景图层（canvas cover 裁剪 + blur + 像素化）并做玻璃表面半透明——纯函数移植自参考仓库。② **Tauri 壳域**（Rust + 启动页）：Rust 读 `settings.yaml` 的 `ui-theme` 分节解析 token，新增 `get_ui_theme` 命令与 `dsh-ui-theme` 事件；窗口改为无边框，新增 `window_action` 命令与 `dsh-window-state` 事件，启动页自绘标题栏，并向 dsh web 注入自绘标题栏脚本（`HARNESS_CHROME_SCRIPT`，Tauri 用 `data-tauri-drag-region`，参考 harness-chrome-inject.js）。

**Tech Stack:** Tauri 2（Rust，新增 `serde_yaml`）、TypeScript strict、dsh cordis 插件生态（`settingsScope` / `slots` / `locale`）、React 18（client 面）、node:test（零依赖纯函数测试）。

**Spec:** `docs/superpowers/specs/2026-08-14-ui-theme-wallpaper-spec.md`（本计划由此推演；参考实现为 Deepseek-Harness-Desktop 的 `vendor/deepseek-harness/packages/client/ui-theme`、`src/shared/themes.js`、`src/main/chrome.js`、`src/main/harness-chrome-inject.js` 与 `src/renderer/window-controls.*`）。

## Global Constraints

- 上游 `deepseek-ai/dsh@0.1.0-rc.6` 已装配 `dsh-client-ui-theme`（web profile 的 `ui-theme` 行）：host 已注册 `ui-theme` 命名空间、已提供 `--dsw-*` 样式表与 `body[data-ds-dark-theme]` 基础调色板。**bridge 只 bind `ui-theme` 命名空间，绝不重复注册**（重复注册会 fail loud）
- 用户可见文案与代码注释使用中文；前后端固定契约通信，字段命名 snake_case
- 新增 native IPC 必须同步四处：`packages/contracts/src/index.ts`、Rust serde 类型（`lib.rs` / `theme.rs`）、`apps/shell/src/main.ts`、`apps/shell/src-tauri/capabilities/*.json`；应用命令 ACL 由 `apps/shell/src-tauri/build.rs` 的 `AppManifest::commands` 生成，新增命令需同步该数组
- 桥接能力只经 `capabilities/bridge.json` remote 白名单开放最小切片：`get_ui_theme` 是 native-only（只进 `default.json`）；`window_action` 是本地窗口与 dsh web 都要用的命令（同时进 `default.json` 与 `bridge.json`）
- Tauri 拖动：`data-tauri-drag-region` 属性（值 `deep` 表示整棵子树可拖，可点击元素自动阻断拖拽；双击区域自动最大化，由 Tauri 注入的 drag.js 处理，无需 ACL）；**不用** `-webkit-app-region`
- client 面构建纯度门禁：`deepseek-ai/*` 值导入只允许平台模块（`tsdown.config.ts` 的 `CLIENT_EXTERNALS`）；共享纯逻辑放插件包内部模块（`src/shared/theme.ts`），随 client 内联打包
- 保持 `cargo fmt` / `cargo clippy` 零警告；`yarn typecheck` 全绿；提交遵循 Conventional Commits
- `dist/`、`gen/`、`target/`、插件 `lib/`（构建产物）为生成产物，勿手改
- 无边框后：main 窗口关闭仍走“关闭到托盘”（CloseRequested 拦截仅隐藏）；托盘“退出”才真正退出

---

### Task 1: 共享主题模型（内置家族 / token 推导 / 背景图 / 排版工具）

**Files:**
- Create: `packages/plugins/bridge/src/shared/theme.ts`
- Create: `packages/plugins/bridge/tests/theme.test.mjs`
- Modify: `packages/plugins/bridge/tsdown.config.ts`（新增 `shared` 构建入口）
- Modify: `packages/plugins/bridge/package.json`（`exports` 加 `./shared`；scripts 加 `test`）

**Interfaces:**
- Consumes: 无（纯 TS，零外部依赖；bridge tsconfig 已含 DOM lib）
- Produces: 后续所有任务依赖的共享模块 API（精确符号名见下）：类型 `ThemeSeeds`/`ThemeFamily`/`ThemePreference`/`ThemeSettings`/`ThemeTokens`；常量 `DEFAULT_FAMILY_ID`/`DEFAULT_PREFERENCE`/`DEFAULT_THEME_SETTINGS`/`BUILTIN_THEME_FAMILIES` 与 settings 字段名常量；函数 `resolveThemeSettings`/`resolveMode`/`resolveThemeFamily`/`listThemeFamilies`/`getReservedThemeIds`/`deriveThemeTokens`/`slugifyThemeId`/`ensureUniqueThemeId`/`duplicateThemeFamily`/`normalizeImportedThemeFamily`/`replaceCustomTheme`/`canonicalizeThemeFamily`/`serializeThemeFamily`/`parseThemeFamilyJson`/`clampWallpaperEffect`/`wallpaperBlurPx`/`wallpaperPixelFactor`/`isWallpaperDataUrl`/`readFileAsDataUrl`/`downscaleWallpaper`/`encodeWallpaperFile`/`wallpaperCanvasSolidity`/`mixWallpaperSurfaces`/`quoteFontFamilyName`/`cssFontFamilies`/`appearanceFontStack`，以及背景图层常量 `WALLPAPER_LAYER_ID`/`WALLPAPER_INNER_ID`/`WALLPAPER_ATTR`/`WALLPAPER_BLEED`

- [ ] **Step 1: 写共享主题模型 `src/shared/theme.ts`**

```ts
/** 主题与背景图共享模型（移植自 Deepseek-Harness-Desktop 的 ui-theme 包，纯函数，无外部依赖）。 */

// ── 类型 ──────────────────────────────────────────────────────────────────────

export type ThemeTokens = Record<string, string>

export interface ThemeSeeds {
  accent: string
  background: string
  foreground: string
  contrast: number
  overrides?: Record<string, string>
}

export interface ThemeFamily {
  id: string
  name: string
  origin: 'builtin' | 'custom'
  light: ThemeSeeds
  dark: ThemeSeeds
}

export const THEME_PREFERENCES = ['light', 'dark', 'system'] as const
export type ThemePreference = typeof THEME_PREFERENCES[number]

/** ui-theme 命名空间持久化分节（与上游 dsh-client-ui-theme 的 schema 对齐）。 */
export interface ThemeSettings {
  preference: ThemePreference
  activeLightThemeId: string
  activeDarkThemeId: string
  customThemes: ThemeFamily[]
  glassOpacity: number
  wallpaperImage: string
  wallpaperBlur: number
  wallpaperPixelate: number
  fontFamilySans: string
  fontFamilyCode: string
  fontSizeInterface: number
  fontSizeCode: number
  fontFamilyComposer: string
  fontFamilyTerminal: string
}

// ── 常量 ──────────────────────────────────────────────────────────────────────

export const DEFAULT_FAMILY_ID = 'deepseek'
export const DEFAULT_PREFERENCE: ThemePreference = 'system'
export const MIN_GLASS_OPACITY = 40
export const MAX_GLASS_OPACITY = 100
export const DEFAULT_GLASS_OPACITY = 80
export const GLASS_OPACITY_STEP = 5
export const MIN_INTERFACE_FONT_SIZE = 12
export const MAX_INTERFACE_FONT_SIZE = 22
export const DEFAULT_INTERFACE_FONT_SIZE = 16
export const MIN_CODE_FONT_SIZE = 10
export const MAX_CODE_FONT_SIZE = 20
export const DEFAULT_CODE_FONT_SIZE = 13
export const DEFAULT_CONTRAST = 46
export const MIN_WALLPAPER_EFFECT = 0
export const MAX_WALLPAPER_EFFECT = 100
export const DEFAULT_WALLPAPER_EFFECT = 0
export const WALLPAPER_EFFECT_STEP = 1
export const MAX_WALLPAPER_DATA_URL_CHARS = 1_800_000
export const MAX_WALLPAPER_EDGE = 1920
export const WALLPAPER_LAYER_ID = 'dsh-wallpaper'
export const WALLPAPER_INNER_ID = 'dsh-wallpaper-inner'
export const WALLPAPER_ATTR = 'data-dsh-wallpaper'
export const WALLPAPER_BLEED = 48

export const THEME_SETTINGS_NAMESPACE = 'ui-theme'
export const THEME_PREFERENCE_FIELD = 'preference'
export const THEME_LIGHT_FAMILY_FIELD = 'activeLightThemeId'
export const THEME_DARK_FAMILY_FIELD = 'activeDarkThemeId'
export const THEME_CUSTOM_THEMES_FIELD = 'customThemes'
export const THEME_GLASS_OPACITY_FIELD = 'glassOpacity'
export const THEME_WALLPAPER_IMAGE_FIELD = 'wallpaperImage'
export const THEME_WALLPAPER_BLUR_FIELD = 'wallpaperBlur'
export const THEME_WALLPAPER_PIXELATE_FIELD = 'wallpaperPixelate'

export const DEFAULT_THEME_SETTINGS: ThemeSettings = {
  preference: DEFAULT_PREFERENCE,
  activeLightThemeId: DEFAULT_FAMILY_ID,
  activeDarkThemeId: DEFAULT_FAMILY_ID,
  customThemes: [],
  glassOpacity: DEFAULT_GLASS_OPACITY,
  wallpaperImage: '',
  wallpaperBlur: DEFAULT_WALLPAPER_EFFECT,
  wallpaperPixelate: DEFAULT_WALLPAPER_EFFECT,
  fontFamilySans: '',
  fontFamilyCode: '',
  fontSizeInterface: DEFAULT_INTERFACE_FONT_SIZE,
  fontSizeCode: DEFAULT_CODE_FONT_SIZE,
  fontFamilyComposer: '',
  fontFamilyTerminal: '',
}

// ── 内置家族 ──────────────────────────────────────────────────────────────────

function seeds(accent: string, background: string, foreground: string, contrast = DEFAULT_CONTRAST): ThemeSeeds {
  return { accent, background, foreground, contrast }
}

function family(id: string, name: string, light: ThemeSeeds, dark: ThemeSeeds): ThemeFamily {
  return { id, name, origin: 'builtin', light, dark }
}

/** 产品默认家族：不推导 token（空字典，样式表保持权威），种子仍驱动主题库色卡。 */
export const DEEPSEEK_FAMILY: ThemeFamily = family(
  DEFAULT_FAMILY_ID,
  'DeepSeek',
  seeds('#4176e6', '#ffffff', '#0f1115', 46),
  seeds('#6ea8ff', '#151517', '#f5f5f5', 41),
)

export const BUILTIN_THEME_FAMILIES: readonly ThemeFamily[] = Object.freeze([
  DEEPSEEK_FAMILY,
  family('midnight', '午夜', seeds('#3b6fd4', '#f3f6fb', '#1a1f2b', 44), seeds('#6ea8ff', '#0b0d12', '#e8eef9', 48)),
  family('celadon', '青瓷', seeds('#0f766e', '#f3faf7', '#10211c', 44), seeds('#3dd6b5', '#071411', '#e7f6f1', 50)),
  family('violet', '暮紫', seeds('#7c3aed', '#f7f3fc', '#1c1524', 46), seeds('#c4a1ff', '#120e18', '#f3eefc', 50)),
  family('amber', '琥珀', seeds('#b45309', '#fbf6ee', '#1c1915', 48), seeds('#e2b15c', '#14100b', '#f6efe4', 52)),
  family('paper', '宣纸', seeds('#0f766e', '#f3efe6', '#1c1915', 50), seeds('#5eead4', '#1a1712', '#f6efe4', 48)),
  family('contrast', '对比', seeds('#111111', '#ffffff', '#050505', 68), seeds('#ffffff', '#050505', '#f5f5f5', 64)),
])

const BUILTIN_BY_ID = new Map(BUILTIN_THEME_FAMILIES.map((item) => [item.id, item]))

export function getBuiltinFamily(id: string): ThemeFamily | undefined {
  return BUILTIN_BY_ID.get(id)
}

export function isBuiltinFamilyId(id: string): boolean {
  return BUILTIN_BY_ID.has(id)
}

export function resolveThemeFamily(id: string, customThemes: ReadonlyArray<ThemeFamily>): ThemeFamily {
  return getBuiltinFamily(id) ?? customThemes.find((item) => item.id === id) ?? DEEPSEEK_FAMILY
}

export function listThemeFamilies(customThemes: ReadonlyArray<ThemeFamily>): ThemeFamily[] {
  return [...BUILTIN_THEME_FAMILIES, ...customThemes]
}

export function getReservedThemeIds(): ReadonlySet<string> {
  return new Set(BUILTIN_BY_ID.keys())
}

// ── settings 分节解析 ─────────────────────────────────────────────────────────

export function isThemePreference(value: unknown): value is ThemePreference {
  return THEME_PREFERENCES.some((item) => item === value)
}

export function resolveThemeSettings(section: ThemeSettings | undefined): ThemeSettings {
  if (section === undefined) return { ...DEFAULT_THEME_SETTINGS, customThemes: [] }
  return { ...DEFAULT_THEME_SETTINGS, ...section, customThemes: section.customThemes ?? [] }
}

export function resolveMode(preference: ThemePreference, systemDark: boolean): 'light' | 'dark' {
  if (preference === 'dark' || preference === 'light') return preference
  return systemDark ? 'dark' : 'light'
}

// ── 颜色推导（移植自参考 derive.ts）───────────────────────────────────────────

type Rgb = { r: number; g: number; b: number }

const STATUS_PALETTE = {
  light: { destructive: '#c53b2c', success: '#0a7d5d', warning: '#a95a00' },
  dark: { destructive: '#ff7b72', success: '#3fb950', warning: '#d29922' },
} as const

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function parseHexColor(hex: string): Rgb {
  const normalized = hex.replace('#', '')
  const expanded = normalized.length === 8 ? normalized.slice(0, 6) : normalized
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
  }
}

function toHexColor({ r, g, b }: Rgb): string {
  const channel = (value: number) => Math.round(clamp(value, 0, 255)).toString(16).padStart(2, '0')
  return '#' + channel(r) + channel(g) + channel(b)
}

function mixColors(left: string, right: string, ratio: number): string {
  const from = parseHexColor(left)
  const to = parseHexColor(right)
  const amount = clamp(ratio, 0, 1)
  return toHexColor({
    r: from.r + (to.r - from.r) * amount,
    g: from.g + (to.g - from.g) * amount,
    b: from.b + (to.b - from.b) * amount,
  })
}

function withAlpha(color: string, alpha: number): string {
  const { r, g, b } = parseHexColor(color)
  return 'rgb(' + r + ' ' + g + ' ' + b + ' / ' + clamp(alpha, 0, 1).toFixed(3) + ')'
}

function transformGammaChannel(channel: number): number {
  const normalized = channel / 255
  return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4
}

function getRelativeLuminance(color: string): number {
  const { r, g, b } = parseHexColor(color)
  return 0.2126 * transformGammaChannel(r) + 0.7152 * transformGammaChannel(g) + 0.0722 * transformGammaChannel(b)
}

function getContrastRatio(left: string, right: string): number {
  const lighter = Math.max(getRelativeLuminance(left), getRelativeLuminance(right))
  const darker = Math.min(getRelativeLuminance(left), getRelativeLuminance(right))
  return (lighter + 0.05) / (darker + 0.05)
}

export function pickReadableText(background: string, candidates: readonly string[]): string {
  return candidates.toSorted(
    (left, right) => getContrastRatio(background, right) - getContrastRatio(background, left),
  )[0] ?? candidates[0]!
}

/** 由种子推导 `--dsw-alias-*` 变量字典（DeepSeek 家族返回空字典，样式表保持权威）。 */
export function deriveThemeTokens(seeds: ThemeSeeds): ThemeTokens {
  const contrastFactor = clamp(seeds.contrast / 100, 0, 1)
  const isDark = getRelativeLuminance(seeds.background) < getRelativeLuminance(seeds.foreground)
  const wash = isDark ? 0.14 + contrastFactor * 0.1 : 0.1 + contrastFactor * 0.08
  const washStrong = isDark ? 0.26 + contrastFactor * 0.12 : 0.18 + contrastFactor * 0.1
  const accentWash = mixColors(seeds.background, seeds.accent, wash)
  const accentWashStrong = mixColors(seeds.background, seeds.accent, washStrong)
  const card = mixColors(mixColors(seeds.background, seeds.foreground, isDark ? 0.02 + contrastFactor * 0.04 : 0.006 + contrastFactor * 0.016), seeds.accent, isDark ? 0.08 + contrastFactor * 0.05 : 0.05 + contrastFactor * 0.04)
  const overlay = mixColors(mixColors(seeds.background, seeds.foreground, isDark ? 0.035 + contrastFactor * 0.05 : 0.012 + contrastFactor * 0.02), seeds.accent, isDark ? 0.1 + contrastFactor * 0.05 : 0.06 + contrastFactor * 0.04)
  const layer2 = mixColors(mixColors(seeds.background, seeds.foreground, isDark ? 0.05 + contrastFactor * 0.05 : 0.02 + contrastFactor * 0.02), seeds.accent, isDark ? 0.12 + contrastFactor * 0.06 : 0.07 + contrastFactor * 0.04)
  const mutedForeground = mixColors(seeds.foreground, seeds.background, 0.38 - contrastFactor * 0.14)
  const border = withAlpha(seeds.foreground, 0.08 + contrastFactor * 0.18)
  const input = withAlpha(seeds.foreground, 0.1 + contrastFactor * 0.2)
  const sidebar = mixColors(seeds.background, seeds.accent, washStrong)
  const accentHover = mixColors(seeds.accent, isDark ? '#ffffff' : '#000000', 0.18)
  const onAccent = pickReadableText(seeds.accent, [seeds.foreground, seeds.background, '#ffffff', '#0f1115'])
  const status = isDark ? STATUS_PALETTE.dark : STATUS_PALETTE.light
  const tokens: ThemeTokens = {
    '--dsw-alias-bg-base': seeds.background,
    '--dsw-alias-bg-layer-1': card,
    '--dsw-alias-bg-layer-2': layer2,
    '--dsw-alias-bg-overlay': overlay,
    '--dsw-alias-label-primary': seeds.foreground,
    '--dsw-alias-label-secondary': mutedForeground,
    '--dsw-alias-label-primary-foreground': onAccent,
    '--dsw-alias-brand-primary': seeds.accent,
    '--dsw-alias-brand-primary-invert': onAccent,
    '--dsw-alias-brand-text': seeds.accent,
    '--dsw-alias-brand-primary-new-colorprimary-new-color': seeds.accent,
    '--dsw-alias-button-primary-fill': seeds.accent,
    '--dsw-alias-button-primary-hover': accentHover,
    '--dsw-alias-button-info-fill': seeds.accent,
    '--dsw-alias-button-info-hover': accentHover,
    '--dsw-alias-state-business-primary': seeds.accent,
    '--dsw-alias-state-business-tertiary': accentWash,
    '--dsw-alias-border-l1': border,
    '--dsw-alias-border-l2': input,
    '--dsw-alias-state-error-primary': status.destructive,
    '--dsw-alias-state-success-primary': status.success,
    '--dsw-alias-state-warn-primary': status.warning,
    '--dsw-specific-bubble': accentWash,
    '--dsw-specific-bubble-highlight': accentWashStrong,
    '--dsw-specific-sidebar-fill': sidebar,
    '--dsw-specific-sidebar-nav-item-active': accentWash,
    '--dsw-specific-sidebar-nav-item-active-accent': accentWashStrong,
    '--dsw-alias-interactive-bg-hover-accent': withAlpha(seeds.accent, isDark ? 0.22 : 0.14),
  }
  if (seeds.overrides) {
    for (const [name, value] of Object.entries(seeds.overrides)) {
      if (value) tokens[name] = value
    }
  }
  return tokens
}

// ── 自定义主题 CRUD（移植自参考 theme-family.ts）────────────────────────────

const HEX_COLOR = /^#(?:[0-9a-fA-F]{6})$/

export function normalizeHexColor(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined
  const trimmed = value.trim()
  return HEX_COLOR.test(trimmed) ? trimmed.toLowerCase() : undefined
}

export function slugifyThemeId(value: string): string {
  const slug = value.toLowerCase().trim().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
  return slug || 'custom-theme'
}

export function ensureUniqueThemeId(baseId: string, existingIds: ReadonlySet<string>): string {
  if (!existingIds.has(baseId)) return baseId
  let index = 2
  while (existingIds.has(baseId + '-' + index)) index += 1
  return baseId + '-' + index
}

export function duplicateThemeFamily(family: ThemeFamily, existingIds: ReadonlySet<string>): ThemeFamily {
  const nextId = ensureUniqueThemeId(slugifyThemeId(family.id + '-copy'), existingIds)
  const suffix = nextId === slugifyThemeId(family.id) + '-copy' ? ' Copy' : ' Copy ' + (nextId.split('-').at(-1) ?? '')
  return canonicalizeThemeFamily({ ...family, id: nextId, name: family.name + suffix, origin: 'custom' }, 'custom')
}

export function normalizeImportedThemeFamily(family: ThemeFamily, existingIds: ReadonlySet<string>): ThemeFamily {
  const uniqueId = ensureUniqueThemeId(slugifyThemeId(family.id || family.name), existingIds)
  return canonicalizeThemeFamily({ ...family, id: uniqueId, origin: 'custom' }, 'custom')
}

export function replaceCustomTheme(customThemes: ReadonlyArray<ThemeFamily>, nextTheme: ThemeFamily): ThemeFamily[] {
  const canonical = canonicalizeThemeFamily(nextTheme, 'custom')
  return [...customThemes.filter((item) => item.id !== canonical.id), canonical]
}

export function canonicalizeThemeFamily(family: ThemeFamily, origin: ThemeFamily['origin'] = family.origin): ThemeFamily {
  return {
    id: family.id,
    name: family.name,
    origin,
    light: canonicalizeSeeds(family.light),
    dark: canonicalizeSeeds(family.dark),
  }
}

function canonicalizeSeeds(s: ThemeSeeds): ThemeSeeds {
  const overrides = s.overrides
    ? Object.fromEntries(Object.entries(s.overrides).filter(([, value]) => value !== ''))
    : undefined
  return {
    accent: s.accent,
    background: s.background,
    foreground: s.foreground,
    contrast: s.contrast,
    ...(overrides && Object.keys(overrides).length > 0 ? { overrides } : {}),
  }
}

export function serializeThemeFamily(family: ThemeFamily): string {
  return JSON.stringify(canonicalizeThemeFamily(family), null, 2) + '\n'
}

export function parseThemeFamilyJson(raw: string): ThemeFamily {
  const doc: unknown = JSON.parse(raw)
  if (typeof doc !== 'object' || doc === null) throw new Error('主题 JSON 必须是对象')
  const value = doc as { id?: unknown; name?: unknown; light?: unknown; dark?: unknown }
  if (typeof value.id !== 'string' || typeof value.name !== 'string'
    || typeof value.light !== 'object' || value.light === null
    || typeof value.dark !== 'object' || value.dark === null) {
    throw new Error('主题 JSON 缺少 id/name/light/dark')
  }
  return doc as ThemeFamily
}

// ── 背景图（移植自参考 wallpaper.ts）──────────────────────────────────────────

const ALLOWED_TYPES = new Set(['image/png', 'image/jpeg', 'image/jpg', 'image/webp', 'image/gif'])
const DATA_URL = /^data:image\/(?:png|jpe?g|webp|gif);base64,[A-Za-z0-9+/=\s]+$/i

export function clampWallpaperEffect(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_WALLPAPER_EFFECT
  return Math.min(MAX_WALLPAPER_EFFECT, Math.max(MIN_WALLPAPER_EFFECT, Math.round(value)))
}

export function wallpaperBlurPx(percent: number): number {
  return (clampWallpaperEffect(percent) / MAX_WALLPAPER_EFFECT) * 40
}

export function wallpaperPixelFactor(percent: number): number {
  const clamped = clampWallpaperEffect(percent)
  return clamped <= 0 ? 1 : 1 + (clamped / MAX_WALLPAPER_EFFECT) * 19
}

export function isWallpaperDataUrl(value: unknown): value is string {
  if (typeof value !== 'string' || value.length === 0) return false
  if (value.length > MAX_WALLPAPER_DATA_URL_CHARS) return false
  return DATA_URL.test(value)
}

export function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '')
    reader.onerror = () => reject(reader.error ?? new Error('read failed'))
    reader.readAsDataURL(file)
  })
}

export function downscaleWallpaper(dataUrl: string): Promise<string | null> {
  return new Promise((resolve) => {
    if (typeof Image === 'undefined' || typeof document === 'undefined') {
      resolve(null)
      return
    }
    const image = new Image()
    let settled = false
    const finish = (value: string | null): void => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      image.onload = null
      image.onerror = null
      resolve(value)
    }
    const timer = setTimeout(() => { finish(null) }, 200)
    const paint = (): void => {
      const width = image.naturalWidth || image.width
      const height = image.naturalHeight || image.height
      if (!width || !height) { finish(null); return }
      const scale = Math.min(1, MAX_WALLPAPER_EDGE / Math.max(width, height))
      const canvas = document.createElement('canvas')
      canvas.width = Math.max(1, Math.round(width * scale))
      canvas.height = Math.max(1, Math.round(height * scale))
      const context = canvas.getContext('2d')
      if (context === null) { finish(null); return }
      context.drawImage(image, 0, 0, canvas.width, canvas.height)
      try {
        const jpeg = canvas.toDataURL('image/jpeg', 0.82)
        finish(isWallpaperDataUrl(jpeg) ? jpeg : null)
      } catch {
        finish(null)
      }
    }
    image.onload = paint
    image.onerror = () => { finish(null) }
    image.src = dataUrl
    if (image.complete) paint()
  })
}

export async function encodeWallpaperFile(file: File): Promise<string | null> {
  const named = /\\.(png|jpe?g|webp|gif)$/i.test(file.name)
  if (!ALLOWED_TYPES.has(file.type) && !named) return null
  let raw: string
  try {
    raw = await readFileAsDataUrl(file)
  } catch {
    return null
  }
  if (!raw.startsWith('data:image/')) return null
  const resized = await downscaleWallpaper(raw)
  const next = resized ?? (isWallpaperDataUrl(raw) ? raw : null)
  if (next === null || next.length > MAX_WALLPAPER_DATA_URL_CHARS) return null
  return next
}

export function wallpaperCanvasSolidity(solidity: number): number {
  const kept = Math.min(100, Math.max(0, Math.round(solidity)))
  if (kept <= 40) return Math.round(kept * 0.375)
  if (kept <= 80) return Math.round(15 + (kept - 40) * 0.75)
  return Math.round(45 + (kept - 80) * 2.75)
}

/** 主要表面 token 半透明化，让背景透出来（玻璃透明度）。 */
export function mixWallpaperSurfaces(tokens: ThemeTokens, mode: 'light' | 'dark', solidity: number): ThemeTokens {
  const next: ThemeTokens = { ...tokens }
  const kept = Math.min(100, Math.max(0, Math.round(solidity)))
  const canvas = wallpaperCanvasSolidity(kept)
  const sidebar = Math.round((canvas + kept) / 2)
  const base = mode === 'dark' ? 'var(--dsw-static-neutral-bluish-950)' : 'var(--dsw-static-neutral-bluish-00)'
  const raised = mode === 'dark' ? 'var(--dsw-static-neutral-bluish-875)' : 'var(--dsw-static-neutral-bluish-00)'
  const surfaces: Record<string, { fallback: string; percent: number }> = {
    '--dsw-alias-bg-base': { fallback: base, percent: canvas },
    '--dsw-alias-bg-layer-1': { fallback: raised, percent: kept },
    '--dsw-alias-bg-layer-2': { fallback: raised, percent: kept },
    '--dsw-specific-sidebar-fill': { fallback: raised, percent: sidebar },
  }
  for (const [name, { fallback, percent }] of Object.entries(surfaces)) {
    const current = next[name]
    const solid = current !== undefined && !current.includes('color-mix') ? current : fallback
    next[name] = 'color-mix(in srgb, ' + solid + ' ' + percent + '%, transparent)'
  }
  return next
}

// ── 排版（移植自参考 appearance-apply.ts）────────────────────────────────────

export const DEFAULT_SANS_STACK = "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Helvetica Neue', Helvetica, Arial, sans-serif"
export const DEFAULT_CODE_STACK = "'SF Mono', 'JetBrains Mono', 'Fira Code', Consolas, 'Liberation Mono', Menlo, Courier, 'PingFang SC', 'Microsoft YaHei'"

export function quoteFontFamilyName(name: string): string {
  const bare = name.trim()
  if (bare.length === 0) return ''
  if (/^(['"]).*\1$/.test(bare)) return bare
  if (/^[a-zA-Z][a-zA-Z0-9-]*$/.test(bare)) return bare
  return '"' + bare.replaceAll('"', '') + '"'
}

export function cssFontFamilies(input: string): string | null {
  const families = input.split(',').map(quoteFontFamilyName).filter((name) => name.length > 0)
  return families.length > 0 ? families.join(', ') : null
}

export function appearanceFontStack(custom: string, defaultStack: string): string {
  const families = cssFontFamilies(custom)
  return families === null ? defaultStack : families + ', ' + defaultStack
}
```

- [ ] **Step 2: 新增 `shared` 构建入口（tsdown.config.ts）**

在 `packages/plugins/bridge/tsdown.config.ts` 的 `defineConfig([...])` 数组末尾追加：

```ts
  {
    // shared 面：纯函数共享模型，供 host 单测与 client 内联打包
    name: "@dsh-desktop/plugin-bridge/shared",
    entry: { shared: "src/shared/theme.ts" },
    outDir: "lib",
    format: ["esm"],
    platform: "neutral",
    target: "es2024",
    fixedExtension: false,
    dts: false,
    clean: false,
  },
```

- [ ] **Step 3: package.json 暴露 shared 入口与测试脚本**

`packages/plugins/bridge/package.json` 的 `exports` 增加 `"./shared": "./lib/shared.js"`，`scripts` 增加：

```json
"test": "yarn build && node tests/theme.test.mjs"
```

- [ ] **Step 4: 写纯函数单测 `tests/theme.test.mjs`**

```js
import test from "node:test";
import assert from "node:assert/strict";
import {
  resolveMode, resolveThemeSettings, resolveThemeFamily, listThemeFamilies,
  deriveThemeTokens, wallpaperBlurPx, wallpaperPixelFactor, isWallpaperDataUrl,
  mixWallpaperSurfaces, wallpaperCanvasSolidity, duplicateThemeFamily,
  normalizeImportedThemeFamily, ensureUniqueThemeId, slugifyThemeId, serializeThemeFamily,
} from "../lib/shared.js";

test("resolveMode 解析 system 偏好", () => {
  assert.equal(resolveMode("system", true), "dark");
  assert.equal(resolveMode("system", false), "light");
  assert.equal(resolveMode("light", true), "light");
  assert.equal(resolveMode("dark", false), "dark");
});

test("resolveThemeSettings 填充默认值", () => {
  const s = resolveThemeSettings(undefined);
  assert.equal(s.preference, "system");
  assert.equal(s.activeDarkThemeId, "deepseek");
  assert.equal(s.glassOpacity, 80);
  assert.deepEqual(s.customThemes, []);
});

test("内置家族列表含 7 个家族，深色 id 可解析", () => {
  const families = listThemeFamilies([]);
  assert.equal(families.length, 7);
  assert.equal(resolveThemeFamily("midnight", []).name, "午夜");
  assert.equal(resolveThemeFamily("unknown", []).id, "deepseek");
});

test("deriveThemeTokens 产出关键 token 且 DeepSeek 家族不推导", () => {
  const midnight = resolveThemeFamily("midnight", []);
  const tokens = deriveThemeTokens(midnight.dark);
  assert.ok(tokens["--dsw-alias-bg-base"].startsWith("#"));
  assert.ok(tokens["--dsw-alias-brand-primary"].startsWith("#"));
  assert.equal(deriveThemeTokens(resolveThemeFamily("deepseek", []).dark)["--dsw-alias-bg-base"], undefined);
});

test("背景图效果映射与 data URL 校验", () => {
  assert.equal(wallpaperBlurPx(50), 20);
  assert.equal(wallpaperPixelFactor(0), 1);
  assert.equal(wallpaperPixelFactor(100), 20);
  assert.equal(isWallpaperDataUrl("data:image/png;base64,AAAA"), true);
  assert.equal(isWallpaperDataUrl("https://x/y.png"), false);
  assert.equal(isWallpaperDataUrl(""), false);
});

test("玻璃表面混合输出 color-mix", () => {
  const mixed = mixWallpaperSurfaces({ "--dsw-alias-bg-base": "#ffffff" }, "light", 80);
  assert.match(mixed["--dsw-alias-bg-base"], /^color-mix\(in srgb/);
  assert.equal(wallpaperCanvasSolidity(80), 45);
});

test("自定义主题复制与导入生成唯一 id", () => {
  const base = { id: "violet", name: "暮紫", origin: "custom", light: { accent: "#7c3aed", background: "#fff", foreground: "#000", contrast: 46 }, dark: { accent: "#c4a1ff", background: "#000", foreground: "#fff", contrast: 46 } };
  const dup = duplicateThemeFamily(base, new Set(["violet"]));
  assert.equal(dup.id, "violet-copy");
  assert.equal(dup.origin, "custom");
  const imp = normalizeImportedThemeFamily({ ...base, id: "violet" }, new Set(["violet", "violet-copy"]));
  assert.equal(imp.id, "violet-2");
  assert.equal(ensureUniqueThemeId("deepseek", new Set(["deepseek"])), "deepseek-2");
  assert.equal(slugifyThemeId("  我的 主题! "), "我的-主题");
  assert.equal(JSON.parse(serializeThemeFamily(dup)).name, "暮紫 Copy");
});
```

- [ ] **Step 5: 跑测试确认通过**

Run: `yarn workspace @dsh-desktop/plugin-bridge test`（先 tsdown 构建出 `lib/shared.js`，再跑 node:test）
Expected: 全部 test 通过（`# pass` 且无 `# fail`）

- [ ] **Step 6: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 7: 提交**

```bash
git add packages/plugins/bridge/src/shared/theme.ts packages/plugins/bridge/tests/theme.test.mjs packages/plugins/bridge/tsdown.config.ts packages/plugins/bridge/package.json
git commit -m "feat(bridge): 共享主题模型（内置家族/token 推导/背景图/排版工具）"
```

---

### Task 2: client 面主题应用（token / 暗色切换 / 背景图层 / 玻璃 / 排版）

**Files:**
- Create: `packages/plugins/bridge/src/client/theme-apply.ts`

**Interfaces:**
- Consumes: Task 1 共享模块全部符号；`ThemeSettings`/`ThemeFamily`/`deriveThemeTokens`/`resolveMode`/`resolveThemeFamily`/`DEFAULT_FAMILY_ID`/`isWallpaperDataUrl`/`mixWallpaperSurfaces`/`clampWallpaperEffect`/`wallpaperBlurPx`/`wallpaperPixelFactor`/`WALLPAPER_*` 常量/`appearanceFontStack`/`DEFAULT_*`
- Produces: `applyThemeSection(section, systemDark, preview?)`（把分节应用到 dsh web DOM）与 `wallpaperStyleSheet()`（背景图层样式字符串）；Task 7 装配时每份快照变化调用

- [ ] **Step 1: 写 `src/client/theme-apply.ts`**

```ts
/** client 面主题应用：把 ui-theme 分节写到 dsh web DOM（--dsw-alias-* 变量 + 背景图层）。 */
import {
  DEFAULT_FAMILY_ID,
  DEFAULT_SANS_STACK,
  DEFAULT_CODE_STACK,
  WALLPAPER_ATTR,
  WALLPAPER_BLEED,
  WALLPAPER_INNER_ID,
  WALLPAPER_LAYER_ID,
  appearanceFontStack,
  clampWallpaperEffect,
  deriveThemeTokens,
  isWallpaperDataUrl,
  mixWallpaperSurfaces,
  resolveMode,
  resolveThemeFamily,
  wallpaperBlurPx,
  wallpaperPixelFactor,
  type ThemeFamily,
  type ThemeSettings,
  type ThemeTokens,
} from "../shared/theme";

/** 背景图层样式（上游 rc.6 无 wallpaper.css，由本插件注入）。 */
export function wallpaperStyleSheet(): string {
  return [
    "#" + WALLPAPER_LAYER_ID + " {",
    "  position: fixed;",
    "  inset: 0;",
    "  z-index: 0;",
    "  overflow: hidden;",
    "  pointer-events: none;",
    "}",
    "#" + WALLPAPER_INNER_ID + " {",
    "  position: absolute;",
    "  left: -" + WALLPAPER_BLEED + "px;",
    "  top: -" + WALLPAPER_BLEED + "px;",
    "  width: calc(100% + " + WALLPAPER_BLEED * 2 + "px);",
    "  height: calc(100% + " + WALLPAPER_BLEED * 2 + "px);",
    "  background-position: center;",
    "  background-repeat: no-repeat;",
    "  background-size: cover;",
    "  image-rendering: pixelated;",
    "  filter: blur(var(--dsh-wallpaper-blur, 0px));",
    "}",
    "html[" + WALLPAPER_ATTR + "],",
    "html[" + WALLPAPER_ATTR + "] body,",
    "html[" + WALLPAPER_ATTR + "] #root {",
    "  background: transparent;",
    "}",
    "html[" + WALLPAPER_ATTR + "] #root {",
    "  position: relative;",
    "  z-index: 1;",
    "}",
  ].join("\n")
}

let styleInjected = false

function ensureWallpaperStyle(): void {
  if (styleInjected) return
  const style = document.createElement('style')
  style.id = 'dsh-wallpaper-style'
  style.textContent = wallpaperStyleSheet()
  document.documentElement.appendChild(style)
  styleInjected = true
}

/** 解析当前生效的家族（preview 优先于持久化选择）。 */
function activeFamily(section: ThemeSettings, mode: 'light' | 'dark', preview: ThemeFamily | null): ThemeFamily {
  if (preview !== null) return preview
  const familyId = mode === 'dark' ? section.activeDarkThemeId : section.activeLightThemeId
  return resolveThemeFamily(familyId, section.customThemes)
}

// 背景图层幂等状态（参考 wallpaper.ts 的 applied 缓存）
let applied: { image: string; blurPx: number; factor: number } | null = null
let decodedFor = ''
let decoded: HTMLImageElement | null = null
let resizeBound = false

function layerCssSize(): { width: number; height: number } {
  const width = typeof window === 'undefined' ? 0 : window.innerWidth
  const height = typeof window === 'undefined' ? 0 : window.innerHeight
  return {
    width: Math.max(1, width + WALLPAPER_BLEED * 2),
    height: Math.max(1, height + WALLPAPER_BLEED * 2),
  }
}

function drawWallpaperBitmap(canvas: HTMLCanvasElement, image: HTMLImageElement, factor: number): void {
  const context = canvas.getContext('2d')
  if (context === null) return
  const { width, height } = layerCssSize()
  const bitmapWidth = Math.max(1, Math.round(width / factor))
  const bitmapHeight = Math.max(1, Math.round(height / factor))
  const sourceWidth = image.naturalWidth || image.width
  const sourceHeight = image.naturalHeight || image.height
  if (!sourceWidth || !sourceHeight) return
  canvas.width = bitmapWidth
  canvas.height = bitmapHeight
  const scale = Math.max(bitmapWidth / sourceWidth, bitmapHeight / sourceHeight)
  const drawWidth = sourceWidth * scale
  const drawHeight = sourceHeight * scale
  context.drawImage(image, (bitmapWidth - drawWidth) / 2, (bitmapHeight - drawHeight) / 2, drawWidth, drawHeight)
}

function redrawApplied(): void {
  if (applied === null || typeof document === 'undefined') return
  const canvas = document.getElementById(WALLPAPER_INNER_ID)
  if (canvas === null) return
  redrawWallpaper(canvas as HTMLCanvasElement, applied.image, applied.factor)
}

function redrawWallpaper(canvas: HTMLCanvasElement, image: string, factor: number): void {
  if (decodedFor === image && decoded !== null) {
    if (decoded.complete) drawWallpaperBitmap(canvas, decoded, factor)
    return
  }
  if (typeof Image === 'undefined') return
  const next = new Image()
  decodedFor = image
  decoded = next
  next.onload = () => {
    if (decoded !== next || applied === null || applied.image !== image) return
    redrawApplied()
  }
  next.src = image
  if (next.complete) drawWallpaperBitmap(canvas, next, factor)
}

/** 绘制或移除固定背景图层（幂等，仅字段变化才碰 DOM）。 */
export function applyWallpaperLayer(extras: {
  wallpaperImage: string
  wallpaperBlur: number
  wallpaperPixelate: number
}): void {
  if (typeof document === 'undefined') return
  const image = isWallpaperDataUrl(extras.wallpaperImage) ? extras.wallpaperImage : ''
  const root = document.documentElement
  if (image.length === 0) {
    applied = null
    decoded = null
    decodedFor = ''
    root.removeAttribute(WALLPAPER_ATTR)
    document.getElementById(WALLPAPER_LAYER_ID)?.remove()
    root.style.removeProperty('--dsh-wallpaper-blur')
    if (resizeBound && typeof window !== 'undefined') {
      window.removeEventListener('resize', redrawApplied)
      resizeBound = false
    }
    return
  }
  const blurPx = wallpaperBlurPx(extras.wallpaperBlur)
  const factor = wallpaperPixelFactor(extras.wallpaperPixelate)
  root.setAttribute(WALLPAPER_ATTR, '')
  ensureWallpaperStyle()
  let layer = document.getElementById(WALLPAPER_LAYER_ID)
  let canvas: HTMLCanvasElement
  if (layer === null) {
    layer = document.createElement('div')
    layer.id = WALLPAPER_LAYER_ID
    layer.setAttribute('aria-hidden', 'true')
    canvas = document.createElement('canvas')
    canvas.id = WALLPAPER_INNER_ID
    layer.appendChild(canvas)
    document.body.insertBefore(layer, document.body.firstChild)
    applied = null
  } else {
    canvas = layer.firstElementChild as HTMLCanvasElement
  }
  if (!resizeBound && typeof window !== 'undefined') {
    window.addEventListener('resize', redrawApplied)
    resizeBound = true
  }
  if (applied === null || applied.blurPx !== blurPx) {
    root.style.setProperty('--dsh-wallpaper-blur', blurPx + 'px')
  }
  const imageChanged = applied === null || applied.image !== image
  const factorChanged = applied === null || applied.factor !== factor
  if (imageChanged) canvas.style.backgroundImage = 'url("' + image + '")'
  applied = { image, blurPx, factor }
  if (imageChanged || factorChanged) redrawWallpaper(canvas, image, factor)
}

/** 把 ui-theme 分节整体应用到 dsh web DOM。 */
export function applyThemeSection(section: ThemeSettings, systemDark: boolean, preview: ThemeFamily | null = null): void {
  if (typeof document === 'undefined') return
  const mode = resolveMode(section.preference, systemDark)
  const root = document.documentElement
  const body = document.body
  root.style.colorScheme = mode
  body.toggleAttribute('data-ds-dark-theme', mode === 'dark')

  const family = activeFamily(section, mode, preview)
  let tokens: ThemeTokens = {}
  if (family.id !== DEFAULT_FAMILY_ID) {
    tokens = deriveThemeTokens(family[mode])
  }
  if (isWallpaperDataUrl(section.wallpaperImage)) {
    tokens = mixWallpaperSurfaces(tokens, mode, section.glassOpacity)
  }
  tokens['--dsw-alias-glass-opacity'] = clampWallpaperEffect(section.glassOpacity) + '%'
  for (const [name, value] of Object.entries(tokens)) {
    body.style.setProperty(name, value)
  }

  // 排版
  root.style.fontSize = (section.fontSizeInterface || 16) + 'px'
  const sans = appearanceFontStack(section.fontFamilySans, DEFAULT_SANS_STACK)
  const code = appearanceFontStack(section.fontFamilyCode, DEFAULT_CODE_STACK)
  root.style.setProperty('--dsw-font-family', sans)
  root.style.setProperty('--ds-font-family-code', code)
  root.style.setProperty('--dsw-font-size-code', (section.fontSizeCode || 13) + 'px')
  root.style.setProperty('--dsw-font-family-composer', appearanceFontStack(section.fontFamilyComposer || '', sans))
  root.style.setProperty('--dsw-font-family-terminal', appearanceFontStack(section.fontFamilyTerminal || '', code))

  applyWallpaperLayer({
    wallpaperImage: section.wallpaperImage,
    wallpaperBlur: section.wallpaperBlur,
    wallpaperPixelate: section.wallpaperPixelate,
  })
}
```

- [ ] **Step 2: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add packages/plugins/bridge/src/client/theme-apply.ts
git commit -m "feat(bridge): client 面主题应用（token/暗色切换/背景图层/玻璃/排版）"
```

---

### Task 3: client 面设置快照存储（bind ui-theme scope + 防抖持久化）

**Files:**
- Create: `packages/plugins/bridge/src/client/theme-store.ts`

**Interfaces:**
- Consumes: Task 1 的 `ThemeSettings`/`ThemeFamily`/`THEME_*` 字段名常量/`clampWallpaperEffect`/`isWallpaperDataUrl`/玻璃/排版边界常量
- Produces: `createThemeStore(scope)` → `ThemeStore`（`getSnapshot`/`getPreview`/`subscribe`/`setPreference`/`setThemeHalf`/`setCustomThemes`/`setGlassOpacity`/`setWallpaper`/`setTypography`/`previewFamily`），本地快照立即发布、Host 写防抖 300ms（参考 ThemeRuntime.queueWrite/flushWrites）；Task 4–7 消费

- [ ] **Step 1: 写 `src/client/theme-store.ts`**

```ts
/** ui-theme 设置快照存储：bind 上游命名空间，本地快照 + 防抖写回 Host。 */
import {
  DEFAULT_THEME_SETTINGS,
  MAX_CODE_FONT_SIZE,
  MAX_GLASS_OPACITY,
  MAX_INTERFACE_FONT_SIZE,
  MAX_WALLPAPER_EFFECT,
  MIN_CODE_FONT_SIZE,
  MIN_GLASS_OPACITY,
  MIN_INTERFACE_FONT_SIZE,
  MIN_WALLPAPER_EFFECT,
  THEME_CUSTOM_THEMES_FIELD,
  THEME_DARK_FAMILY_FIELD,
  THEME_GLASS_OPACITY_FIELD,
  THEME_LIGHT_FAMILY_FIELD,
  THEME_PREFERENCE_FIELD,
  THEME_WALLPAPER_BLUR_FIELD,
  THEME_WALLPAPER_IMAGE_FIELD,
  THEME_WALLPAPER_PIXELATE_FIELD,
  clampWallpaperEffect,
  isWallpaperDataUrl,
  resolveThemeSettings,
  type ThemeFamily,
  type ThemePreference,
  type ThemeSettings,
} from "../shared/theme";

/** settingsScope.bind 的最小形状（与 client.tsx 的 SettingsScopeLike 对齐）。 */
export interface ThemeScopeLike<T> {
  getSnapshot(): { status: 'loading' | 'ready' | 'unavailable'; value: T | undefined }
  subscribe(listener: () => void): () => void
  set(field: string, value: unknown): Promise<void>
}

const clampInt = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, Math.round(value)))

/** 主题设置存储：快照镜像 + 防抖 Host 写。 */
export interface ThemeStore {
  getSnapshot(): ThemeSettings
  getPreview(): ThemeFamily | null
  subscribe(listener: () => void): () => void
  setPreference(preference: ThemePreference): void
  setThemeHalf(mode: 'light' | 'dark', familyId: string): void
  setCustomThemes(customThemes: ThemeFamily[]): void
  setGlassOpacity(value: number): void
  setWallpaper(patch: Partial<Pick<ThemeSettings, 'wallpaperImage' | 'wallpaperBlur' | 'wallpaperPixelate'>>): void
  setTypography(patch: Partial<Pick<ThemeSettings, 'fontFamilySans' | 'fontFamilyCode' | 'fontSizeInterface' | 'fontSizeCode' | 'fontFamilyComposer' | 'fontFamilyTerminal'>>): void
  previewFamily(family: ThemeFamily | null): void
}

export function createThemeStore(scope: ThemeScopeLike<ThemeSettings>): ThemeStore {
  let settings: ThemeSettings = { ...DEFAULT_THEME_SETTINGS, customThemes: [] }
  let preview: ThemeFamily | null = null
  const listeners = new Set<() => void>()

  const pendingWrites = new Map<string, unknown>()
  let writeTimer: ReturnType<typeof setTimeout> | undefined
  let inFlightWrites = 0

  const publish = (): void => {
    for (const listener of [...listeners]) {
      try {
        listener()
      } catch {
        // 监听器异常不阻断后续
      }
    }
  }

  const adopt = (): void => {
    if (pendingWrites.size > 0 || inFlightWrites > 0) return
    const snapshot = scope.getSnapshot()
    if (snapshot.status !== 'ready' || snapshot.value === undefined) return
    const next = resolveThemeSettings(snapshot.value)
    if (JSON.stringify(next) === JSON.stringify(settings)) return
    settings = next
    publish()
  }

  const queueWrite = (field: string, value: unknown): void => {
    pendingWrites.set(field, value)
    if (writeTimer !== undefined) clearTimeout(writeTimer)
    writeTimer = setTimeout(() => { void flushWrites() }, 300)
  }

  const flushWrites = async (): Promise<void> => {
    if (writeTimer !== undefined) {
      clearTimeout(writeTimer)
      writeTimer = undefined
    }
    if (pendingWrites.size === 0) return
    const writes = [...pendingWrites]
    pendingWrites.clear()
    for (const [field, value] of writes) {
      inFlightWrites += 1
      await scope.set(field, value).catch(() => {})
      inFlightWrites -= 1
    }
    if (inFlightWrites === 0 && pendingWrites.size === 0) adopt()
  }

  const unsubscribe = scope.subscribe(adopt)
  adopt()

  const store: ThemeStore = {
    getSnapshot: () => settings,
    getPreview: () => preview,
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    setPreference: (preference) => {
      if (settings.preference === preference) return
      settings = { ...settings, preference }
      queueWrite(THEME_PREFERENCE_FIELD, preference)
      publish()
    },
    setThemeHalf: (mode, familyId) => {
      const field = mode === 'dark' ? THEME_DARK_FAMILY_FIELD : THEME_LIGHT_FAMILY_FIELD
      if (settings[field] === familyId) return
      settings = { ...settings, [field]: familyId }
      queueWrite(field, familyId)
      publish()
    },
    setCustomThemes: (customThemes) => {
      const next = customThemes.map((item) => ({ ...item, origin: 'custom' as const }))
      settings = { ...settings, customThemes: next }
      queueWrite(THEME_CUSTOM_THEMES_FIELD, next)
      publish()
    },
    setGlassOpacity: (value) => {
      const next = clampInt(value, MIN_GLASS_OPACITY, MAX_GLASS_OPACITY)
      if (settings.glassOpacity === next) return
      settings = { ...settings, glassOpacity: next }
      queueWrite(THEME_GLASS_OPACITY_FIELD, next)
      publish()
    },
    setWallpaper: (patch) => {
      const next: Pick<ThemeSettings, 'wallpaperImage' | 'wallpaperBlur' | 'wallpaperPixelate'> = {
        wallpaperImage: settings.wallpaperImage,
        wallpaperBlur: settings.wallpaperBlur,
        wallpaperPixelate: settings.wallpaperPixelate,
      }
      if (patch.wallpaperImage !== undefined) {
        next.wallpaperImage = patch.wallpaperImage === '' || isWallpaperDataUrl(patch.wallpaperImage)
          ? patch.wallpaperImage
          : ''
      }
      if (patch.wallpaperBlur !== undefined) next.wallpaperBlur = clampWallpaperEffect(patch.wallpaperBlur)
      if (patch.wallpaperPixelate !== undefined) next.wallpaperPixelate = clampWallpaperEffect(patch.wallpaperPixelate)
      if (next.wallpaperImage === settings.wallpaperImage
        && next.wallpaperBlur === settings.wallpaperBlur
        && next.wallpaperPixelate === settings.wallpaperPixelate) {
        return
      }
      settings = { ...settings, ...next }
      if (patch.wallpaperImage !== undefined) queueWrite(THEME_WALLPAPER_IMAGE_FIELD, next.wallpaperImage)
      if (patch.wallpaperBlur !== undefined) queueWrite(THEME_WALLPAPER_BLUR_FIELD, next.wallpaperBlur)
      if (patch.wallpaperPixelate !== undefined) queueWrite(THEME_WALLPAPER_PIXELATE_FIELD, next.wallpaperPixelate)
      publish()
    },
    setTypography: (patch) => {
      const next = { ...patch }
      if (next.fontSizeInterface !== undefined) next.fontSizeInterface = clampInt(next.fontSizeInterface, MIN_INTERFACE_FONT_SIZE, MAX_INTERFACE_FONT_SIZE)
      if (next.fontSizeCode !== undefined) next.fontSizeCode = clampInt(next.fontSizeCode, MIN_CODE_FONT_SIZE, MAX_CODE_FONT_SIZE)
      settings = { ...settings, ...next }
      for (const [field, value] of Object.entries(next)) queueWrite(field, value)
      publish()
    },
    previewFamily: (family) => {
      if (preview === family) return
      preview = family
      publish()
    },
  }

  return store
}
```

> 说明：`scope.subscribe(adopt)` 在插件 fiber 销毁时随 scope 订阅机制清理；防抖写与快照镜像属 client bundle 运行时行为，验证方式为 Task 7 的集成验证（打开设置 → 外观，拖动滑块后查看 `settings.yaml` 的 `ui-theme` 分节是否更新）。

- [ ] **Step 2: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add packages/plugins/bridge/src/client/theme-store.ts
git commit -m "feat(bridge): client 面主题设置快照存储（防抖持久化）"
```

---

### Task 4: 外观设置节——主题偏好与主题库

**Files:**
- Create: `packages/plugins/bridge/src/client/AppearanceSection.tsx`
- Create: `packages/plugins/bridge/src/client/ThemeLibrary.tsx`
- Create: `packages/plugins/bridge/src/client/appearance.module.css`

**Interfaces:**
- Consumes: Task 3 的 `ThemeStore`；Task 1 的 `listThemeFamilies`/`isBuiltinFamilyId`/`THEME_PREFERENCES`/`ThemePreference`
- Produces: `AppearanceSection({ store, t })`（外观设置节：本任务先含 主题偏好 + 主题库 两个子块）与 `ThemeLibrary({ store, t })`（主题卡网格）；Task 5/6 向本组件追加背景图/玻璃/自定义主题/排版子块，Task 7 装配

- [ ] **Step 1: 写 `appearance.module.css`（主题库布局）**

```css
.library {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  margin-top: 12px;
}
.card {
  border: 1px solid var(--dsw-alias-border-l1, #e3e6eb);
  border-radius: 10px;
  overflow: hidden;
  background: var(--dsw-alias-bg-layer-1, #fff);
  cursor: pointer;
}
.halves {
  display: grid;
  grid-template-columns: 1fr 1fr;
}
.half {
  height: 64px;
  display: flex;
  align-items: flex-end;
  padding: 6px 8px;
  font-size: 11px;
  font-weight: 600;
  border: 0;
}
.half[data-active="true"] {
  outline: 2px solid var(--dsw-alias-brand-primary, #3f63f4);
  outline-offset: -2px;
}
.caption {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  font-size: 13px;
}
.badge {
  font-size: 11px;
  opacity: 0.6;
}
.prefRow {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}
.cube {
  flex: 1;
  border: 1px solid var(--dsw-alias-border-l1, #e3e6eb);
  border-radius: 8px;
  padding: 8px 0;
  font-size: 13px;
  background: var(--dsw-alias-bg-layer-1, #fff);
  cursor: pointer;
}
.cube[data-active="true"] {
  border-color: var(--dsw-alias-brand-primary, #3f63f4);
  color: var(--dsw-alias-brand-primary, #3f63f4);
  font-weight: 600;
}
.sectionTitle {
  font-size: 15px;
  font-weight: 600;
  margin: 0 0 6px;
}
.hint {
  font-size: 12px;
  opacity: 0.6;
  margin: 0 0 10px;
}
```

- [ ] **Step 2: 写 `ThemeLibrary.tsx`**

```tsx
/** 主题库：每个家族一张卡（浅/深两半，点哪半用哪半）。 */
import { useSyncExternalStore } from "react";
import {
  isBuiltinFamilyId,
  listThemeFamilies,
  type ThemeFamily,
} from "../shared/theme";
import type { ThemeStore } from "./theme-store";
import css from "./appearance.module.css";

export function familySwatch(family: ThemeFamily, mode: 'light' | 'dark'): { background: string; foreground: string; accent: string } {
  const seeds = family[mode]
  return {
    background: seeds.background,
    foreground: seeds.foreground,
    accent: seeds.accent,
  }
}

export function ThemeLibrary({ store, t }: { store: ThemeStore; t: (key: string) => string }): JSX.Element {
  const settings = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
  )
  const families = listThemeFamilies(settings.customThemes)
  const activeLight = settings.activeLightThemeId
  const activeDark = settings.activeDarkThemeId

  return (
    <section aria-label={t("library.title")}>
      <h3 className={css.sectionTitle}>{t("library.title")}</h3>
      <p className={css.hint}>{t("library.desc")}</p>
      <div className={css.library}>
        {families.map((family) => {
          const light = familySwatch(family, "light")
          const dark = familySwatch(family, "dark")
          return (
            <div key={family.id} className={css.card}>
              <div className={css.halves}>
                <button
                  type="button"
                  className={css.half}
                  data-active={activeLight === family.id}
                  style={{ background: light.background, color: light.foreground, borderRight: "1px solid rgba(0,0,0,0.08)" }}
                  aria-label={family.name + " 浅色"}
                  title={t("library.lightHalf")}
                  onClick={() => store.setThemeHalf("light", family.id)}
                >
                  <span style={{ color: light.accent, fontWeight: 700 }}>Aa</span>
                </button>
                <button
                  type="button"
                  className={css.half}
                  data-active={activeDark === family.id}
                  style={{ background: dark.background, color: dark.foreground }}
                  aria-label={family.name + " 深色"}
                  title={t("library.darkHalf")}
                  onClick={() => store.setThemeHalf("dark", family.id)}
                >
                  <span style={{ color: dark.accent, fontWeight: 700 }}>Aa</span>
                </button>
              </div>
              <div className={css.caption}>
                <span>{family.name}</span>
                <span className={css.badge}>{isBuiltinFamilyId(family.id) ? t("library.builtin") : t("library.custom")}</span>
              </div>
            </div>
          )
        })}
      </div>
    </section>
  )
}
```

- [ ] **Step 3: 写 `AppearanceSection.tsx`（本任务含主题偏好 + 主题库）**

```tsx
/** 外观设置节：主题偏好 + 主题库（背景图/玻璃/自定义主题/排版由 Task 5/6 追加）。 */
import { useSyncExternalStore } from "react";
import {
  DEFAULT_PREFERENCE,
  THEME_PREFERENCES,
  type ThemePreference,
} from "../shared/theme";
import type { ThemeStore } from "./theme-store";
import { ThemeLibrary } from "./ThemeLibrary";
import css from "./appearance.module.css";

const PREFERENCE_LABELS: Record<ThemePreference, string> = {
  light: "pref.light",
  dark: "pref.dark",
  system: "pref.system",
}

export function AppearanceSection({ store, t }: { store: ThemeStore; t: (key: string) => string }): JSX.Element {
  const settings = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
  )
  const setPreference = (preference: ThemePreference): void => {
    if (preference === DEFAULT_PREFERENCE || THEME_PREFERENCES.includes(preference)) {
      store.setPreference(preference)
    }
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 28, padding: "8px 0", maxWidth: 640 }}>
      <section aria-label={t("pref.title")}>
        <h3 className={css.sectionTitle}>{t("pref.title")}</h3>
        <div className={css.prefRow} role="radiogroup" aria-label={t("pref.title")}>
          {THEME_PREFERENCES.map((preference) => (
            <button
              key={preference}
              type="button"
              className={css.cube}
              data-active={settings.preference === preference}
              role="radio"
              aria-checked={settings.preference === preference}
              onClick={() => setPreference(preference)}
            >
              {t(PREFERENCE_LABELS[preference])}
            </button>
          ))}
        </div>
      </section>
      <ThemeLibrary store={store} t={t} />
    </div>
  )
}
```

- [ ] **Step 4: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add packages/plugins/bridge/src/client/AppearanceSection.tsx packages/plugins/bridge/src/client/ThemeLibrary.tsx packages/plugins/bridge/src/client/appearance.module.css
git commit -m "feat(bridge): 外观设置节（主题偏好 + 主题库）"
```

---

### Task 5: 外观设置节——背景图与玻璃

**Files:**
- Create: `packages/plugins/bridge/src/client/WallpaperRow.tsx`
- Create: `packages/plugins/bridge/src/client/GlassSlider.tsx`
- Modify: `packages/plugins/bridge/src/client/AppearanceSection.tsx`（追加两个子块）

**Interfaces:**
- Consumes: Task 1 `encodeWallpaperFile`/`MAX_WALLPAPER_EFFECT`/`MIN_WALLPAPER_EFFECT`/`DEFAULT_WALLPAPER_EFFECT`/`GLASS_OPACITY_STEP`/`MAX_GLASS_OPACITY`/`MIN_GLASS_OPACITY`；Task 3 `ThemeStore.setWallpaper/setGlassOpacity`
- Produces: `WallpaperRow`（选图 + 毛玻璃/像素化滑块 + 清除/重置）与 `GlassSlider`（玻璃透明度滑块）；Task 6 后由 `AppearanceSection` 统一渲染

- [ ] **Step 1: 写 `WallpaperRow.tsx`**

```tsx
/** 背景图：选图 → data URL → 毛玻璃/像素化滑块。 */
import { useRef } from "react";
import {
  DEFAULT_WALLPAPER_EFFECT,
  MAX_WALLPAPER_EFFECT,
  MIN_WALLPAPER_EFFECT,
  WALLPAPER_EFFECT_STEP,
  encodeWallpaperFile,
} from "../shared/theme";
import css from "./appearance.module.css";

export function WallpaperRow({
  wallpaperImage,
  wallpaperBlur,
  wallpaperPixelate,
  t,
  setWallpaper,
}: {
  wallpaperImage: string
  wallpaperBlur: number
  wallpaperPixelate: number
  t: (key: string) => string
  setWallpaper: (patch: { wallpaperImage?: string; wallpaperBlur?: number; wallpaperPixelate?: number }) => void
}): JSX.Element {
  const fileRef = useRef<HTMLInputElement>(null)
  const hasImage = wallpaperImage.length > 0

  const pick = async (file: File | undefined): Promise<void> => {
    if (file === undefined) return
    const encoded = await encodeWallpaperFile(file)
    if (encoded === null) return
    setWallpaper({ wallpaperImage: encoded })
  }

  return (
    <section aria-label={t("wallpaper.title")}>
      <h3 className={css.sectionTitle}>{t("wallpaper.title")}</h3>
      <p className={css.hint}>{t("wallpaper.desc")}</p>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button type="button" onClick={() => fileRef.current?.click()}>{t("wallpaper.choose")}</button>
        {hasImage ? (
          <button type="button" onClick={() => setWallpaper({ wallpaperImage: "" })}>{t("wallpaper.clear")}</button>
        ) : null}
        <input
          ref={fileRef}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          hidden
          onChange={(event) => {
            const file = event.currentTarget.files?.[0]
            event.currentTarget.value = ""
            void pick(file)
          }}
        />
      </div>
      {hasImage ? (
        <>
          <div
            style={{
              marginTop: 12,
              height: 120,
              borderRadius: 8,
              backgroundImage: 'url("' + wallpaperImage + '")',
              backgroundSize: "cover",
              backgroundPosition: "center",
            }}
            role="img"
            aria-label={t("wallpaper.title")}
          />
          {renderSlider("wallpaper.blur", wallpaperBlur, (value) => setWallpaper({ wallpaperBlur: value }))}
          {renderSlider("wallpaper.pixelate", wallpaperPixelate, (value) => setWallpaper({ wallpaperPixelate: value }))}
          <button
            type="button"
            onClick={() =>
              setWallpaper({
                wallpaperBlur: DEFAULT_WALLPAPER_EFFECT,
                wallpaperPixelate: DEFAULT_WALLPAPER_EFFECT,
              })
            }
          >
            {t("wallpaper.reset")}
          </button>
        </>
      ) : null}
    </section>
  )

  function renderSlider(labelKey: string, value: number, onChange: (value: number) => void): JSX.Element {
    return (
      <label style={{ display: "block", marginTop: 10 }}>
        <span style={{ fontSize: 13 }}>{t(labelKey)}：{value}%</span>
        <input
          type="range"
          min={MIN_WALLPAPER_EFFECT}
          max={MAX_WALLPAPER_EFFECT}
          step={WALLPAPER_EFFECT_STEP}
          value={value}
          style={{ width: "100%" }}
          aria-label={t(labelKey)}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    )
  }
}
```

- [ ] **Step 2: 写 `GlassSlider.tsx`**

```tsx
/** 玻璃透明度：40–100，越低表面越通透。 */
import {
  GLASS_OPACITY_STEP,
  MAX_GLASS_OPACITY,
  MIN_GLASS_OPACITY,
} from "../shared/theme";

export function GlassSlider({
  value,
  t,
  onChange,
}: {
  value: number
  t: (key: string) => string
  onChange: (value: number) => void
}): JSX.Element {
  return (
    <section aria-label={t("glass.title")}>
      <h3 style={{ fontSize: 15, fontWeight: 600, margin: "0 0 6px" }}>{t("glass.title")}</h3>
      <p style={{ fontSize: 12, opacity: 0.6, margin: "0 0 10px" }}>{t("glass.desc")}</p>
      <label style={{ display: "block" }}>
        <span style={{ fontSize: 13 }}>{t("glass.opacity")}：{value}%</span>
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
  )
}
```

- [ ] **Step 3: 在 `AppearanceSection.tsx` 追加两个子块**

新增 import：

```tsx
import { WallpaperRow } from "./WallpaperRow";
import { GlassSlider } from "./GlassSlider";
```

在 `<ThemeLibrary .../>` 之后追加：

```tsx
      <WallpaperRow
        wallpaperImage={settings.wallpaperImage}
        wallpaperBlur={settings.wallpaperBlur}
        wallpaperPixelate={settings.wallpaperPixelate}
        t={t}
        setWallpaper={(patch) => store.setWallpaper(patch)}
      />
      <GlassSlider value={settings.glassOpacity} t={t} onChange={(value) => store.setGlassOpacity(value)} />
```

- [ ] **Step 4: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add packages/plugins/bridge/src/client/WallpaperRow.tsx packages/plugins/bridge/src/client/GlassSlider.tsx packages/plugins/bridge/src/client/AppearanceSection.tsx
git commit -m "feat(bridge): 外观设置节（背景图 + 玻璃透明度）"
```

---

### Task 6: 外观设置节——自定义主题与排版

**Files:**
- Create: `packages/plugins/bridge/src/client/CustomThemeEditor.tsx`
- Create: `packages/plugins/bridge/src/client/TypographySection.tsx`
- Modify: `packages/plugins/bridge/src/client/AppearanceSection.tsx`（追加两个子块）

**Interfaces:**
- Consumes: Task 1 `duplicateThemeFamily`/`normalizeImportedThemeFamily`/`replaceCustomTheme`/`serializeThemeFamily`/`parseThemeFamilyJson`/`getReservedThemeIds`/`slugifyThemeId`/`ensureUniqueThemeId`/`MAX/MIN_*FONT_SIZE`/`DEFAULT_*FONT_SIZE`；Task 3 `ThemeStore.setCustomThemes/previewFamily/setTypography`
- Produces: `CustomThemeEditor`（创建/复制/编辑/导入导出）与 `TypographySection`（字号/字体族）

- [ ] **Step 1: 写 `CustomThemeEditor.tsx`**

```tsx
/** 自定义主题：创建/复制/编辑/导入导出（JSON），编辑时实时预览。 */
import { useState } from "react";
import {
  duplicateThemeFamily,
  ensureUniqueThemeId,
  getReservedThemeIds,
  normalizeImportedThemeFamily,
  parseThemeFamilyJson,
  replaceCustomTheme,
  serializeThemeFamily,
  slugifyThemeId,
  type ThemeFamily,
  type ThemeSeeds,
} from "../shared/theme";
import type { ThemeStore } from "./theme-store";
import css from "./appearance.module.css";

function blankSeeds(): ThemeSeeds {
  return { accent: "#4176e6", background: "#ffffff", foreground: "#0f1115", contrast: 46 }
}

function blankFamily(name: string, id: string): ThemeFamily {
  return { id, name, origin: "custom", light: blankSeeds(), dark: blankSeeds() }
}

const SEED_FIELDS: Array<keyof ThemeSeeds> = ["accent", "background", "foreground"]

export function CustomThemeEditor({
  store,
  t,
}: {
  store: ThemeStore
  t: (key: string) => string
}): JSX.Element {
  const settings = store.getSnapshot()
  const [draft, setDraft] = useState<ThemeFamily | null>(null)

  const existingIds = new Set([...getReservedThemeIds(), ...settings.customThemes.map((item) => item.id)])

  const save = (family: ThemeFamily): void => {
    store.setCustomThemes(replaceCustomTheme(settings.customThemes, family))
  }

  const create = (): void => {
    const id = ensureUniqueThemeId(slugifyThemeId(t("custom.newName")), existingIds)
    setDraft(blankFamily(t("custom.newName"), id))
  }

  const exportFamily = (family: ThemeFamily): void => {
    const blob = new Blob([serializeThemeFamily(family)], { type: "application/json" })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = family.id + ".json"
    anchor.click()
    URL.revokeObjectURL(url)
  }

  const importFamily = (file: File | undefined): void => {
    if (file === undefined) return
    void file.text().then((raw) => {
      try {
        const family = normalizeImportedThemeFamily(parseThemeFamilyJson(raw), existingIds)
        save(family)
      } catch {
        // 非法 JSON 静默失败
      }
    })
  }

  return (
    <section aria-label={t("custom.title")}>
      <h3 className={css.sectionTitle}>{t("custom.title")}</h3>
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 10 }}>
        <button type="button" onClick={create}>{t("custom.create")}</button>
        <label style={{ cursor: "pointer", fontSize: 13 }}>
          {t("custom.import")}
          <input
            type="file"
            accept="application/json,.json"
            hidden
            onChange={(event) => importFamily(event.currentTarget.files?.[0])}
          />
        </label>
      </div>
      {settings.customThemes.map((family) => (
        <div key={family.id} style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 0", borderTop: "1px solid var(--dsw-alias-border-l1, #e3e6eb)" }}>
          <span style={{ flex: 1, fontSize: 13 }}>{family.name}</span>
          <button type="button" onClick={() => setDraft(family)}>{t("custom.edit")}</button>
          <button type="button" onClick={() => { const dup = duplicateThemeFamily(family, existingIds); save(dup); }}>{t("custom.copy")}</button>
          <button type="button" onClick={() => exportFamily(family)}>{t("custom.export")}</button>
          <button type="button" onClick={() => store.setCustomThemes(settings.customThemes.filter((item) => item.id !== family.id))}>{t("custom.remove")}</button>
        </div>
      ))}
      {draft !== null ? (
        <div style={{ border: "1px solid var(--dsw-alias-border-l1, #e3e6eb)", borderRadius: 8, padding: 12, marginTop: 10 }}>
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
            {t("custom.name")}
            <input
              value={draft.name}
              style={{ flex: 1, padding: "6px 8px" }}
              onInput={(e) => setDraft({ ...draft, name: e.currentTarget.value, id: slugifyThemeId(e.currentTarget.value) })}
            />
          </label>
          {(["light", "dark"] as const).map((mode) => (
            <div key={mode}>
              <div style={{ fontSize: 13, fontWeight: 600, margin: "8px 0 4px" }}>{mode === "light" ? t("custom.lightHalf") : t("custom.darkHalf")}</div>
              {SEED_FIELDS.map((field) => (
                <label key={field} style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13, margin: "4px 0" }}>
                  <span style={{ width: 80 }}>{t("custom." + field)}</span>
                  <input
                    type="color"
                    value={draft[mode][field]}
                    style={{ width: 44, height: 26, padding: 0, border: "none" }}
                    onChange={(e) => {
                      const next = { ...draft, [mode]: { ...draft[mode], [field]: e.currentTarget.value } }
                      setDraft(next)
                      store.previewFamily(next)
                    }}
                  />
                  <input
                    value={draft[mode][field]}
                    style={{ flex: 1, padding: "4px 6px", fontSize: 12 }}
                    onInput={(e) => {
                      const next = { ...draft, [mode]: { ...draft[mode], [field]: e.currentTarget.value } }
                      setDraft(next)
                      store.previewFamily(next)
                    }}
                  />
                </label>
              ))}
              <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13, margin: "4px 0" }}>
                <span style={{ width: 80 }}>{t("custom.contrast")}</span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={draft[mode].contrast}
                  style={{ flex: 1 }}
                  onChange={(e) => {
                    const next = { ...draft, [mode]: { ...draft[mode], contrast: Number(e.currentTarget.value) } }
                    setDraft(next)
                    store.previewFamily(next)
                  }}
                />
                <span style={{ fontSize: 12, width: 32, textAlign: "right" }}>{draft[mode].contrast}</span>
              </label>
            </div>
          ))}
          <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
            <button type="button" onClick={() => { save(draft); setDraft(null); store.previewFamily(null); }}>{t("custom.save")}</button>
            <button type="button" onClick={() => { setDraft(null); store.previewFamily(null); }}>{t("custom.cancel")}</button>
          </div>
        </div>
      ) : null}
    </section>
  )
}
```

- [ ] **Step 2: 写 `TypographySection.tsx`**

```tsx
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
  settings: ThemeSettings
  t: (key: string) => string
  onChange: (patch: Partial<Pick<ThemeSettings, "fontFamilySans" | "fontFamilyCode" | "fontSizeInterface" | "fontSizeCode" | "fontFamilyComposer" | "fontFamilyTerminal">>) => void
}): JSX.Element {
  return (
    <section aria-label={t("type.title")}>
      <h3 style={{ fontSize: 15, fontWeight: 600, margin: "0 0 6px" }}>{t("type.title")}</h3>
      <p style={{ fontSize: 12, opacity: 0.6, margin: "0 0 10px" }}>{t("type.desc")}</p>
      {renderRange("type.interfaceSize", settings.fontSizeInterface, MIN_INTERFACE_FONT_SIZE, MAX_INTERFACE_FONT_SIZE, (value) => onChange({ fontSizeInterface: value }))}
      {renderRange("type.codeSize", settings.fontSizeCode, MIN_CODE_FONT_SIZE, MAX_CODE_FONT_SIZE, (value) => onChange({ fontSizeCode: value }))}
      {renderText("type.sans", settings.fontFamilySans, (value) => onChange({ fontFamilySans: value }))}
      {renderText("type.code", settings.fontFamilyCode, (value) => onChange({ fontFamilyCode: value }))}
      {renderText("type.composer", settings.fontFamilyComposer, (value) => onChange({ fontFamilyComposer: value }))}
      {renderText("type.terminal", settings.fontFamilyTerminal, (value) => onChange({ fontFamilyTerminal: value }))}
    </section>
  )

  function renderRange(labelKey: string, value: number, min: number, max: number, apply: (value: number) => void): JSX.Element {
    return (
      <label style={{ display: "block", margin: "6px 0" }}>
        <span style={{ fontSize: 13 }}>{t(labelKey)}：{value}px</span>
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
    )
  }

  function renderText(labelKey: string, value: string, apply: (value: string) => void): JSX.Element {
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
    )
  }
}
```

- [ ] **Step 3: 在 `AppearanceSection.tsx` 追加两个子块**

新增 import：

```tsx
import { CustomThemeEditor } from "./CustomThemeEditor";
import { TypographySection } from "./TypographySection";
```

在 `<GlassSlider .../>` 之后追加：

```tsx
      <CustomThemeEditor store={store} t={t} />
      <TypographySection
        settings={settings}
        t={t}
        onChange={(patch) => store.setTypography(patch)}
      />
```

- [ ] **Step 4: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add packages/plugins/bridge/src/client/CustomThemeEditor.tsx packages/plugins/bridge/src/client/TypographySection.tsx packages/plugins/bridge/src/client/AppearanceSection.tsx
git commit -m "feat(bridge): 外观设置节（自定义主题 + 排版）"
```

---

### Task 7: client 面装配（外观设置节注册 + 主题应用接线 + 语言包）

**Files:**
- Modify: `packages/plugins/bridge/src/client.tsx`

**Interfaces:**
- Consumes: Task 2 `applyThemeSection`；Task 3 `createThemeStore`/`ThemeStore`；Task 4 `AppearanceSection`；Task 1 `THEME_SETTINGS_NAMESPACE`/`ThemeSettings`
- Produces: 装配完成的 client 面：绑定 `ui-theme` 命名空间、初始化应用、监听快照/系统主题变化、注册“外观”设置节（id `appearance`，order 5）、zh/en 语言包；`yarn build:plugins` 后产物进 `apps/shell/src-tauri/resources/plugins/bridge/`

- [ ] **Step 1: 扩展 `client.tsx` 的 `apply()`**

在 `client.tsx` 顶部新增 import：

```tsx
import {
  THEME_SETTINGS_NAMESPACE,
  type ThemeSettings,
} from "./shared/theme";
import { applyThemeSection } from "./client/theme-apply";
import { createThemeStore } from "./client/theme-store";
import { AppearanceSection } from "./client/AppearanceSection";
```

在 `apply()` 内、注册“桌面”设置节之前插入（外观节用独立语言命名空间 `settings.appearance`，避免污染桌面节字典）：

```tsx
  // ── 主题与背景（ui-theme 命名空间由上游 dsh-client-ui-theme host 注册，这里只 bind）──
  const THEME_NS = "settings.appearance";
  const themeScope = ctx.settingsScope.bind<ThemeSettings>({ namespace: THEME_SETTINGS_NAMESPACE });
  const themeStore = createThemeStore(themeScope);

  const systemDark = (): boolean =>
    typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;

  const applyNow = (): void => {
    applyThemeSection(themeStore.getSnapshot(), systemDark(), themeStore.getPreview());
  };

  ctx.effect(() => {
    applyNow();
    const unsubscribe = themeStore.subscribe(applyNow);
    const media = matchMedia("(prefers-color-scheme: dark)");
    const onMedia = (): void => {
      if (themeStore.getSnapshot().preference === "system") applyNow();
    };
    media.addEventListener("change", onMedia);
    return () => {
      unsubscribe();
      media.removeEventListener("change", onMedia);
    };
  }, "bridge: 主题应用");

  const themeT = ctx.locale.bind(THEME_NS);
  ctx.effect(() => ctx.locale.register(THEME_NS, "zh", themeSectionZh), "bridge: 外观中文字典");
  ctx.effect(() => ctx.locale.register(THEME_NS, "en", themeSectionEn), "bridge: 外观英文字典");
  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "appearance",
        order: 5,
        label: () => themeT("nav"),
        locale: THEME_NS,
        children: {},
      },
      () => <AppearanceSection store={themeStore} t={themeT} />,
    ),
  );
```

在文件底部（`apply` 之外）定义字典（键覆盖 Task 4/5/6 全部文案）：

```ts
const themeSectionZh = {
  nav: "外观",
  "pref.title": "主题偏好",
  "pref.light": "浅色",
  "pref.dark": "深色",
  "pref.system": "跟随系统",
  "library.title": "主题库",
  "library.desc": "每张卡有浅、深两半，点哪半用哪半",
  "library.lightHalf": "浅色半",
  "library.darkHalf": "深色半",
  "library.builtin": "内置",
  "library.custom": "自定义",
  "wallpaper.title": "背景图",
  "wallpaper.desc": "选一张图铺在整个界面后面，可调毛玻璃与像素化",
  "wallpaper.choose": "选择图片",
  "wallpaper.clear": "清除",
  "wallpaper.blur": "毛玻璃",
  "wallpaper.pixelate": "像素化",
  "wallpaper.reset": "重置效果",
  "glass.title": "玻璃透明度",
  "glass.desc": "数值越低，侧栏、对话框和输入框越通透",
  "glass.opacity": "透明度",
  "custom.title": "自定义主题",
  "custom.create": "新建",
  "custom.import": "导入",
  "custom.edit": "编辑",
  "custom.copy": "复制",
  "custom.export": "导出",
  "custom.remove": "删除",
  "custom.name": "名称",
  "custom.lightHalf": "浅色",
  "custom.darkHalf": "深色",
  "custom.accent": "强调色",
  "custom.background": "背景",
  "custom.foreground": "前景",
  "custom.contrast": "对比度",
  "custom.save": "保存",
  "custom.cancel": "取消",
  "custom.newName": "新主题",
  "type.title": "排版",
  "type.desc": "界面与代码字号、字体族",
  "type.interfaceSize": "界面字号",
  "type.codeSize": "代码字号",
  "type.sans": "界面字体族",
  "type.code": "代码字体族",
  "type.composer": "输入框字体族",
  "type.terminal": "终端字体族",
  "type.default": "留空使用默认栈",
}

const themeSectionEn = {
  nav: "Appearance",
  "pref.title": "Theme preference",
  "pref.light": "Light",
  "pref.dark": "Dark",
  "pref.system": "System",
  "library.title": "Theme library",
  "library.desc": "Each card has a light and dark half; click the half to use it",
  "library.lightHalf": "Light half",
  "library.darkHalf": "Dark half",
  "library.builtin": "Built-in",
  "library.custom": "Custom",
  "wallpaper.title": "Wallpaper",
  "wallpaper.desc": "Pick an image to lay behind the whole UI; adjust frost and pixelation",
  "wallpaper.choose": "Choose image",
  "wallpaper.clear": "Clear",
  "wallpaper.blur": "Frosted glass",
  "wallpaper.pixelate": "Pixelation",
  "wallpaper.reset": "Reset effects",
  "glass.title": "Glass opacity",
  "glass.desc": "Lower values make sidebar, dialogs and composer more translucent",
  "glass.opacity": "Opacity",
  "custom.title": "Custom themes",
  "custom.create": "Create",
  "custom.import": "Import",
  "custom.edit": "Edit",
  "custom.copy": "Duplicate",
  "custom.export": "Export",
  "custom.remove": "Remove",
  "custom.name": "Name",
  "custom.lightHalf": "Light",
  "custom.darkHalf": "Dark",
  "custom.accent": "Accent",
  "custom.background": "Background",
  "custom.foreground": "Foreground",
  "custom.contrast": "Contrast",
  "custom.save": "Save",
  "custom.cancel": "Cancel",
  "custom.newName": "New theme",
  "type.title": "Typography",
  "type.desc": "Interface and code font sizes and families",
  "type.interfaceSize": "Interface font size",
  "type.codeSize": "Code font size",
  "type.sans": "Interface font family",
  "type.code": "Code font family",
  "type.composer": "Composer font family",
  "type.terminal": "Terminal font family",
  "type.default": "Leave empty to keep the default stack",
}
```

- [ ] **Step 2: 构建与装配插件**

Run: `node scripts/build-plugins.mjs`
Expected: `[build-plugins] 已装配 @dsh-desktop/plugin-bridge@0.1.0 → resources/plugins/bridge/`，且 `lib/client.js` 包含新代码

- [ ] **Step 3: 类型检查**

Run: `yarn workspace @dsh-desktop/plugin-bridge typecheck`
Expected: 无错误

- [ ] **Step 4: 提交**

```bash
git add packages/plugins/bridge/src/client.tsx
git commit -m "feat(bridge): 装配外观设置节与主题应用接线"
```

---

### Task 8: 壳侧——Rust 读取 ui-theme 分节 + 新契约 + 窗口背景

**Files:**
- Modify: `apps/shell/src-tauri/Cargo.toml`（新增 `serde_yaml`）
- Create: `apps/shell/src-tauri/src/theme.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`（`mod theme`、新命令 `get_ui_theme`、事件 `dsh-ui-theme`、settings.yaml 轮询线程、窗口背景跟随）
- Modify: `apps/shell/src-tauri/build.rs`（`AppManifest::commands` 加 `get_ui_theme`）
- Modify: `apps/shell/src-tauri/capabilities/default.json`（加 `allow-get-ui-theme`）
- Modify: `packages/contracts/src/index.ts`（`UiThemeTokens`/`UiThemeSnapshot` 类型 + `COMMANDS.getUiTheme` + `EVENTS.dshUiTheme`）

**Interfaces:**
- Consumes: `config::load(...).effective(...)` 解析 `dsh_home`；现有 `AppState`/事件 emit 模式
- Produces: `get_ui_theme` 命令（返回 `UiThemeSnapshot`）与 `dsh-ui-theme` 事件（payload 同）；窗口背景随主题变化；Task 9 的启动页消费这些契约

- [ ] **Step 1: Cargo.toml 新增 `serde_yaml`**

```toml
serde_yaml = "0.9"
```

- [ ] **Step 2: 写 `src/theme.rs`（内置家族种子 + mixHex 推导 + 分节解析）**

```rust
//! 壳侧主题：读取 settings.yaml 的 ui-theme 分节，解析少量 token 供启动页/窗口背景。
//! 移植自参考仓库 src/shared/themes.js（家族种子 + mixHex + resolveMode）。

use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_FAMILY_ID: &str = "deepseek";

#[derive(Clone, Copy)]
struct Seeds {
    accent: &'static str,
    background: &'static str,
    foreground: &'static str,
}

struct Family {
    id: &'static str,
    name: &'static str,
    light: Seeds,
    dark: Seeds,
}

const fn seeds(accent: &'static str, background: &'static str, foreground: &'static str) -> Seeds {
    Seeds { accent, background, foreground }
}

const FAMILIES: &[Family] = &[
    Family { id: "deepseek", name: "DeepSeek", light: seeds("#4176e6", "#ffffff", "#0f1115"), dark: seeds("#6ea8ff", "#151517", "#f5f5f5") },
    Family { id: "midnight", name: "午夜", light: seeds("#3b6fd4", "#f3f6fb", "#1a1f2b"), dark: seeds("#6ea8ff", "#0b0d12", "#e8eef9") },
    Family { id: "celadon", name: "青瓷", light: seeds("#0f766e", "#f3faf7", "#10211c"), dark: seeds("#3dd6b5", "#071411", "#e7f6f1") },
    Family { id: "violet", name: "暮紫", light: seeds("#7c3aed", "#f7f3fc", "#1c1524"), dark: seeds("#c4a1ff", "#120e18", "#f3eefc") },
    Family { id: "amber", name: "琥珀", light: seeds("#b45309", "#fbf6ee", "#1c1915"), dark: seeds("#e2b15c", "#14100b", "#f6efe4") },
    Family { id: "paper", name: "宣纸", light: seeds("#0f766e", "#f3efe6", "#1c1915"), dark: seeds("#5eead4", "#1a1712", "#f6efe4") },
    Family { id: "contrast", name: "对比", light: seeds("#111111", "#ffffff", "#050505"), dark: seeds("#ffffff", "#050505", "#f5f5f5") },
];

/// settings.yaml 的 ui-theme 分节（camelCase 键；缺失字段回退默认）。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiThemeSection {
    pub preference: Option<String>,
    pub active_light_theme_id: Option<String>,
    pub active_dark_theme_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UiThemeTokens {
    pub bg: String,
    pub fg: String,
    pub muted: String,
    pub accent: String,
    pub field: String,
    pub line: String,
    pub button_fg: String,
    pub scheme: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UiThemeSnapshot {
    pub tokens: UiThemeTokens,
    pub preference: String,
    pub mode: String,
}

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let value = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&value[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&value[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&value[4..6], 16).unwrap_or(0);
    (r, g, b)
}

fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// 在 left 与 right 之间按 amount(0..=1) 插值。
fn mix_hex(left: &str, right: &str, amount: f64) -> String {
    let (lr, lg, lb) = parse_hex(left);
    let (rr, rg, rb) = parse_hex(right);
    let t = amount.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    to_hex(channel(lr, rr), channel(lg, rg), channel(lb, rb))
}

fn resolve_mode(preference: &str, system_dark: bool) -> &'static str {
    match preference {
        "dark" => "dark",
        "light" => "light",
        _ => {
            if system_dark {
                "dark"
            } else {
                "light"
            }
        }
    }
}

fn find_family(id: &str) -> &'static Family {
    FAMILIES
        .iter()
        .find(|family| family.id == id)
        .unwrap_or(&FAMILIES[0])
}

fn tokens_for(section: &UiThemeSection, mode: &str, system_dark: bool) -> UiThemeTokens {
    let family_id = if mode == "dark" {
        section.active_dark_theme_id.as_deref().unwrap_or(DEFAULT_FAMILY_ID)
    } else {
        section.active_light_theme_id.as_deref().unwrap_or(DEFAULT_FAMILY_ID)
    };
    let family = find_family(family_id);
    let seeds = if mode == "dark" { &family.dark } else { &family.light };
    let _ = system_dark;
    let scheme = if mode == "dark" { "dark" } else { "light" };
    let bg = seeds.background;
    let fg = seeds.foreground;
    let muted = mix_hex(fg, bg, 0.42);
    let field = mix_hex(bg, fg, 0.06);
    let line = if scheme == "light" { "rgba(15, 17, 21, 0.12)" } else { "rgba(245, 245, 245, 0.10)" };
    let button_fg = if scheme == "light" { mix_hex(bg, "#000000", 0.08) } else { mix_hex(bg, "#000000", 0.0) };
    UiThemeTokens {
        bg: bg.to_string(),
        fg: fg.to_string(),
        muted,
        accent: seeds.accent.to_string(),
        field,
        line: line.to_string(),
        button_fg,
        scheme: scheme.to_string(),
    }
}

/// 解析 settings.yaml 中的 ui-theme 分节；文件缺失/非法时返回默认分节。
pub fn read_ui_theme_section(settings_path: &Path) -> UiThemeSection {
    let Ok(raw) = std::fs::read_to_string(settings_path) else {
        return UiThemeSection::default();
    };
    let doc: Result<serde_yaml::Value, _> = serde_yaml::from_str(&raw);
    let Ok(doc) = doc else {
        return UiThemeSection::default();
    };
    let Some(section) = doc.get("ui-theme") else {
        return UiThemeSection::default();
    };
    serde_yaml::from_value(section.clone()).unwrap_or_default()
}

/// 组装启动页/窗口背景要用的快照。
pub fn resolve_ui_theme(section: &UiThemeSection, system_dark: bool) -> UiThemeSnapshot {
    let mode = resolve_mode(section.preference.as_deref().unwrap_or("system"), system_dark);
    let preference = section.preference.clone().unwrap_or_else(|| "system".to_string());
    UiThemeSnapshot {
        tokens: tokens_for(section, mode, system_dark),
        preference,
        mode: mode.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mode_resolves_system() {
        assert_eq!(resolve_mode("system", true), "dark");
        assert_eq!(resolve_mode("system", false), "light");
        assert_eq!(resolve_mode("light", true), "light");
        assert_eq!(resolve_mode("dark", false), "dark");
    }

    #[test]
    fn missing_section_falls_back_to_deepseek_dark() {
        let section = UiThemeSection::default();
        let snapshot = resolve_ui_theme(&section, true);
        assert_eq!(snapshot.mode, "dark");
        assert_eq!(snapshot.tokens.scheme, "dark");
        assert_eq!(snapshot.tokens.bg, "#151517");
    }

    #[test]
    fn parses_camel_case_section() {
        let raw = "ui-theme:\n  preference: light\n  activeDarkThemeId: midnight\n";
        let path = std::env::temp_dir().join("dsh-desktop-theme-test.yaml");
        std::fs::write(&path, raw).unwrap();
        let section = read_ui_theme_section(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(section.preference.as_deref(), Some("light"));
        assert_eq!(section.active_dark_theme_id.as_deref(), Some("midnight"));
        let snapshot = resolve_ui_theme(&section, true);
        assert_eq!(snapshot.tokens.bg, "#f3f6fb");
    }

    #[test]
    fn mix_hex_interpolates() {
        assert_eq!(mix_hex("#000000", "#ffffff", 1.0), "#ffffff");
        assert_eq!(mix_hex("#000000", "#ffffff", 0.0), "#000000");
    }
}
```

- [ ] **Step 3: lib.rs 接入 theme 模块、新命令与事件**

文件顶部追加：

```rust
mod theme;

use theme::{read_ui_theme_section, resolve_ui_theme, UiThemeSnapshot};
```

在 `run()` 的 `.setup()` 里、`start_tx` 线程之后追加轮询线程（settings.yaml 变化时 emit `dsh-ui-theme` 并设置窗口背景）：

```rust
            // 主题跟随：轮询 settings.yaml 的 ui-theme 分节，变化时发 dsh-ui-theme 并更新窗口背景
            {
                let app = app_handle.clone();
                let config_path_for_theme = config_path.clone();
                thread::spawn(move || {
                    let mut last: Option<String> = None;
                    loop {
                        thread::sleep(Duration::from_secs(2));
                        let config = config::load(&config_path_for_theme).effective(|k| std::env::var(k).ok());
                        let home = config
                            .dsh_home
                            .clone()
                            .or_else(|| std::env::var("DSH_HOME").ok())
                            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
                        let Some(home) = home else { continue };
                        let settings_path = std::path::Path::new(&home).join("settings.yaml");
                        let system_dark = app
                            .get_webview_window("main")
                            .and_then(|w| w.theme().ok())
                            .map(|t| t == tauri::Theme::Dark)
                            .unwrap_or(false);
                        let section = read_ui_theme_section(&settings_path);
                        let snapshot = resolve_ui_theme(&section, system_dark);
                        let key = format!("{}:{}:{}", snapshot.preference, snapshot.mode, snapshot.tokens.bg);
                        if last.as_deref() == Some(key.as_str()) {
                            continue;
                        }
                        last = Some(key);
                        let _ = app.emit("dsh-ui-theme", &snapshot);
                        if let Some(window) = app.get_webview_window("main") {
                            if let Some(color) = parse_window_color(&snapshot.tokens.bg) {
                                let _ = window.set_background_color(Some(color));
                            }
                        }
                    }
                });
            }
```

在 `invoke_handler` 列表追加 `get_ui_theme` 并实现（native-only，不暴露给 remote）：

```rust
/// #rrggbb → tauri::window::Color
fn parse_window_color(hex: &str) -> Option<tauri::window::Color> {
    let value = hex.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(tauri::window::Color(r, g, b, 255))
}

#[tauri::command]
fn get_ui_theme(app: AppHandle) -> UiThemeSnapshot {
    let system_dark = app
        .get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .map(|t| t == tauri::Theme::Dark)
        .unwrap_or(false);
    let home = std::env::var("DSH_HOME").ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.dsh")));
    let section = match home {
        Some(home) => read_ui_theme_section(&std::path::Path::new(&home).join("settings.yaml")),
        None => theme::UiThemeSection::default(),
    };
    resolve_ui_theme(&section, system_dark)
}
```

> 说明：`get_ui_theme` 是 native-only 命令（dsh web 经 settingsScope 直接读 dsh settings，不需要它），所以只进 `default.json` 与 `build.rs` 的 commands 数组，不碰 `bridge.json`。

- [ ] **Step 4: build.rs 与 capabilities/default.json 声明新命令**

`build.rs` 的 `AppManifest::new().commands(&[...])` 数组追加 `"get_ui_theme"`；`capabilities/default.json` 的 permissions 追加 `"allow-get-ui-theme"`。

- [ ] **Step 5: contracts 增加 native 契约**

`packages/contracts/src/index.ts` 追加：

```ts
export interface UiThemeTokens {
  bg: string;
  fg: string;
  muted: string;
  accent: string;
  field: string;
  line: string;
  button_fg: string;
  scheme: "light" | "dark";
}

export interface UiThemeSnapshot {
  tokens: UiThemeTokens;
  preference: "light" | "dark" | "system";
  mode: "light" | "dark";
}
```

`COMMANDS` 追加 `getUiTheme: "get_ui_theme"`；`EVENTS` 追加 `dshUiTheme: "dsh-ui-theme"`。

- [ ] **Step 6: 编译与单测**

Run: `cd apps/shell/src-tauri && cargo fmt && cargo test && cargo check`
Expected: theme.rs 四个单测通过；`cargo check` 无错误；`cargo clippy` 无警告

- [ ] **Step 7: 提交**

```bash
git add apps/shell/src-tauri/Cargo.toml apps/shell/src-tauri/src/theme.rs apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/build.rs apps/shell/src-tauri/capabilities/default.json packages/contracts/src/index.ts
git commit -m "feat(shell): 壳侧读取 ui-theme 分节（get_ui_theme/dsh-ui-theme + 窗口背景跟随）"
```

---

### Task 9: 壳侧——启动页跟随主题

**Files:**
- Modify: `apps/shell/src/main.ts`
- Modify: `apps/shell/src/style.css`

**Interfaces:**
- Consumes: Task 8 的 `UiThemeSnapshot`（`packages/contracts`）与 `get_ui_theme`/`dsh-ui-theme`
- Produces: 启动页初始加载 + 实时更新主题变量（`--theme-*`，映射自 `style.css` 现有变量）

- [ ] **Step 1: 修改 `style.css` 使变量可被 JS 覆盖**

在 `:root` 规则内新增主题占位（保持现有默认值，JS 设置内联变量时优先）：

```css
  /* 主题跟随（由 main.ts 经 dsh-ui-theme 写入） */
  --theme-bg: var(--bg);
  --theme-ink: var(--ink);
  --theme-muted: var(--muted);
  --theme-line: var(--line);
  --theme-accent: var(--accent);
  --theme-accent-ink: var(--accent-ink);
```

并把关键引用改为主题变量（默认样式表值保留，作为非 Tauri 环境兜底）：

```css
body {
  background: var(--theme-bg);
  color: var(--theme-ink);
}
.eyebrow { color: var(--theme-accent-ink); }
.status__dot { background: var(--theme-accent); }
```

（`--muted`/`--line`/按钮边框等按需替换为 `--theme-*` 引用；深色模式 `@media (prefers-color-scheme: dark)` 里的覆盖值保持不动，作为无主题时的兜底。）

- [ ] **Step 2: 修改 `main.ts` 应用主题**

新增：

```ts
import type { UiThemeSnapshot } from "@dsh-desktop/contracts";

function applyUiTheme(snapshot: UiThemeSnapshot): void {
  const root = document.documentElement;
  const tokens = snapshot.tokens;
  root.style.setProperty("--theme-bg", tokens.bg);
  root.style.setProperty("--theme-ink", tokens.fg);
  root.style.setProperty("--theme-muted", tokens.muted);
  root.style.setProperty("--theme-line", tokens.line);
  root.style.setProperty("--theme-accent", tokens.accent);
  root.style.setProperty("--theme-accent-ink", tokens.accent);
  root.style.colorScheme = tokens.scheme;
}

async function initTheme(): Promise<void> {
  if (!isTauri()) return;
  try {
    const snapshot = await invoke<UiThemeSnapshot>("get_ui_theme");
    applyUiTheme(snapshot);
  } catch {
    // 忽略：主题不可用时保持样式表默认
  }
  await listen<UiThemeSnapshot>("dsh-ui-theme", (event) => applyUiTheme(event.payload));
}
```

在 `init()` 内、现有 `get_status` 之后调用 `await initTheme();`。

- [ ] **Step 3: 构建与验证**

Run: `yarn build:web`
Expected: `dist/` 产物生成；在 `yarn dev` 启动的应用里，启动页背景/强调色随 `settings.yaml` 的 `ui-theme` 变化

- [ ] **Step 4: 提交**

```bash
git add apps/shell/src/main.ts apps/shell/src/style.css
git commit -m "feat(shell): 启动页跟随 ui-theme 主题"
```

---

### Task 10: 无边框窗口 + 窗口控制命令/事件/桥接

**Files:**
- Modify: `apps/shell/src-tauri/tauri.conf.json`（main 窗口 `decorations: false`）
- Modify: `apps/shell/src-tauri/src/lib.rs`（`window_action` 命令、`dsh-window-state` 事件、BRIDGE_SCRIPT 扩展）
- Modify: `apps/shell/src-tauri/build.rs`（commands 加 `window_action`）
- Modify: `apps/shell/src-tauri/capabilities/default.json` 与 `capabilities/bridge.json`（加 `allow-window-action`）
- Modify: `packages/contracts/src/index.ts`（`WindowAction`/`WindowState` + 常量）
- Modify: `packages/plugins/bridge/src/index.ts`（`DshDesktopBridge` 类型加 `windowAction`/`onWindowState`）

**Interfaces:**
- Consumes: Tauri 2 的 `WebviewWindow` API（`minimize`/`maximize`/`unmaximize`/`close`/`is_maximized`）；现有桥接注入模式
- Produces: `window_action` 命令（`minimize`/`maximize`/`close`，maximize 为切换）与 `dsh-window-state` 事件（`{ maximized }`）；BRIDGE_SCRIPT 暴露 `windowAction`/`onWindowState`；Task 11/12 的标题栏按钮与图标切换消费

- [ ] **Step 1: tauri.conf.json 设置无边框**

`apps/shell/src-tauri/tauri.conf.json` 的 `app.windows[0]`（label `main`）增加：

```json
"decorations": false
```

- [ ] **Step 2: lib.rs 新增命令、事件与桥接扩展**

新增命令与状态事件（`invoke_handler` 列表追加 `window_action`）：

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WindowStateSnapshot {
    maximized: bool,
}

/// 无边框窗口控制：minimize / maximize（切换）/ close。
#[tauri::command]
fn window_action(window: tauri::WebviewWindow, action: String) -> Result<(), String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => window.close().map_err(|e| e.to_string()),
        other => Err(format!("未知窗口动作: {other}")),
    }
}

fn emit_window_state(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = app.emit(
            "dsh-window-state",
            WindowStateSnapshot { maximized: window.is_maximized().unwrap_or(false) },
        );
    }
}
```

在 `.setup()` 末尾调用一次 `emit_window_state(app.handle())`；在 `.on_window_event` 里对 main 窗口的 `WindowEvent::Resized` 分支调用（最大化/还原都会触发）：

```rust
                if let WindowEvent::Resized(_) = event {
                    emit_window_state(window.app_handle());
                }
```

在 `DshManager::open_window`（就绪导航后）调用一次 `emit_window_state(&self.app)`。

`BRIDGE_SCRIPT` 的 `window.__DSH_DESKTOP__` 对象追加两个成员：

```js
    windowAction: function (action) { return invoke("window_action", { action: action }); },
    onWindowState: function (cb) { return listen("dsh-window-state", cb); },
```

- [ ] **Step 3: 权限声明**

`build.rs` 的 commands 数组追加 `"window_action"`；`capabilities/default.json` 与 `capabilities/bridge.json` 的 permissions 均追加 `"allow-window-action"`（本地启动页与 dsh web 注入的标题栏都要调用）。

- [ ] **Step 4: contracts 增加契约**

`packages/contracts/src/index.ts` 追加：

```ts
export type WindowAction = "minimize" | "maximize" | "close";

export interface WindowState {
  maximized: boolean;
}
```

`COMMANDS` 追加 `windowAction: "window_action"`；`EVENTS` 追加 `dshWindowState: "dsh-window-state"`。

- [ ] **Step 5: 更新 bridge 插件的桥接类型**

`packages/plugins/bridge/src/index.ts` 的 `DshDesktopBridge` 接口追加：

```ts
  windowAction(action: "minimize" | "maximize" | "close"): Promise<void>;
  onWindowState(cb: (state: { maximized: boolean }) => void): Promise<() => void>;
```

- [ ] **Step 6: 编译与验证**

Run: `cd apps/shell/src-tauri && cargo fmt && cargo check && cargo clippy`；`yarn typecheck`
Expected: 无错误；`yarn dev` 启动后窗口无系统边框（关闭到托盘行为不变）

- [ ] **Step 7: 提交**

```bash
git add apps/shell/src-tauri/tauri.conf.json apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/build.rs apps/shell/src-tauri/capabilities/default.json apps/shell/src-tauri/capabilities/bridge.json packages/contracts/src/index.ts packages/plugins/bridge/src/index.ts
git commit -m "feat(shell): 无边框窗口与窗口控制命令/事件（window_action/dsh-window-state）"
```

---

### Task 11: 启动页自绘标题栏

**Files:**
- Modify: `apps/shell/index.html`（新增标题栏标记）
- Modify: `apps/shell/src/style.css`（标题栏样式）
- Modify: `apps/shell/src/main.ts`（接线窗口控制按钮）

**Interfaces:**
- Consumes: Task 10 的 `window_action`/`dsh-window-state`（`WindowState`）与 `--theme-*` 变量（Task 9）
- Produces: 启动页顶部的自绘标题栏：拖动条（`data-tauri-drag-region`）+ 最小化/最大化/关闭按钮，最大化图标随状态切换；非 Tauri 环境隐藏

- [ ] **Step 1: index.html 新增标题栏**

在 `<body>` 顶部、`<main class="shell">` 之前插入：

```html
    <header class="titlebar" id="titlebar" data-tauri-drag-region="deep">
      <span class="titlebar__title">DSH Desktop</span>
      <div class="titlebar__controls">
        <button type="button" id="win-min" aria-label="最小化" title="最小化">
          <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2" y="5.4" width="8" height="1.2" rx="0.6" fill="currentColor"/></svg>
        </button>
        <button type="button" id="win-max" aria-label="最大化" title="最大化">
          <svg viewBox="0 0 12 12" aria-hidden="true" id="win-max-icon"><rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>
        </button>
        <button type="button" id="win-close" aria-label="关闭" title="关闭">
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3L3 9" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></svg>
        </button>
      </div>
    </header>
```

> 说明：`data-tauri-drag-region="deep"` 使标题栏整条可拖；按钮是可点击元素，Tauri 的 drag.js 会自动阻断拖拽（无需额外处理）；双击拖动条由 Tauri 自动最大化（macOS 同样生效）。

- [ ] **Step 2: style.css 新增标题栏样式**

```css
.titlebar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-left: 14px;
  background: var(--theme-bg);
  color: var(--theme-ink);
  -webkit-user-select: none;
  user-select: none;
  z-index: 10;
}
.titlebar__title {
  font-size: 13px;
  font-weight: 600;
  opacity: 0.8;
}
.titlebar__controls {
  display: flex;
  height: 100%;
}
.titlebar__controls button {
  width: 46px;
  height: 100%;
  border: 0;
  border-radius: 0;
  background: transparent;
  color: var(--theme-ink);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
}
.titlebar__controls button:hover {
  background: rgba(128, 128, 128, 0.18);
}
.titlebar__controls #win-close:hover {
  background: #e81123;
  color: #fff;
}
.titlebar__controls svg {
  width: 12px;
  height: 12px;
}
/* 非 Tauri 环境隐藏标题栏 */
html:not([data-tauri]) .titlebar {
  display: none;
}
```

启动页主体让出标题栏高度：

```css
.shell {
  padding-top: 64px;
}
```

- [ ] **Step 3: main.ts 接线窗口控制**

新增（利用既有 `isTauri()` 与 `invoke`/`listen`）：

```ts
import type { WindowState } from "@dsh-desktop/contracts";

function windowAction(action: "minimize" | "maximize" | "close"): void {
  invoke("window_action", { action }).catch((error) => {
    console.error("window_action 失败:", error);
  });
}

async function initWindowControls(): Promise<void> {
  if (!isTauri()) return;
  document.getElementById("win-min")?.addEventListener("click", () => windowAction("minimize"));
  document.getElementById("win-max")?.addEventListener("click", () => windowAction("maximize"));
  document.getElementById("win-close")?.addEventListener("click", () => windowAction("close"));
  await listen<WindowState>("dsh-window-state", (event) => {
    const icon = document.getElementById("win-max-icon");
    if (icon !== null) {
      icon.innerHTML = event.payload.maximized
        ? '<rect x="3.4" y="2.2" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/><rect x="2.2" y="3.6" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/>'
        : '<rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/>';
    }
    const button = document.getElementById("win-max");
    if (button !== null) {
      button.setAttribute("aria-label", event.payload.maximized ? "还原" : "最大化");
    }
  });
}
```

在 `init()` 内调用 `await initWindowControls();`（与 `initTheme()` 并列）。

- [ ] **Step 4: 构建与验证**

Run: `yarn build:web`；`yarn dev` 启动
Expected: 启动页顶部显示自绘标题栏：可拖动、双击最大化、三个按钮生效、最大化图标随状态切换、标题栏背景跟随主题

- [ ] **Step 5: 提交**

```bash
git add apps/shell/index.html apps/shell/src/style.css apps/shell/src/main.ts
git commit -m "feat(shell): 启动页自绘标题栏（拖动/双击最大化/窗口控制）"
```

---

### Task 12: dsh web 页面自绘标题栏注入（HARNESS_CHROME_SCRIPT）

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`（新增 `HARNESS_CHROME_SCRIPT` 常量并在 `on_page_load` 注入 dsh web 页面）

**Interfaces:**
- Consumes: Task 10 的桥接 `windowAction`/`onWindowState`；现有 `BRIDGE_SCRIPT` 注入点（`on_page_load` + `is_dsh_web_url`）
- Produces: dsh web 页面内注入的自绘标题栏：顶部拖动条（`data-tauri-drag-region="deep"`）+ 右上角窗口控制按钮（参考 harness-chrome-inject.js）；按钮颜色跟随页面主题（读 `--dsw-alias-label-primary`）

- [ ] **Step 1: 新增 `HARNESS_CHROME_SCRIPT` 常量**

在 `BRIDGE_SCRIPT` 之后追加（Rust 原始字符串，JS 内不写模板字面量以免转义）：

```rust
/// 注入 dsh web 的自绘标题栏脚本（参考参考仓库 harness-chrome-inject.js，适配 Tauri data-tauri-drag-region）。
const HARNESS_CHROME_SCRIPT: &str = r##"(function () {
  "use strict";
  if (document.getElementById("dsh-shell-controls")) return;
  var STYLE_ID = "dsh-shell-chrome-style";
  var CONTROLS_ID = "dsh-shell-controls";
  var DRAG_ID = "dsh-shell-drag-strip";
  var EDGE = 8;
  var SIZE = 32;
  var GAP = 0;

  var ICON_MIN = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2" y="5.4" width="8" height="1.2" rx="0.6" fill="currentColor"/></svg>';
  var ICON_MAX = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.4" y="2.4" width="7.2" height="7.2" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>';
  var ICON_RESTORE = '<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="3.4" y="2.2" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/><rect x="2.2" y="3.6" width="6.2" height="6.2" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.15"/></svg>';
  var ICON_CLOSE = '<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3L3 9" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/></svg>';

  function reservedRight() {
    return EDGE + SIZE * 3 + GAP * 2;
  }

  function ensureStyle() {
    var style = document.getElementById(STYLE_ID);
    if (style) return;
    style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = [
      "#" + CONTROLS_ID + " {",
      "  position: fixed;",
      "  top: 4px;",
      "  right: " + EDGE + "px;",
      "  z-index: 2147483647;",
      "  display: flex;",
      "  gap: " + GAP + "px;",
      "  height: " + SIZE + "px;",
      "}",
      "#" + CONTROLS_ID + " button {",
      "  width: " + SIZE + "px;",
      "  height: " + SIZE + "px;",
      "  margin: 0;",
      "  padding: 0;",
      "  display: inline-flex;",
      "  align-items: center;",
      "  justify-content: center;",
      "  border: 0;",
      "  border-radius: 8px;",
      "  background: transparent;",
      "  color: var(--dsh-ctrl-fg, #3f3f46);",
      "  cursor: pointer;",
      "}",
      "#" + CONTROLS_ID + " button svg { width: 12px; height: 12px; display: block; }",
      "#" + CONTROLS_ID + " button:hover { background: var(--dsh-ctrl-hover, rgba(0, 0, 0, 0.08)); }",
      "#" + CONTROLS_ID + " button[data-act=close]:hover { background: #e81123; color: #fff; }",
      "#" + DRAG_ID + " {",
      "  position: fixed;",
      "  top: 0;",
      "  left: 0;",
      "  right: " + reservedRight() + "px;",
      "  height: 44px;",
      "  z-index: 2147483644;",
      "}"
    ].join("\n");
    (document.head || document.documentElement).appendChild(style);
  }

  function findTopBar() {
    var buttons = document.querySelectorAll("button");
    for (var i = 0; i < buttons.length; i++) {
      var label = (buttons[i].getAttribute("aria-label") || "") + " " + (buttons[i].textContent || "");
      if (/session\s*log/i.test(label)) {
        var header = buttons[i].closest("header");
        if (header) return header;
      }
    }
    var nodes = document.querySelectorAll("header, [role=banner]");
    for (var j = 0; j < nodes.length; j++) {
      var rect = nodes[j].getBoundingClientRect();
      if (rect.top <= 8 && rect.height >= 32 && rect.height <= 160) return nodes[j];
    }
    return null;
  }

  function ensureControls() {
    var host = document.getElementById(CONTROLS_ID);
    if (host) return host;
    host = document.createElement("div");
    host.id = CONTROLS_ID;
    host.innerHTML = [
      '<button type="button" data-act="minimize" aria-label="最小化">' + ICON_MIN + "</button>",
      '<button type="button" data-act="maximize" aria-label="最大化">' + ICON_MAX + "</button>",
      '<button type="button" data-act="close" aria-label="关闭">' + ICON_CLOSE + "</button>"
    ].join("");
    host.addEventListener("click", function (event) {
      var button = event.target.closest("[data-act]");
      if (!button || !window.__DSH_DESKTOP__) return;
      window.__DSH_DESKTOP__.windowAction(button.dataset.act);
    });
    (document.body || document.documentElement).appendChild(host);
    return host;
  }

  function ensureDragStrip() {
    var strip = document.getElementById(DRAG_ID);
    if (!strip) {
      strip = document.createElement("div");
      strip.id = DRAG_ID;
      strip.setAttribute("data-tauri-drag-region", "deep");
      (document.body || document.documentElement).appendChild(strip);
    }
    return strip;
  }

  function applyControlTheme(host) {
    var fg = getComputedStyle(document.body).getPropertyValue("--dsw-alias-label-primary").trim();
    if (!fg) {
      fg = getComputedStyle(document.body).getPropertyValue("color") || "#3f3f46";
    }
    host.style.setProperty("--dsh-ctrl-fg", fg);
    host.style.setProperty("--dsh-ctrl-hover", "rgba(128, 128, 128, 0.18)");
  }

  function install() {
    ensureStyle();
    var host = ensureControls();
    var bar = findTopBar();
    ensureDragStrip();
    if (bar && bar instanceof HTMLElement) {
      bar.setAttribute("data-tauri-drag-region", "deep");
      var prev = parseFloat(bar.style.paddingRight) || 0;
      bar.style.paddingRight = Math.max(prev, reservedRight()) + "px";
    }
    applyControlTheme(host);
    if (window.__DSH_DESKTOP__ && typeof window.__DSH_DESKTOP__.onWindowState === "function") {
      window.__DSH_DESKTOP__.onWindowState(function (state) {
        var maximized = !!(state && state.maximized);
        var maxBtn = host.querySelector("[data-act=maximize]");
        if (maxBtn) {
          maxBtn.innerHTML = maximized ? ICON_RESTORE : ICON_MAX;
          maxBtn.setAttribute("aria-label", maximized ? "还原" : "最大化");
        }
      });
    }
  }

  install();
})();"##;
```

- [ ] **Step 2: 在 `on_page_load` 注入**

把现有注入块改为先注入桥接、再注入标题栏：

```rust
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished && is_dsh_web_url(payload.url()) {
                let _ = webview.eval(BRIDGE_SCRIPT);
                let _ = webview.eval(HARNESS_CHROME_SCRIPT);
            }
        })
```

- [ ] **Step 3: 编译与验证**

Run: `cd apps/shell/src-tauri && cargo fmt && cargo check && cargo clippy`；`yarn dev` 启动进入 dsh web
Expected: dsh web 顶部可拖动、双击最大化；右上角最小化/最大化/关闭按钮生效（关闭仍走关闭到托盘）；最大化图标随状态切换；按钮颜色跟随页面主题

- [ ] **Step 4: 提交**

```bash
git add apps/shell/src-tauri/src/lib.rs
git commit -m "feat(shell): 向 dsh web 注入自绘标题栏（拖动条 + 窗口控制）"
```

---

### Task 13: 文档同步与全量验证

**Files:**
- Modify: `AGENTS.md`（根 README 通信契约表）
- Modify: `apps/shell/AGENTS.md`（启动页通信契约表 + 标题栏）
- Modify: `apps/shell/src-tauri/AGENTS.md`（IPC 命令/事件表 + 原生能力 + 无边框窗口）
- Modify: `packages/contracts/AGENTS.md`（契约清单）
- Modify: `packages/plugins/bridge/AGENTS.md`（client 面职责：外观设置节）
- Modify: `docs/plugin-tauri-boundary.md`（§6 bridge 插件定位补充外观节说明）
- Modify: `README.md`（特性列表补“主题与背景图”与“无边框窗口 + 自绘标题栏”）

**Interfaces:**
- Consumes: Task 1–12 的全部契约与文件

- [ ] **Step 1: 更新契约表**

各 AGENTS.md 的 IPC 契约表补：`get_ui_theme` 命令（`UiThemeSnapshot`）、`window_action` 命令（`WindowAction`）、`dsh-ui-theme` 事件、`dsh-window-state` 事件（`WindowState`）；bridge AGENTS.md 补“外观”设置节与 `ui-theme` 命名空间 bind（不注册）以及 `windowAction`/`onWindowState` 桥接成员。

- [ ] **Step 2: 更新边界文档与 README**

`docs/plugin-tauri-boundary.md`：§6 补充 bridge client 面“外观”设置节复用上游 `ui-theme` 命名空间；壳侧 `get_ui_theme` 属 native 契约（§4），`window_action` 属受控桥接（§5）。`README.md` 特性列表追加“主题与背景图：设置 → 外观（内置 7 主题家族浅/深两半、自定义主题、背景图毛玻璃/像素化/玻璃透明度、排版），启动页与窗口背景跟随”与“无边框窗口 + 自绘标题栏：可拖动、双击最大化，最小化/最大化/关闭按钮齐全，标题栏背景跟随主题”。

- [ ] **Step 3: 全量验证**

Run（仓库根）：
```bash
yarn typecheck
node scripts/build-plugins.mjs
yarn build:web
cd apps/shell/src-tauri && cargo fmt --check && cargo clippy && cargo test
```
Expected: 全部通过；`yarn lint`（biome check）无错误

- [ ] **Step 4: 端到端手动验证**

`yarn dev` 启动应用，逐一验证：① 设置 → 外观——偏好 cube、主题库点浅/深半、自定义主题新建/复制/编辑（实时预览）/导入/导出、背景图选图与毛玻璃/像素化/玻璃透明度、排版字号；确认 `settings.yaml` 的 `ui-theme` 分节被写入；重启应用后启动页与窗口背景跟随。② 无边框窗口——启动页与 dsh web 都能拖动、双击最大化、三个窗口控制按钮正常、最大化图标切换、关闭到托盘行为不变。

- [ ] **Step 5: 提交**

```bash
git add AGENTS.md apps/shell/AGENTS.md apps/shell/src-tauri/AGENTS.md packages/contracts/AGENTS.md packages/plugins/bridge/AGENTS.md docs/plugin-tauri-boundary.md README.md
git commit -m "docs: 同步主题/背景图与无边框标题栏的契约表与文档"
```

---

## 任务依赖一览

| 任务 | 依赖 | 交付物 |
| --- | --- | --- |
| 1 共享模型 | — | `src/shared/theme.ts` + 单测 |
| 2 主题应用 | 1 | `theme-apply.ts`（`applyThemeSection`/`wallpaperStyleSheet`） |
| 3 快照存储 | 1 | `theme-store.ts`（`createThemeStore`） |
| 4 主题库 UI | 3 | `AppearanceSection`/`ThemeLibrary` + css |
| 5 背景图/玻璃 UI | 1,3 | `WallpaperRow`/`GlassSlider` |
| 6 自定义主题/排版 UI | 1,3 | `CustomThemeEditor`/`TypographySection` |
| 7 client 装配 | 2,3,4,5,6 | 外观设置节注册 + 主题应用接线 + 语言包 |
| 8 Rust 壳侧主题 | — | `theme.rs` + `get_ui_theme`/`dsh-ui-theme` + 契约 + 窗口背景 |
| 9 启动页跟随主题 | 8 | 启动页主题变量应用 |
| 10 无边框 + 窗口控制 | — | `decorations:false` + `window_action`/`dsh-window-state` + 桥接扩展 |
| 11 启动页标题栏 | 10 | 启动页自绘标题栏 |
| 12 dsh web 标题栏注入 | 10 | `HARNESS_CHROME_SCRIPT` |
| 13 文档 + 验证 | 全部 | 契约表/边界文档/README 同步 + 全量验证 |
