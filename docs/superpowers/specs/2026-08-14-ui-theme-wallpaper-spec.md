# 主题与背景图（参考 Deepseek-Harness-Desktop）设计说明

> 对应实施计划：`docs/superpowers/plans/2026-08-14-ui-theme-wallpaper.md`
> 参考实现：https://github.com/ChisaAlter/Deepseek-Harness-Desktop（Electron 桌面壳，在其 vendored
> deepseek-harness 里自建了完整的主题与背景图功能）

## 1. 背景与现状分析

### 1.1 参考仓库做了什么

参考仓库（Electron）在 **vendored deepseek-harness** 里自建了一套 `packages/client/ui-theme`：

- **外观设置节**（设置 → 外观）：主题偏好（浅色/深色/跟随系统）、**主题库**（7 个内置主题家族，
  每族浅色/深色两半，点哪半用哪半）、**自定义主题**（创建/复制/编辑/导入导出 JSON）、
  **背景图**（选图 → data URL → 铺在整个界面后面，毛玻璃/像素化/玻璃透明度滑块）、**排版**
  （界面/代码字号与字体族）。
- **持久化**：写入 `$DSH_HOME/settings.yaml` 的 `ui-theme` 分节。
- **应用**：把种子色推导成 `--dsw-alias-*` CSS 变量写到 `body`，切换 `body[data-ds-dark-theme]`；
  背景图用一个 fixed 图层 + canvas（cover 裁剪、blur、像素化），并把主要表面 token 混成半透明
  （color-mix）让背景透出来。
- **壳侧跟随**：Electron 主进程读同一份 `settings.yaml`（迷你 YAML 解析），解析出少量 token
  （`--bg/--fg/--muted/--accent/--field/--line/--button-fg`），用于启动页（boot 页）与窗口背景。

### 1.2 本仓库（dsh-desktop，Tauri）的现状

- 不 vendored harness：启动已安装的 `dsh web`（当前 `@deepseek-ai/dsh@0.1.0-rc.6`）子进程。
- **上游 rc.6 已内置** `@deepseek-ai/dsh-client-ui-theme`（web profile 已装配 `ui-theme` 行）：
  - host 面已注册 `ui-theme` settings 命名空间（写入 `settings.yaml`）；
  - 已提供 `--dsw-*` token 样式表（`base.css` / `design-platform.css` 等），暗色切换靠
    `body[data-ds-dark-theme]`，别名层变量为 `--dsw-alias-*`；
  - 设置 → 通用 里只有一个**紧凑的“外观”偏好行**（浅色/深色/跟随系统三个 cube）；
  - **没有**主题库、自定义主题、背景图/毛玻璃/像素化/玻璃透明度、排版字号。
- 因此真正缺失、需要本计划补齐的，正是参考仓库自建的那部分：**主题库 + 自定义主题 + 背景图 +
  玻璃透明度 + 排版**，以及**壳侧（启动页 + 窗口背景）跟随**。

### 1.3 分层归属（遵循 docs/plugin-tauri-boundary.md）

| 能力 | 归属 | 载体 |
| --- | --- | --- |
| 外观设置节 UI + 主题/背景应用（dsh web 内） | dsh 插件域 | `packages/plugins/bridge` 的 client 面（新增“外观”设置节） |
| `ui-theme` 命名空间注册（settings.yaml 持久化） | dsh 插件域（**上游已做，本计划不重复**） | 上游 `dsh-client-ui-theme` host |
| 壳侧读取 settings.yaml、启动页/窗口背景跟随 | Tauri 壳域 | Rust 后端 + 启动页（新增 `get_ui_theme` 命令 + `dsh-ui-theme` 事件，经 `packages/contracts`） |
| 背景图选图 | dsh web 内 | `<input type="file">`（webview 原生文件选择），不走壳桥接 |

## 2. 目标

1. 在 dsh WebUI 设置面板新增“外观”设置节：主题偏好、主题库（内置 7 家族 + 自定义主题
   创建/复制/编辑/导入导出）、背景图（毛玻璃/像素化/玻璃透明度）、排版（字号与字体族）。
