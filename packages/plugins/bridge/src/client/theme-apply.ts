/** client 面主题应用：把 ui-theme 分节写到 dsh web DOM（--dsw-alias-* 变量 + 背景图层）。 */
import {
  appearanceFontStack,
  clampWallpaperEffect,
  DEFAULT_CODE_FONT_SIZE,
  DEFAULT_CODE_STACK,
  DEFAULT_FAMILY_ID,
  DEFAULT_INTERFACE_FONT_SIZE,
  DEFAULT_SANS_STACK,
  deriveThemeTokens,
  isWallpaperDataUrl,
  mixWallpaperSurfaces,
  resolveMode,
  resolveThemeFamily,
  type ThemeFamily,
  type ThemeSettings,
  type ThemeTokens,
  WALLPAPER_ATTR,
  WALLPAPER_BLEED,
  WALLPAPER_INNER_ID,
  WALLPAPER_LAYER_ID,
  wallpaperBlurPx,
  wallpaperPixelFactor,
} from "../shared/theme";

type FontKind = "interface" | "code";

interface FontToken {
  name: string;
  size: number;
  line: number;
  weight: number | string;
  style?: "normal" | "italic";
  kind: FontKind;
}

/** dsh ui-theme 在 body 上发布的字体 token（gradient-shadow-text.css 同源）。 */
const FONT_TOKENS: readonly FontToken[] = [
  { name: "--dsw-font-markdown-h1", size: 24, line: 34, weight: 700, kind: "interface" },
  { name: "--dsw-font-markdown-h2", size: 22, line: 32, weight: 700, kind: "interface" },
  { name: "--dsw-font-markdown-h3", size: 20, line: 30, weight: 700, kind: "interface" },
  { name: "--dsw-font-markdown-h4", size: 16, line: 28, weight: 600, kind: "interface" },
  { name: "--dsw-font-markdown-base", size: 16, line: 28, weight: 400, kind: "interface" },
  { name: "--dsw-font-markdown-base-strong", size: 16, line: 28, weight: 600, kind: "interface" },
  {
    name: "--dsw-font-markdown-base-italic",
    size: 16,
    line: 28,
    weight: 400,
    style: "italic",
    kind: "interface",
  },
  {
    name: "--dsw-font-markdown-base-strong-italic",
    size: 16,
    line: 28,
    weight: 600,
    style: "italic",
    kind: "interface",
  },
  { name: "--dsw-font-markdown-table", size: 15, line: 25, weight: 400, kind: "interface" },
  { name: "--dsw-font-markdown-table-head", size: 15, line: 25, weight: 500, kind: "interface" },
  { name: "--dsw-font-markdown-small", size: 14, line: 24, weight: 400, kind: "interface" },
  { name: "--dsw-font-markdown-small-strong", size: 14, line: 24, weight: 600, kind: "interface" },
  {
    name: "--dsw-font-markdown-small-italic",
    size: 14,
    line: 24,
    weight: 400,
    style: "italic",
    kind: "interface",
  },
  {
    name: "--dsw-font-markdown-small-strong-italic",
    size: 14,
    line: 24,
    weight: 600,
    style: "italic",
    kind: "interface",
  },
  { name: "--dsw-font-markdown-code", size: 14, line: 22, weight: 400, kind: "code" },
  { name: "--dsw-font-markdown-code-block", size: 13, line: 22, weight: 400, kind: "code" },
  { name: "--dsw-font-markdown-code-block-small", size: 12, line: 18, weight: 400, kind: "code" },
  { name: "--dsw-font-xl-24", size: 24, line: 32, weight: 600, kind: "interface" },
  { name: "--dsw-font-l-20", size: 20, line: 28, weight: 500, kind: "interface" },
  { name: "--dsw-font-m-18", size: 16, line: 28, weight: 500, kind: "interface" },
  { name: "--dsw-font-base-16", size: 16, line: 24, weight: 400, kind: "interface" },
  { name: "--dsw-font-base-strong-16", size: 16, line: 24, weight: 500, kind: "interface" },
  { name: "--dsw-font-s-14", size: 14, line: 22, weight: 400, kind: "interface" },
  { name: "--dsw-font-s-strong-14", size: 14, line: 22, weight: 500, kind: "interface" },
  { name: "--dsw-font-xs-13", size: 13, line: 20, weight: 400, kind: "interface" },
  { name: "--dsw-font-xs-strong-13", size: 13, line: 20, weight: 500, kind: "interface" },
  { name: "--dsw-font-xxs-12", size: 12, line: 18, weight: 400, kind: "interface" },
  { name: "--dsw-font-xxs-strong-12", size: 12, line: 18, weight: 500, kind: "interface" },
  { name: "--dsw-font-xxxs-11", size: 11, line: 14, weight: 400, kind: "interface" },
  { name: "--dsw-font-xxxs-strong-11", size: 11, line: 14, weight: 500, kind: "interface" },
];

