# AGENTS.md — packages/plugins（dsh 插件容器）

dsh 插件的开发容器：**每个含 `dsh` 字段的子目录一个 dsh 插件**（cordis bundle），各自独立开发；
`client-kit/` 是共享源码目录，不是插件，构建时只内联进各插件的 client bundle。
插件不是 IPC 契约——契约在 `packages/contracts`。

## 现有插件

- `bridge/`（`@dsh-desktop/plugin-bridge`）— **桥接 Tauri 壳能力的双面插件**（host + client）：
  让 dsh web 内的插件/页面能调用 Tauri 壳能力。**桥接命令契约由本插件自持**
  （Rust 侧在 `apps/shell/src-tauri/capabilities/bridge.json` 声明），不并入
  `packages/contracts`——contracts 只负责 native 内容（边界见 `docs/plugin-tauri-boundary.md`）。
  host 侧注册 `dsh-desktop/health` 健康端点，不实现 dsh 业务；client 侧在 dsh WebUI 设置面板
  渲染“插件”、“桌面”与“外观”设置节，插件页按 group 管理本地与外部固定预设，并按 `dsh-desktop.mode` 提供兼容 / 高级桌面形态
  （详见 `./bridge/AGENTS.md`）
- `shortcuts/`（`@dsh-desktop/plugin-shortcuts`）— **全局快捷键设置插件**（client）：
  在 dsh WebUI 设置面板注册“快捷键”设置节，经 `window.__DSH_DESKTOP__.shortcuts`
  管理壳侧全局快捷键（注册 / 查询 / 移除 / 全部清理）；同时在 client 全局生命周期监听
  双击 Esc 停止当前对话：首次按键经 `shell.overlay` 显示“再次按 Esc 终止当前对话”，
  第二次按键调用 `ctx.sessions` 当前会话的 `cancel()`；另提供“常用动作”系统级
  全局快捷键预设（显示/隐藏主窗口、停止当前对话、新建对话），经 `onShortcut` 分发到
  对应动作。设置经 localStorage 持久化，职责和维护约定见 `./shortcuts/AGENTS.md`
- `projects/`（`@dsh-desktop/plugin-projects`）— **项目 host 插件**：
  host 面经 `webServer` 暴露 `/dsh-desktop/workspaces`（工作区列表）与
  `/dsh-desktop/workspaces/action`（置顶 / 取消置顶 / Finder / 工作树 / 改名 / 归档 /
  移除，走 dsh `workspaceRegistry` 服务；会话归档也走该 action 端点）
  及 `/dsh-desktop/sessions`（会话列表，含未加载的持久会话 / 工作区归属），供壳注入
  脚本渲染 dsh 侧边栏右键菜单；会话行使用独立的“归档聊天”菜单，与工作区菜单分开处理。
  端点只监听循环回环地址，动作均为工作区级最小切片。
- `reasoning/`（`@dsh-desktop/plugin-reasoning`）— **模型设置插件**（client）：
  在 dsh WebUI 设置面板注册“模型”设置节，按参考实现复刻官方模型管理页，并在
  pi-ai 模型行的自定义设置中提供 low / medium / high / xhigh / max 勾选；
  保存时经 settings RPC 写入 `reasoningEfforts`，让输入栏模型菜单直接切换推理等级。
  实现与维护约定见 `./reasoning/AGENTS.md`
- `client-kit/` — **非插件共享源码**：统一 `window.__DSH_DESKTOP__` 读取、client CSS
  注入与 tsdown 公共插件（CSS 内嵌 + `@deepseek-ai` 值导入 purity gate）；不含
  `package.json`，不会被 `yarn build:plugins` 装配

## 新增插件

1. 在 `packages/plugins/` 下建子目录（如 `custom-foo/`），包名 `@dsh-desktop/plugin-<name>`
2. 参照现有插件的结构：`package.json`（含 `dsh: { bundle: { patch: "./cordis.patch.yml" } }`、
   依赖 `@deepseek-ai/cordis`）、`tsconfig.json`、`cordis.patch.yml`、`src/index.ts`