2. 主题与背景在 dsh web 内实时生效：推导 `--dsw-alias-*` 变量、切换 `body[data-ds-dark-theme]`、
   注入背景图层（blur + pixelate）、玻璃表面半透明。
3. 壳侧跟随：Rust 读 `$DSH_HOME/settings.yaml` 的 `ui-theme` 分节，解析 token；启动页与
   窗口背景跟随所选主题；设置变化时实时更新。

## 3. 非目标（上游已实现，不在本计划范围）

- `ui-theme` 命名空间的注册（上游 host 已注册；bridge 只 bind，不注册）。
- `--dsw-*` token 样式表与 `body[data-ds-dark-theme]` 基础调色板（上游已提供）。
- 上游“外观”偏好行（保留不动；本计划的完整外观节是并列的独立设置节）。

## 4. 数据模型（settings.yaml `ui-theme` 分节）

```yaml
ui-theme:
  preference: system            # system | light | dark
  activeLightThemeId: deepseek  # 浅色半的主题家族 id
  activeDarkThemeId: midnight   # 深色半的主题家族 id
  customThemes: []              # [{ id, name, origin: custom, light: {…seeds}, dark: {…seeds} }]
  glassOpacity: 80              # 40–100，玻璃表面不透明度（%）
  wallpaperImage: ''            # data:image/…;base64,…，空串=无背景
  wallpaperBlur: 0              # 0–100，毛玻璃
  wallpaperPixelate: 0          # 0–100，像素化
  fontFamilySans: ''            # 界面字体族（空=默认栈）
  fontFamilyCode: ''            # 等宽字体族
  fontSizeInterface: 16         # 界面字号 px，12–22
  fontSizeCode: 13              # 代码字号 px，10–20
  fontFamilyComposer: ''        # 输入框字体族（空=跟随界面栈）
  fontFamilyTerminal: ''        # 终端字体族（空=跟随等宽栈）
```

Seeds（每个主题家族每半一份）：

```ts
interface ThemeSeeds {
  accent: string       // 强调色（发送按钮/用户气泡/侧栏选中）
  background: string   // 画布背景
  foreground: string   // 主文本
  contrast: number     // 0–100 对比度（默认 46）
  overrides?: Record<string, string>  // 精确 --dsw-alias-* 覆盖（可选）
}
interface ThemeFamily {
  id: string; name: string; origin: 'builtin' | 'custom'
  light: ThemeSeeds; dark: ThemeSeeds
}
```

内置 7 家族（id / 名称，浅色种子 accent/background/foreground）：

| id | 名称 | 浅 accent/bg/fg | 深 accent/bg/fg |
| --- | --- | --- | --- |
| deepseek | DeepSeek | #4176e6 / #ffffff / #0f1115 | #6ea8ff / #151517 / #f5f5f5 |
| midnight | 午夜 | #3b6fd4 / #f3f6fb / #1a1f2b | #6ea8ff / #0b0d12 / #e8eef9 |
| celadon | 青瓷 | #0f766e / #f3faf7 / #10211c | #3dd6b5 / #071411 / #e7f6f1 |
| violet | 暮紫 | #7c3aed / #f7f3fc / #1c1524 | #c4a1ff / #120e18 / #f3eefc |
| amber | 琥珀 | #b45309 / #fbf6ee / #1c1915 | #e2b15c / #14100b / #f6efe4 |
| paper | 宣纸 | #0f766e / #f3efe6 / #1c1915 | #5eead4 / #1a1712 / #f6efe4 |
| contrast | 对比 | #111111 / #ffffff / #050505 | #ffffff / #050505 / #f5f5f5 |

> `deepseek` 家族（产品默认）不推导 token（空 token 字典，样式表保持权威）；其他家族按 §5 推导。

## 5. Token 推导（移植自参考 derive.ts）

