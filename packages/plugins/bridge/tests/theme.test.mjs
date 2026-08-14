import assert from "node:assert/strict";
import test from "node:test";
import {
  deriveThemeTokens,
  duplicateThemeFamily,
  ensureUniqueThemeId,
  isWallpaperDataUrl,
  listThemeFamilies,
  mixWallpaperSurfaces,
  normalizeImportedThemeFamily,
  resolveMode,
  resolveThemeFamily,
  resolveThemeSettings,
  serializeThemeFamily,
  slugifyThemeId,
  wallpaperBlurPx,
  wallpaperCanvasSolidity,
  wallpaperPixelFactor,
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
  assert.equal(
    deriveThemeTokens(resolveThemeFamily("deepseek", []).dark)["--dsw-alias-bg-base"],
    undefined,
  );
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
  const base = {
    id: "violet",
    name: "暮紫",
    origin: "custom",
    light: { accent: "#7c3aed", background: "#fff", foreground: "#000", contrast: 46 },
    dark: { accent: "#c4a1ff", background: "#000", foreground: "#fff", contrast: 46 },
  };
  const dup = duplicateThemeFamily(base, new Set(["violet"]));
  assert.equal(dup.id, "violet-copy");
  assert.equal(dup.origin, "custom");
  const imp = normalizeImportedThemeFamily(
    { ...base, id: "violet" },
    new Set(["violet", "violet-copy"]),
  );
  assert.equal(imp.id, "violet-2");
  assert.equal(ensureUniqueThemeId("deepseek", new Set(["deepseek"])), "deepseek-2");
  assert.equal(slugifyThemeId("  我的 主题! "), "我的-主题");
  assert.equal(JSON.parse(serializeThemeFamily(dup)).name, "暮紫 Copy");
});