3. 无需改根 workspaces——`packages/plugins/*` 已通配
4. `yarn install` 注册后，根 `yarn typecheck` 自动覆盖

需要同时替换/停用官方插件行时，在 `package.json` 的 `dshDesktop` 增加
`"overlay": true`，并把完整 overlay 内容放进 `cordis.patch.yml`；Rust 装配时会把该文件
原样写入 `embedded-plugins.patch.yml`，而不是只生成一行 `insert`。

## 插件格式要点（dsh bundle）

- `package.json` 声明 `dsh.bundle.patch` 指向 `cordis.patch.yml`
- `cordis.patch.yml` 是 YAML 配置层：`- insert:` 向 profile 插入插件行（`id` / `name`）
- 插件源码：`export const name` + `export function apply(ctx, config)`；事件名必须是
  cordis `Events` 接口里的键（骨架阶段不要注册未声明的事件）
- 安装：**内嵌装配（主交付路径）**——`yarn build:plugins` 编译打包进 Tauri resources，`tauri dev` 与 `tauri build` 前都会自动构建，桌面应用启动时自动复制进 `$DSH_HOME/profiles/node_modules/@dsh-desktop/<name>/` 并以 `--patch` overlay 挂载（见 `docs/plugin-tauri-boundary.md` §6）；开发期亦可 `dsh plugin --profile <active> add <包>` 单独安装
- 构建与装配：`yarn build:plugins`（根脚本）经 tsdown 编译各插件——host ESM 产出 `lib/index.js`，可选 client UMD 产出 `lib/client.js`——并把自包含 dist 包（`package.json` 白名单字段 + `cordis.patch.yml` + `lib/`）装配进 `apps/shell/src-tauri/resources/plugins/<name>/`；桌面应用启动时 Rust 侧自动装配进 dsh 的 profile 模块兜底目录并以 `--patch` overlay 挂载（机制见 `docs/plugin-tauri-boundary.md` §6）
  构建成功后自动清理 `resources/plugins` 中已不在源码里的旧插件目录；传入 `--home` 时还会清理 profile 中已不存在的 `@dsh-desktop/plugin-*` 包
- 开发热更新：`yarn dev` 会先跑 `yarn build:plugins`，再经 `scripts/dev.mjs` 同时启动 Vite 与 `scripts/watch-plugins.mjs`；watch 扫描 `packages/plugins` 下全部源码（含 `client-kit/` 与 tsdown 配置，排除 `lib/` 产物），变化后重新构建并热部署到 `$DSH_HOME/profiles/node_modules`（未设置 `DSH_HOME` 时用 `~/.dsh`），dsh 自带的 `dsh-client-hmr` 会轮询 client bundle 并在 Web UI 中热替换；开发模式壳侧还会在收到 rebuilt 帧后自动整页刷新，作为页面级兜底。也可以只运行 `yarn dev:plugins` 重建并热部署，便于在已启动的 dsh web 中单独调试插件
  - 适用范围：现有插件的 client bundle 内容变化可热更新；新增插件、修改 `package.json` 的 client 声明或 host 侧结构仍需重启 dsh
- 注意：dsh 从 Git 安装时跑 `prepare` 而非 `build`，TS 插件需自包含构建产物
  （见官方 publish 文档）

## dsh 官方开发规范（固定）

以下内容整理自 deepseek-harness `master` 官方文档（2026-08 核对），作为本包开发 dsh
插件的固定约束；上游演进后按链接源码/README 更新，而不是凭第三方教程改写。

本仓 client UI 设计范式另见 `./design.md`（含视觉 token、布局、组件、文案与落地检查）。

### 官方来源