输入 seeds（accent/background/foreground/contrast），输出 `--dsw-alias-*` 变量字典，写到 `body`
内联样式（覆盖样式表值）。核心：

- 相对亮度/对比度（WCAG）：`getRelativeLuminance` / `getContrastRatio`
- 混色：`mixColors(a, b, t)`；透明化：`withAlpha(color, a)` → `rgb(r g b / a)`
- 由背景亮度判断明暗：`isDark = L(bg) < L(fg)`
- 派生：`wash = 0.14+cf*0.10`(dark) / `0.10+cf*0.08`(light)；`washStrong`、`card`、`overlay`、
  `layer2`、`mutedForeground`、`border`、`input`、`sidebar`、`accentHover`、`onAccent`
  （`pickReadableText` 从候选里挑对比最高的文本色）
- 输出变量集合（与参考一致）：`--dsw-alias-bg-base`、`-bg-layer-1`、`-bg-layer-2`、`-bg-overlay`、
  `-label-primary`、`-label-secondary`、`-label-primary-foreground`、`-brand-primary`、
  `-brand-primary-invert`、`-brand-text`、`-brand-primary-new-colorprimary-new-color`、
  `-button-primary-fill`、`-button-primary-hover`、`-button-info-fill`、`-button-info-hover`、
  `-state-business-primary`、`-state-business-tertiary`、`-border-l1`、`-border-l2`、
  `-state-error-primary`、`-state-success-primary`、`-state-warn-primary`、`-specific-bubble`、
  `-specific-bubble-highlight`、`-specific-sidebar-fill`、`-specific-sidebar-nav-item-active`、
  `-specific-sidebar-nav-item-active-accent`、`-interactive-bg-hover-accent`；
  再叠加 `seeds.overrides`。

## 6. 背景图层（移植自参考 wallpaper.ts / wallpaper.css）

- 常量：`WALLPAPER_LAYER_ID='dsh-wallpaper'`、`WALLPAPER_INNER_ID='dsh-wallpaper-inner'`、
  `WALLPAPER_ATTR='data-dsh-wallpaper'`、`WALLPAPER_BLEED=48`、`MAX_WALLPAPER_EDGE=1920`、
  `MAX_WALLPAPER_DATA_URL_CHARS=1_800_000`。
- 选图：`<input type="file" accept="image/png,image/jpeg,image/webp,image/gif">` →
  `readFileAsDataUrl` → `downscaleWallpaper`（Image + canvas 等比缩到最长边 1920，输出 JPEG
  data URL）→ `encodeWallpaperFile`。
- 效果映射：`wallpaperBlurPx(percent)=percent/100*40`（0–40px）；`wallpaperPixelFactor(percent)=1+percent/100*19`。
- 图层：`#dsh-wallpaper` fixed 覆盖全屏（`z-index:0; pointer-events:none`），内层 canvas 绘制
  “cssSize/factor 分辨率、cover 裁剪”的位图，CSS `image-rendering:pixelated` 拉伸回原尺寸实现
  像素化；`filter: blur(var(--dsh-wallpaper-blur))`。激活时 `html[data-dsh-wallpaper]`，`html/body/#root`
  背景透明，`#root` 置 `position:relative; z-index:1`。
- 玻璃：`mixWallpaperSurfaces(tokens, mode, solidity)` 把 `--dsw-alias-bg-base`（画布，最透）、
  `-bg-layer-1/-bg-layer-2`（提升表面）、`-specific-sidebar-fill`（侧栏，居中）改为
  `color-mix(in srgb, <solid> <percent>%, transparent)`；`wallpaperCanvasSolidity` 分段映射
  40→15、80→45、100→100。另设 `--dsw-alias-glass-opacity`。
- 幂等：模块级缓存 `applied`，只有字段变化才碰 DOM；resize 时重绘。

## 7. 排版（移植自参考 appearance-apply.ts）

- 变量：`--dsw-font-family`（界面栈）、`--ds-font-family-code`（等宽栈）、
  `--dsw-font-size-code`、`--dsw-font-family-composer`、`--dsw-font-family-terminal`；
  `documentElement.style.fontSize = fontSizeInterface + 'px'`。