function roundPixel(value: number): string {
  return `${Math.round(value * 100) / 100}px`;
}

function applyTypographyTokens(
  body: HTMLElement,
  section: ThemeSettings,
  sans: string,
  code: string,
  composer: string,
  terminal: string,
): void {
  const interfaceSize = section.fontSizeInterface || DEFAULT_INTERFACE_FONT_SIZE;
  const codeSize = section.fontSizeCode || DEFAULT_CODE_FONT_SIZE;
  const interfaceFactor = interfaceSize / DEFAULT_INTERFACE_FONT_SIZE;
  const codeFactor = codeSize / DEFAULT_CODE_FONT_SIZE;

  for (const token of FONT_TOKENS) {
    const factor = token.kind === "code" ? codeFactor : interfaceFactor;
    const size = token.size * factor;
    const line = token.line * factor;
    const family = token.kind === "code" ? "var(--ds-font-family-code)" : "var(--dsw-font-family)";
    const style = token.style ?? "normal";
    const prefix = style === "italic" ? "italic " : "";
    body.style.setProperty(
      token.name,
      `${prefix}${token.weight} ${roundPixel(size)}/${roundPixel(line)} ${family}`,
    );
    body.style.setProperty(`${token.name}-font-family`, family);
    body.style.setProperty(`${token.name}-font-weight`, String(token.weight));
    body.style.setProperty(`${token.name}-line-height`, roundPixel(line));
    body.style.setProperty(`${token.name}-font-size`, roundPixel(size));
    body.style.setProperty(`${token.name}-font-style`, style);
  }

  body.style.setProperty("--dsw-font-size-code", `${codeSize}px`);
  body.style.setProperty("--dsw-font-family", sans);
  body.style.setProperty("--ds-font-family-code", code);
  body.style.setProperty("--dsw-font-family-composer", composer);
  body.style.setProperty("--dsw-font-family-terminal", terminal);
  body.style.setProperty("--dsw-font-mono", code);
  body.style.fontFamily = sans;
  body.style.fontSize = `${interfaceSize}px`;
}

/** 背景图层样式（上游 rc.6 无 wallpaper.css，由本插件注入）。 */
export function wallpaperStyleSheet(): string {
  return [
    `#${WALLPAPER_LAYER_ID} {`,
    "  position: fixed;",
    "  inset: 0;",
    "  z-index: 0;",
    "  overflow: hidden;",
    "  pointer-events: none;",
    "}",
    `#${WALLPAPER_INNER_ID} {`,
    "  position: absolute;",
    `  left: -${WALLPAPER_BLEED}px;`,
    `  top: -${WALLPAPER_BLEED}px;`,
    `  width: calc(100% + ${WALLPAPER_BLEED * 2}px);`,
    `  height: calc(100% + ${WALLPAPER_BLEED * 2}px);`,
    "  background-position: center;",
    "  background-repeat: no-repeat;",
    "  background-size: cover;",
    "  image-rendering: pixelated;",
    "  filter: blur(var(--dsh-wallpaper-blur, 0px));",
    "}",
    `html[${WALLPAPER_ATTR}],`,
    `html[${WALLPAPER_ATTR}] body,`,
    `html[${WALLPAPER_ATTR}] #root {`,
    "  background: transparent;",
    "}",
    `html[${WALLPAPER_ATTR}] #root {`,
    "  position: relative;",
    "  z-index: 1;",
    "}",
  ].join("\n");
}

let styleInjected = false;

const PAGE_STYLE_ID = "dsh-desktop-page-style";

