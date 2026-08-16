# AGENTS.md - packages/plugins/shortcuts（快捷键统一管理插件）

`@dsh-desktop/plugin-shortcuts`：dsh WebUI 内快捷键的统一管理插件。后续新增或修改
dsh 侧快捷键优先集中在这里，不散落到 bridge、projects 或其他业务插件。

## 职责

- 在 dsh WebUI 设置面板注册“快捷键”设置节，管理壳侧全局快捷键：
  - 查看已注册全局快捷键
  - 添加 / 移除 / 全部移除全局快捷键
  - 经 `window.__DSH_DESKTOP__.shortcuts` 调用 `register_shortcut` /
    `unregister_shortcut` / `get_shortcuts` / `unregister_all_shortcuts`
- 维护 dsh 页面内快捷键，当前实现“双击 Esc 停止当前对话”：
  - 第一次 Esc：先检测当前会话 running，只有 running 才显示“再次按 Esc 终止当前对话”
  - 第二次 Esc：调用 `ctx.sessions` 当前会话的 `cancel()` 停止当前对话
  - 超过判定窗口：提示消失，不停止
- 维护“常用动作”系统级全局快捷键预设，把动作绑定到全局快捷键并在按下时执行：
  - 显示 / 隐藏主窗口：经桥接 `windowAction("toggle-visible")`（壳侧 `window_action`）
  - 停止当前对话：同双击 Esc，`ctx.sessions` 当前会话 `cancel()`
  - 新建对话：`ctx.workspaces.startSession()`（复用空白会话或新建，无工作区时进新建视图）
  - 预设经 `bridge.onShortcut` 分发：一次订阅把快捷键字符串映射回对应动作；
    默认全部关闭（避免抢占系统级组合键），快捷键字符串可编辑，启用后走壳侧注册并持久化
- 通过 dsh settings 的 `dsh-desktop.shortcuts` namespace 持久化快捷键设置，让行为可配置并跨页面刷新保留。

## 双面结构

| 面 | 文件 | 职责 |
| --- | --- | --- |
| host | `src/index.ts` | 注册 `dsh-desktop.shortcuts` settings namespace |
| shared | `src/shared/settings.ts` | client 共用的设置类型、默认值（含预设）与归一化函数 |
| client | `src/client.tsx` | 快捷键设置节、全局 keydown 监听、常用动作预设 UI 与分发、停止对话动作、`shell.overlay` Toast 反馈 |
| client runtime | `src/client/runtime.ts` | 桥接对象最小切片（含 `onShortcut` / `windowAction`）、设置 scope 最小切片 |
| client UI | `src/client/*` | 设置页布局、样式、Toast 与快捷键管理组件 |

## 配置持久化

host 注册 `dsh-desktop.shortcuts` settings namespace；client 通过 `settingsScope.bind` 读写。

| 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `doubleEscapeStopEnabled` | boolean | `true` | 是否启用双击 Esc 停止当前对话 |
| `doubleEscapeStopTimeoutMs` | number | `1000` | 两次 Esc 的判定窗口，200–5000ms |
| `presets` | Record<PresetId, {enabled, shortcut}> | 全部关闭 | 常用动作预设：开关与绑定的全局快捷键字符串 |

预设 id 与默认快捷键（可编辑）：`toggleWindow` → `CmdOrCtrl+Shift+Space`、
`stopConversation` → `CmdOrCtrl+Shift+.`、`newConversation` → `CmdOrCtrl+Shift+N`。
预设以 `presets` 字段稀疏更新，写回时与现有值深合并，避免覆盖其他预设。

旧 `localStorage` 数据仅在首次装载时一次性迁移，随后删除旧 key。

## 行为约定

- “双击 Esc”是 dsh 页面内快捷键，不是系统级全局快捷键；不得注册全局 Esc。
- 提示与 `cancel()` 都只在当前会话 `running` 时可触发，空闲会话不提示、不停止。
- 单次 Esc 不拦截页面现有取消/关闭行为。
- Toast 经 `shell.overlay` 挂载，文案走 locale 字典。
- 快捷键注册/注销统一通过 bridge 暴露的最小切片，不直接触碰 Tauri 内部。
- 常用动作预设默认关闭，避免默认抢占系统级组合键；启用前校验与其他预设/已注册快捷键不冲突。
- 预设注册经壳侧持久化（`config.json.shortcuts`）：重启后壳侧自动恢复注册，插件只负责
  `onShortcut` 分发；设置页装载时对“已启用但未注册”的预设补注册，不注销非预设快捷键。
- “全部移除”会同时停用所有常用动作预设，避免被预设重新补齐注册。

## 维护约定

- 新增 dsh 侧快捷键时，先在本插件登记，避免快捷键逻辑散落到其他插件。
- 需要持久化设置时，扩展 `src/shared/settings.ts` 与 host settings schema，并保持 client `settingsScope` 读写一致。
- 修改桥接命令或事件时，同步 `packages/plugins/bridge/AGENTS.md`、
  `apps/shell/src-tauri/capabilities/shortcuts.json`、`bridge.json` 与本文档。
- `lib/` 为构建产物，不手改；修改后运行 `yarn typecheck`、`yarn lint`、
  `yarn build:plugins`。