- `quoteFontFamilyName`：裸 ident 不加引号，否则加引号；`appearanceFontStack(custom, defaultStack)`
  把自定义栈前置到默认栈。

## 8. 壳侧跟随（移植自参考 shared/themes.js）

- Rust 新增 `theme.rs`：读取 `$DSH_HOME/settings.yaml`（新增 `serde_yaml` 依赖）的 `ui-theme`
  分节；内置家族种子 + `mixHex` 推导少量 token：`bg/fg/muted/accent/field/line/buttonFg`；
  `resolve_mode(preference, systemDark)`；`activeLightThemeId/activeDarkThemeId` 选中家族；
  未知 id 回落 deepseek。
- 新 IPC：`get_ui_theme`（命令，返回 `UiThemeSnapshot`：tokens + raw 分节）与 `dsh-ui-theme`
  （事件，settings.yaml 变化或窗口主题变化时 emit）。契约进 `packages/contracts`。
- 启动页：初始 `invoke('get_ui_theme')` + 监听 `dsh-ui-theme`，把 tokens 映射到 `style.css`
  的 CSS 变量（`--bg/--ink/--muted/--line/--accent/--accent-ink`）。
- 窗口背景：Rust 在主题变化时 `window.set_background_color`。

## 9. 验证路径

- 纯逻辑：bridge 插件 `src/shared/theme.ts` 由 tsdown 额外产出 `lib/shared.js`，
  `node packages/plugins/bridge/tests/theme.test.mjs`（node:test，仓库既有零依赖测试模式）。
- Rust：`cargo test`（theme.rs 解析/推导单测）。
- 集成：`yarn typecheck` + `yarn build:plugins` + `yarn build:web` + `cargo fmt --check` +
  `cargo clippy`；`yarn dev` 起应用，在 dsh WebUI 设置 → 外观 手动验证全部控件与实时生效，
  以及启动页/窗口背景跟随。

## 10. 无边框窗口 + 自绘标题栏（参考仓库 chrome.js / harness-chrome-inject.js / window-controls）

### 10.1 参考仓库做法

- Electron 窗口 `frame: false`；启动页（boot）自带标题栏（`window-controls.css/js`），
  拖动用 `-webkit-app-region: drag`，窗口控制按钮经 IPC 调 minimize/maximize/close。
- dsh web 页面注入 `harness-chrome-inject.js`：找到顶栏（含 session log 的 header）作为拖动区、
  右上角放最小化/最大化/关闭按钮、按页面背景色适配按钮颜色、最大化/还原图标随窗口状态切换。

### 10.2 本仓库（Tauri）落地要点

| 项 | 做法 |
| --- | --- |
| 无边框 | `tauri.conf.json` main 窗口 `decorations: false`（关闭到托盘行为不变） |
| 拖动 | `data-tauri-drag-region="deep"`（Tauri 注入的 drag.js 处理；可点击元素自动阻断拖拽；双击自动最大化，无需 ACL） |
| 窗口控制命令 | 新增 `window_action`（minimize / maximize 切换 / close），进 `build.rs` commands 与 `default.json` + `bridge.json` 白名单 |
| 窗口状态事件 | 新增 `dsh-window-state`（`{ maximized }`），Resized / setup / 就绪导航时 emit |
| 桥接 | `BRIDGE_SCRIPT` 暴露 `windowAction(action)` 与 `onWindowState(cb)` |
| 启动页标题栏 | `index.html` 顶部 header（`data-tauri-drag-region="deep"`）+ 三按钮；样式用 `--theme-*`（跟随主题）；非 Tauri 环境隐藏 |
| dsh web 标题栏 | `HARNESS_CHROME_SCRIPT`（Rust 常量，on_page_load 注入）：顶栏/拖动条设 `data-tauri-drag-region="deep"`、右上角控制按钮、按钮色读 `--dsw-alias-label-primary` |

