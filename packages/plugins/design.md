# dsh 插件设计语言范式

> 适用对象：`packages/plugins` 下各 dsh 插件 client 面。
> 依据：deepseek-harness `master` 官方 README 与源码（2026-08 核对），上游演进后按官方链接更新。
> 本文件独立可引用；插件架构、slot 与桥接边界仍以 `packages/plugins/AGENTS.md` 为准。

## 引用方式

其他文档引用本文件时，使用相对当前文档的路径 `design.md` / `../../packages/plugins/design.md`，
并尽量带稳定锚点：

| 主题 | 稳定锚点 |
| --- | --- |
| 总范式 | `#design-language` |
| 设计原则 | `#principles` |
| 视觉 token | `#tokens` |
| 布局系统 | `#layout` |
| 组件与状态 | `#components` |
| 文案与内容 | `#copy` |
| 落地检查 | `#checklist` |

示例：`[dsh 插件设计语言](../plugins/design.md#design-language)`。

## 官方依据

- 外壳与布局：[ui-layout](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-layout/README.zh.md)、
  [ui-sidebar](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-sidebar/README.zh.md)、
  [ui-workspace](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-workspace/README.zh.md)、
  [ui-settings-general](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-settings-general/README.zh.md)
- 主题与样式：[ui-theme](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-theme/README.zh.md)、
  [ui-theme styles](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/client/ui-theme/src/styles)
- 组件与表单：[ui-primitives](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-primitives/README.zh.md)、
  [schema-form](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/schema-form/README.zh.md)、
  [ui-tool](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-tool/README.zh.md)
- 工具 UI 卡片：[adding-a-tool.zh.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cookbook/adding-a-tool.zh.md)

<a id="design-language"></a>
## 设计语言范式（官方）

dsh 的视觉基调是**低装饰、高密度、可扫描、语义可换肤**的 AI 工作台，不是营销页面：
外壳 chrome 归上游 `ui-*` 包所有，插件只贡献内容；所有颜色、字号、圆角、阴影与动效都从
token / 平台原子继承，不自行定义第二套设计系统。

<a id="principles"></a>
### 设计原则

- **内容优先**：插件面优先呈现可读标题、说明、当前值与操作，不设置 hero、插画、渐变装饰
  或解释产品机制的营销文案。
- **外壳只读**：三栏 AppFrame、侧边栏、设置导航、对话与详情面板均由上游持有；插件只通过
  slot 贡献 section / tab / item / toolview，不复制外壳、不注入全局样式。
- **语义优先于数值**：颜色、文字层级、交互状态和浮层都用语义 token，不用产品色值；需要新
  色值先走语义 token，不做局部补色。
- **稳定布局**：页面/卡片/行使用可预测的栅格、截断和容器高度，hover 只揭示操作，不改变
  主要内容的位置。

<a id="tokens"></a>
### 视觉 token

- 颜色分两层：`--dsw-static-*` 是静态色板，`--dsw-alias-*` 是主题解析后的语义别名。
  插件消费方只使用 `--dsw-alias-*`；`--dsw-static-*` 只允许在少数需要精确阶梯色的平台原语
  内部出现（如运行中状态点）。
- 常用语义组：背景 `bg-base` / `bg-layer-1|2|3`，边框 `border-l1|l2|l3|l4`，文字
  `label-primary` / `label-secondary` / `label-tertiary` / `label-dimmed` /
  `label-caption`，品牌 `brand-primary`，按钮 `button-primary-*`，交互
  `interactive-bg-hover|active`，状态 `state-success|warn|error|business-*`。
- 主题只由 `ctx.theme` 解析 `light` / `dark` / `system` 并发布 `ThemeSnapshot`；插件监听
  `theme/change`，不维护第二份主题状态。`body[data-ds-dark-theme]` 与内联 alias token 由
  ui-layout 呈现器负责。
- 排版走 `--dsw-font-*` token：正文/控件采用官方原语的 14px / 22px 行高节奏；提示与次要值
  可用 12–13px；设置节标题约 15–16px / 24px；代码与终端走 `--ds-font-family-code` 与
  `--dsw-font-size-code`，不要另造英文/等宽字体栈。
- 动效使用 `--ds-ease-in-out`、`--ds-transition-duration-fast` / `--ds-transition-duration` /
  `--ds-transition-duration-slow`（0.1 / 0.2 / 0.3s）；
  状态变化、浮层、hover 才使用过渡，装饰性动画一律不要；尊重 `prefers-reduced-motion`。