/** 全局页面样式：阻止 dsh web 滚动到底/顶时触发系统 overscroll。 */
export function ensurePageStyle(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(PAGE_STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = PAGE_STYLE_ID;
  style.textContent = "html, body, #root { overscroll-behavior: none; }";
  document.documentElement.appendChild(style);
}

function ensureWallpaperStyle(): void {
  if (styleInjected) return;
  const style = document.createElement("style");
  style.id = "dsh-wallpaper-style";
  style.textContent = wallpaperStyleSheet();
  document.documentElement.appendChild(style);
  styleInjected = true;
}

/** 解析当前生效的家族（preview 优先于持久化选择）。 */
function activeFamily(
  section: ThemeSettings,
  mode: "light" | "dark",
  preview: ThemeFamily | null,
): ThemeFamily {
  if (preview !== null) return preview;
  const familyId = mode === "dark" ? section.activeDarkThemeId : section.activeLightThemeId;
  return resolveThemeFamily(familyId, section.customThemes);
}

// 背景图层幂等状态（参考 wallpaper.ts 的 applied 缓存）
let applied: { image: string; blurPx: number; factor: number } | null = null;
let decodedFor = "";
let decoded: HTMLImageElement | null = null;
let resizeBound = false;
const appliedThemeTokens = new Set<string>();

function layerCssSize(): { width: number; height: number } {
  const width = typeof window === "undefined" ? 0 : window.innerWidth;
  const height = typeof window === "undefined" ? 0 : window.innerHeight;
  return {
    width: Math.max(1, width + WALLPAPER_BLEED * 2),
    height: Math.max(1, height + WALLPAPER_BLEED * 2),
  };
}

function drawWallpaperBitmap(
  canvas: HTMLCanvasElement,
  image: HTMLImageElement,
  factor: number,
): void {
  const context = canvas.getContext("2d");
  if (context === null) return;
  const { width, height } = layerCssSize();
  const bitmapWidth = Math.max(1, Math.round(width / factor));
  const bitmapHeight = Math.max(1, Math.round(height / factor));
  const sourceWidth = image.naturalWidth || image.width;
  const sourceHeight = image.naturalHeight || image.height;
  if (!sourceWidth || !sourceHeight) return;
  canvas.width = bitmapWidth;
  canvas.height = bitmapHeight;
  const scale = Math.max(bitmapWidth / sourceWidth, bitmapHeight / sourceHeight);
  const drawWidth = sourceWidth * scale;
  const drawHeight = sourceHeight * scale;
  context.drawImage(
    image,
    (bitmapWidth - drawWidth) / 2,
    (bitmapHeight - drawHeight) / 2,
    drawWidth,
    drawHeight,
  );
}

function redrawApplied(): void {
  if (applied === null || typeof document === "undefined") return;
  const canvas = document.getElementById(WALLPAPER_INNER_ID);
  if (canvas === null) return;
  redrawWallpaper(canvas as HTMLCanvasElement, applied.image, applied.factor);
}

function redrawWallpaper(canvas: HTMLCanvasElement, image: string, factor: number): void {
  if (decodedFor === image && decoded !== null) {
    if (decoded.complete) drawWallpaperBitmap(canvas, decoded, factor);
    return;
  }
  if (typeof Image === "undefined") return;
  const next = new Image();
  decodedFor = image;
  decoded = next;
  next.onload = () => {
    if (decoded !== next || applied === null || applied.image !== image) return;
    redrawApplied();
  };
  next.src = image;
  if (next.complete) drawWallpaperBitmap(canvas, next, factor);
}

/** 绘制或移除固定背景图层（幂等，仅字段变化才碰 DOM）。 */
export function applyWallpaperLayer(extras: {
  wallpaperImage: string;
  wallpaperBlur: number;
  wallpaperPixelate: number;
}): void {
  if (typeof document === "undefined") return;
  const image = isWallpaperDataUrl(extras.wallpaperImage) ? extras.wallpaperImage : "";
  const root = document.documentElement;
  if (image.length === 0) {
    applied = null;
    decoded = null;
    decodedFor = "";
    root.removeAttribute(WALLPAPER_ATTR);
    document.getElementById(WALLPAPER_LAYER_ID)?.remove();
    root.style.removeProperty("--dsh-wallpaper-blur");
    if (resizeBound && typeof window !== "undefined") {
      window.removeEventListener("resize", redrawApplied);
      resizeBound = false;
    }
    return;
  }
  const blurPx = wallpaperBlurPx(extras.wallpaperBlur);
  const factor = wallpaperPixelFactor(extras.wallpaperPixelate);
  root.setAttribute(WALLPAPER_ATTR, "");
  ensureWallpaperStyle();
  let layer = document.getElementById(WALLPAPER_LAYER_ID);
  let canvas: HTMLCanvasElement;
  if (layer === null) {
    layer = document.createElement("div");
    layer.id = WALLPAPER_LAYER_ID;
    layer.setAttribute("aria-hidden", "true");
    canvas = document.createElement("canvas");
    canvas.id = WALLPAPER_INNER_ID;
    layer.appendChild(canvas);
    document.body.insertBefore(layer, document.body.firstChild);
    applied = null;
  } else {
    canvas = layer.firstElementChild as HTMLCanvasElement;
  }
  if (!resizeBound && typeof window !== "undefined") {
    window.addEventListener("resize", redrawApplied);
    resizeBound = true;
  }
  if (applied === null || applied.blurPx !== blurPx) {
    root.style.setProperty("--dsh-wallpaper-blur", `${blurPx}px`);
  }
  const imageChanged = applied === null || applied.image !== image;
  const factorChanged = applied === null || applied.factor !== factor;
  if (imageChanged) canvas.style.backgroundImage = `url("${image}")`;
  applied = { image, blurPx, factor };
  if (imageChanged || factorChanged) redrawWallpaper(canvas, image, factor);
}

/** 把 ui-theme 分节整体应用到 dsh web DOM。 */
export function applyThemeSection(
  section: ThemeSettings,
  systemDark: boolean,
  preview: ThemeFamily | null = null,
): void {
  if (typeof document === "undefined") return;
  const mode = resolveMode(section.preference, systemDark);
  const root = document.documentElement;
  const body = document.body;
  root.style.colorScheme = mode;
  body.toggleAttribute("data-ds-dark-theme", mode === "dark");

  const family = activeFamily(section, mode, preview);
  let tokens: ThemeTokens = {};
  for (const name of appliedThemeTokens) body.style.removeProperty(name);
  appliedThemeTokens.clear();
  if (family.id !== DEFAULT_FAMILY_ID) {
    tokens = deriveThemeTokens(family[mode]);
  }
  if (isWallpaperDataUrl(section.wallpaperImage)) {
    tokens = mixWallpaperSurfaces(tokens, mode, section.glassOpacity);
  }
  tokens["--dsw-alias-glass-opacity"] = `${clampWallpaperEffect(section.glassOpacity)}%`;
  for (const [name, value] of Object.entries(tokens)) {
    body.style.setProperty(name, value);
    appliedThemeTokens.add(name);
  }

  // 排版
  root.style.fontSize = `${section.fontSizeInterface || 16}px`;
  const sans = appearanceFontStack(section.fontFamilySans, DEFAULT_SANS_STACK);
  const code = appearanceFontStack(section.fontFamilyCode, DEFAULT_CODE_STACK);
  root.style.setProperty("--dsw-font-family", sans);
  root.style.setProperty("--ds-font-family-code", code);
  root.style.setProperty("--dsw-font-size-code", `${section.fontSizeCode || 13}px`);
  root.style.setProperty(
    "--dsw-font-family-composer",
    appearanceFontStack(section.fontFamilyComposer || "", sans),
  );
  root.style.setProperty(
    "--dsw-font-family-terminal",
    appearanceFontStack(section.fontFamilyTerminal || "", code),
  );
  applyTypographyTokens(
    body,
    section,
    sans,
    code,
    appearanceFontStack(section.fontFamilyComposer || "", sans),
    appearanceFontStack(section.fontFamilyTerminal || "", code),
  );

  applyWallpaperLayer({
    wallpaperImage: section.wallpaperImage,
    wallpaperBlur: section.wallpaperBlur,
    wallpaperPixelate: section.wallpaperPixelate,
  });
}