- 插件开发入门 / 配置 / 工具 / 发布：[docs/user/develop/basic](https://github.com/deepseek-ai/deepseek-harness/tree/master/docs/user/develop/basic)
- 生命周期、事件与服务：[docs/user/develop/framework](https://github.com/deepseek-ai/deepseek-harness/tree/master/docs/user/develop/framework)
- 设置 seam：[docs/subsystems/settings.zh.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/settings.zh.md)
- client 模块装载：[docs/subsystems/client-modules.zh.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/client-modules.zh.md)
- 设置 slot / 工具 slot / 主题 / UI 原语 / schema form：
  [ui-settings](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-settings/README.zh.md)、
  [ui-tool](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-tool/README.zh.md)、
  [ui-theme](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-theme/README.zh.md)、
  [ui-primitives](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-primitives/README.zh.md)、
  [schema-form](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/schema-form/README.zh.md)
- 工具 UI 卡片：[docs/cookbook/adding-a-tool.zh.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cookbook/adding-a-tool.zh.md)

### 插件模型与生命周期

- 插件是导出 `name` 和 `apply(ctx, config)` 的 TypeScript 模块；需要其他服务时用
  `inject` 声明，`apply` 执行时这些服务保证已就绪。
- `ctx.on()`、`ctx.*.register()`、`ctx.effect()` 都是 effect，插件卸载时自动清理；
  手动资源必须放进 `ctx.effect()` 返回的 disposer。多个异步 disposer 会并发执行，
  顺序依赖必须放在同一个 effect 的 disposer 里。
- Cordis 事件模式分 `emit` / `bail` / `serial` / `waterfall`；`waterfall` 监听器
  必须调用 `next()`，否则会故意短路流水线。事件类型用 `declare module
  '@deepseek-ai/cordis' { interface Events { ... } }` 扩展，禁止注册未声明的键。
- 事件遵循 `namespace/action` 命名；`turn/*`、`step/*`、`tool/call`、`tool/result`、
  `compaction/*` 是持久化会话事件类型，不是同名 Cordis 事件，观察它们要走
  `session/event`。
- 插件配置必须导出同名 `Config` 类型与 Schemastery schema，默认值写在 schema 中；
  凡部署可能需要不同的参数都不许硬编码。schema 校验失败会让插件加载失败；
  配置变更触发整实例 HMR 替换，旧注册会随 effect 清理。
- 对外提供服务用 `Service` 基类，并用声明合并补 `Context` 类型；可选依赖在用到处
  用 `ctx.get()`，不要放进 `inject`。

### bundle 与配置层

- 组合包（bundle）与 profile 是两种 manifest，不会同时是两者：`dsh.bundle.patch`
  回答“这个包贡献什么”，`dsh.profile.bundles` 回答“这套配置由哪些包按什么顺序组成”。
- patch 生效顺序是 profile bundles → profile `cordis.patch.yml` → `$DSH_HOME` →
  `--patch` overlay；后应用层按行胜出，**覆盖整行 config 而不是深合并**。
- 从 Git 安装跑 `prepare` 而非 `build`，TS 插件必须提供自包含构建产物；发布 npm 或
  tarball 则交付预构建产物，不需要构建授权。

### client 面与 UI 接缝

- client 面经 `package.json` 的 `dsh.client`（`platform: "web"`）与
  `exports["./client"]` 进入 Web UI；host 扫描后组装 `window.__DSH_BOOT__` 图，
  经 `/plugins/<id>/client.js` 提供 bundle。
- UI 扩展使用 `@deepseek-ai/dsh-client-ui-slots` 的声明式 slot：
  先通过 `ctx.slots.inject()` 等待目标 slot 声明，再用 `ctx.slots.register()` 挂载；
  禁止绕过 slot 直接改 shell DOM、把组件硬塞进其他包页面。
- 重要 slot 归主：`sidebar.settings` 是设置入口；`settings.section` 是每个功能一页；
  `settings.plugins.tab` 是插件分区页；`settings.general.item` 是“通用”单行偏好；
  `tool.call.toolview` 是按 wire 工具名分发的原子工具卡片；`conversation.chat.node`
  是对话节点渲染。
- 外壳组件所有权在 `ui-layout` / `ui-sidebar` / `ui-workspace` /
  `ui-settings-general`，功能插件只贡献自己的 section、item 或 tab，不复制外壳。

### 设置规范

- host 侧设置 namespace 必须小写 kebab-case，用 `ctx.settings.register(ns, schema,
  options)` 注册；`base` 是组合层，`applies` 是 `live` / `restart`，跨字段约束放
  `validate()`，不放 schema。
- 用户层、组合 base、schema 默认值按“默认值 → base → user”解析；`replace` 才是删除/
  重置路径，`update` 只稀疏合并 user 层。
- 对外传输设置描述必须 `redactSecrets: true`，secret 用 path op 写回，绝不能把
  redacted descriptor 重建后 `replace`（会静默删除所有 secret）。
- client 面经 `@deepseek-ai/dsh-client-ui-settings` 的 `settingsScope.bind()` 读写，
  写入带 `expectedRevision`，避免覆盖并发变更；字段是否被用户覆盖按“是否出现在 user
  层”判断，不按值比较。
- bridge client 只注册“插件 / 桌面 / 外观”设置节；`ui-theme` 由上游
  `dsh-client-ui-theme` host 注册，client 只 bind，绝不重复注册；`shortcuts` 插件
  独立注册“快捷键”设置节；`projects` 插件不注册设置节，只提供侧边栏菜单端点。

### 主题与样式

- 自定义 UI 一律使用 `--dsw-*` token（基础样式表 + 语义别名层），不要直接写产品色值；
  token 样式表是颜色值的唯一权威来源。需要新色值先走语义 token，不做局部补色。
- 主题偏好由 `ctx.theme` 统一解析 `light` / `dark` / `system`，外部只消费不可变
  `ThemeSnapshot`；监听 `theme/change`，不要自己维护第二份主题状态。
- 自定义滚动表面遵守 `--dsh-scrollbar-*` 约定，高层面板在自己的容器上重新绑定 l2
  token；不要把 UI 外壳和第三方主题的 overlay 当成产品主题实现。

### 组件使用

- 基础 React 原子用 `@deepseek-ai/dsh-client-ui-primitives`：Button、Pill、Menu、
  Modal、Input、Toast、StateDot、Tooltip、TerminalBlock、DiffBlock、ReadBlock、
  SearchBlock、WebBlock、MarkdownText 等；这些原子零 Cordis，本地化文案走 label
  props，不要在组件内读全局 locale。
- 设置表单先用 `@deepseek-ai/dsh-client-schema-form` 的 schema 重水合/草稿/校验工具，
  再按功能画自己的控件；官方不提供通用 renderer，禁止自行用字符串拼表单。
- 新增工具展示走 `tool.call.toolview` keyed slot；工具本体仍返回一个规范 JSON 值，
  `output.render` 负责模型面文案，`presentCall` / `presentResult` 只负责纯函数 UI
  卡片，`presentationMeta` 负责可重放卡片数据。模型结果里不放只为 UI 服务的格式。
- client bundle 只依赖平台模块；跨插件值 import 会被本仓 tsdown purity gate 拒绝，
  协作走 cordis services，不是互相 import 组件。`client-kit` 是源码级 helper，
  构建时内联进各 client bundle，不构成插件间值依赖。

## 边界

- 本包不做业务：只负责插件自身逻辑；桥接命令契约由 `bridge/` 插件自持（不并入 `packages/contracts`，contracts 只负责 native）
- dsh 内插件清单 / 安装 UI 仍是 dsh 现成的 cordis 插件（`dsh-host-plugin-inventory` 等）；
  bridge 只补充壳侧远程插件预设（`packages/external-plugins` 外部目录 + config 本地分组）、受管 add/remove/update 与已安装依赖列表，不接管 dsh 内部清单
- `@deepseek-ai/cordis` 等新版本发布不足 1 天会被 yarn 的 npmMinimalAgeGate 隔离；
  已在仓库 `.yarnrc.yml` 对 `@deepseek-ai/*` 定向放行