- 滚动条遵守 `--dsh-scrollbar-*` 约定：`--dsh-scrollbar-width` 为 8px，基础表面绑定 l1，
  菜单/浮层/对话框在自己的容器上重新绑定 l2，不做自定义 WebKit 皮肤。

<a id="layout"></a>
### 布局系统

- 全应用是 `sidebar / conversation / details` 三栏 AppFrame：侧边栏可折叠但保留 56px 控制栏，
  详情栏可拖宽并自动关闭。插件不重建这套几何，也不把自己的内容覆盖到 shell 外。
- 设置外壳是官方 modal：800px 宽、nav rail 约 188px + 内容列，内容区滚动、单页高度稳定。
  插件 `settings.section` 的正文仍按“页面块”组织，内容列内 max-width 720、块间距 24–28px。
- 页面块结构固定为 `heading`（名词标题）→ `hint`（一句设置结果/影响）→ 控件区 →
  `value`（当前值/状态）；块内间距约 12px，动作组 8px，行与行之间用 token 层级而非重复边框。
- 可扫描列表/网格优先：固定导航或筛选 + 主内容，工具动作放工具栏；网格卡片用
  `repeat(auto-fill, minmax(200px, 1fr))`，避免宽屏大卡片空转。
- 卡片必须稳定：固定高度或 min-height、标题单行省略、简介最多三行、操作在 hover 才展开；
  禁止卡片叠卡片，禁止把页面分区包成一张大卡片。
- 工具结果走官方卡片模型：TerminalBlock / ReadBlock / DiffBlock / SearchBlock / WebBlock，
  长内容横向滚动或头尾切片（默认 16 行上限），不软换行、不把截断结果伪装成完整结果。

<a id="components"></a>
### 组件与状态

- 基础交互一律用 `@deepseek-ai/dsh-client-ui-primitives`：Button、Input、Pill、Menu、Modal、
  Tooltip、Toast、StateDot 等；自定义控件只在平台原语不覆盖时才做，且必须复用 token 几何
  与交互状态。
- 按钮层级固定：确认/保存 primary，选择/打开 outline，低频/导航 ghost；同一区块操作收敛到
  块头或 hover，避免每一行堆满按钮。官方几何：md 高 36px、胶囊 18px、14px/22px 文本；sm
  高 28px、14px 圆角、12px/18px 文本。
- 输入框高 32px、8px 圆角、1px `border-l2`、聚焦用品牌边框或 2px outline；禁用态统一
  opacity 0.4，不做一套禁用色。
- 状态语义固定：成功 `state-success-*`、警告 `state-warn-*`、错误 `state-error-*`、运行中
  使用官方 StateDot；状态点是装饰符号时必须带视觉隐藏文本或 `aria-label`。
- 浮层/菜单使用平台 Menu / Modal，不用自建 popover；菜单行 hover 用
  `interactive-bg-hover`，危险项用 error token，浮层在自己的容器上重新绑定滚动条 l2。
- 焦点可见性：所有键盘可达元素提供清晰 focus-visible，通常是 2px 品牌 outline + offset；
  不允许 `outline: none` 后不补等价焦点样式。

<a id="copy"></a>
### 文案与内容

- 标题是名词短语，说明讲设置结果；不要解释 UI 结构、快捷键、组件名或内部机制。
- 本地化文案经 label props / locale 字典传入，zero-cordis 原子组件内不读全局 locale。
- 工具主体返回规范 JSON，UI 文案与格式化只存在于 `presentCall` / `presentResult` /
  `presentationMeta`；模型结果和卡片数据里不放只为 UI 服务的格式。

<a id="checklist"></a>
### 落地检查

- CSS 无硬编码产品色值/字体栈/圆角阶梯；所有颜色、状态、交互、浮层、滚动条都能随主题切换。
- 不使用原生 checkbox/range/menu 冒充平台组件；需要自定义时，几何、focus、disabled、hover
  与语义 token 对齐官方原语。
- 页面/卡片/表单在窄屏到宽屏下无溢出、无文字遮挡、无卡片套卡片；长文本按官方截断规则。
- 新 UI 不触碰 `#root`、`body`、`.AppFrame` 或第三方主题 overlay；外壳 DOM 与全局 token
  一律只读。
