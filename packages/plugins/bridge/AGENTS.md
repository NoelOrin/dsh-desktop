# AGENTS.md - packages/plugins/bridge（桥接 Tauri 壳能力的 dsh 插件）

`@dsh-desktop/plugin-bridge`：让 dsh web 内的插件/页面能调用 Tauri 壳能力的**双面 dsh 插件**
（cordis bundle）。**桥接命令契约由本插件自持**，Rust 侧在
`apps/shell/src-tauri/capabilities/bridge.json` 声明实现与权限；不并入
`packages/contracts`——contracts 只负责 native 内容（边界见 `docs/plugin-tauri-boundary.md`）。

## 双面结构

| 面 | 文件 | 职责 |
| --- | --- | --- |
| host | `src/index.ts` | 插件入口（`export const name` + `apply(ctx)`）；定义 `DshDesktopBridge` 类型；注册 `dsh-desktop/health` 健康端点，不实现 dsh 业务 |
| client | `src/client.tsx` | 在 dsh WebUI 设置面板注册“桌面”设置节（状态 / 配置 / 工具 / 开机自启与启动模式）与“外观”设置节（主题偏好 / 主题库 / 背景图 / 玻璃透明度 / 自定义主题 / 排版）；`ui-theme` 命名空间只 bind 不注册，快照经 `createThemeStore` 防抖写回，变化时 `applyThemeSection` 实时应用 |
| shared | `src/shared/theme.ts` | 主题家族、token 推导、背景/玻璃/排版边界与默认值 |
| client UI | `src/client/*` | 桌面/外观设置节组件（`DesktopPanel.tsx`、`AppearanceSection.tsx` 等）、桌面壳形态（`desktop-shell.ts` / `desktop-shell.module.css`）、共享控件（`ui/controls.*`）、运行时类型切片（`runtime.ts`）与主题应用逻辑；桥接读取与样式注入统一走 `../../../client-kit/inject.ts` |

`package.json` 通过 `dsh.client`（platform web）声明 client 面并导出 `./client`；
client 面依赖 `@deepseek-ai/dsh-client-ui-settings` 等 dsh client 生态（peer 声明）。

## 桥接对象（window.__DSH_DESKTOP__）

壳侧 `BRIDGE_SCRIPT` 注入到 dsh web 页面的受控 API。当前最小切片：

| 成员 | 壳命令 / 事件 | 说明 |
| --- | --- | --- |
| `platform` | - | 当前平台字符串 |
| `notify(title, body)` | `plugin:notification\|notify` | 发原生通知 |
| `clipboard.readText()` / `writeText(text)` | `plugin:clipboard-manager` | 剪贴板读写 |
| `dialog.openFile(opts)` / `saveFile(opts)` | `plugin:dialog` | 文件选择 / 保存对话框 |
| `openExternal(target)` | `open_external` | 用系统默认应用打开目标 |
| `windowAction(action)` | `window_action` | 无边框窗口控制（minimize / maximize / close） |
| `onWindowState(cb)` | 事件 `dsh-window-state` | 订阅窗口最大化状态 |
| `getStatus()` / `restart()` / `installDsh()` / `updateDsh()` / `openLogDirectory()` | 对应 native 命令 | dsh 运行状态与控制 |
| `getConfig()` / `setConfig(config)` | `get_config` / `set_config` | 读写桌面壳配置 |
| `onStatus(cb)` / `onLog(cb)` | 事件 `dsh-status` / `dsh-log` | 订阅运行状态与日志 |
| `onFileDrop(cb)` | 事件 `dsh-file-drop` | 订阅文件拖放 payload |
| `openPaths(paths)` / `importPaths(paths)` | `open_paths` / `import_paths` | 打开文件 / 导入目录动作 |
| `getPendingDeepLinks()` / `ackDeepLink(id)` / `onDeepLink(cb)` | `get_pending_deeplinks` / `ack_deeplink` / 事件 `dsh-deeplink` | 深链队列读取、确认与订阅 |
| `requestNotificationPermission()` / `onNotificationAction(cb)` | `request_notification_permission` / 事件 `dsh-notification-action` | 通知权限申请 / 点击动作 |
| `autostart.get()` / `autostart.set(enabled)` | `get_autostart` / `set_autostart` | 查询 / 设置开机自启 |
| `desktop.get()` / `desktop.set(settings)` | `get_desktop_settings` / `set_desktop_settings` | 查询 / 设置开机自启与自启后窗口状态 |
| `shortcuts.register(s, cb)` / `unregister(s)` / `list()` / `unregisterAll()` | `register_shortcut` / `unregister_shortcut` / `get_shortcuts` / `unregister_all_shortcuts` | 全局快捷键管理 |
| `onShortcut(cb)` | 事件 `dsh-shortcut` | 订阅快捷键按下 |
| `update.check()` / `update.install()` | `check_update` / `install_update` | 检查更新 / 静默下载安装包 |
| `projects.list()` / `add(path)` / `update(project)` / `remove(id)` | `get_projects` / `add_project` / `update_project` / `remove_project` | 项目列表读写（壳侧 `projects.json` 持久化） |
| `projects.setPinned(id, pinned)` / `markRead(id)` / `setArchived(id, archived)` | `set_project_pinned` / `mark_project_read` / `archive_project_chats` | 置顶 / 全部标为已读 / 聊天归档 |
| `projects.createWorktree(id)` / `showInFinder(id)` | `create_project_worktree` / `show_project_in_finder` | 创建永久工作树 / 文件管理器定位 |

“快捷键”设置 UI 由独立的 `@dsh-desktop/plugin-shortcuts` 插件提供；本插件只保留
`shortcuts` 桥接面，不实现设置页。“项目”设置 UI 由独立的 `@dsh-desktop/plugin-projects`
插件提供；本插件只保留 `projects` 桥接面，不实现设置页。

## 开机自启与启动模式设置项归属

- 设置项**状态**存放于壳侧应用数据目录的 `desktop-settings.json`（`startupMode` 默认 normal），
  OS 级启停由 autostart 插件管理——均属 **Tauri 壳域**；
- bridge client 面经 `desktop.get()` / `desktop.set(settings)` 调 `get_desktop_settings` /
  `set_desktop_settings` 读写壳状态，不再依赖 dsh settings 的 `desktop` 命名空间；
- dsh 上游 Web 配置接口对 `desktop` 命名空间返回 `settings-not-exposed`，因此桥接 UI 必须走壳命令，
  否则开关无法落盘。

## 外观设置项归属

- 外观设置项存于 dsh settings 的 `ui-theme` 命名空间（`preference` / `activeLightThemeId` /
  `activeDarkThemeId` / `customThemes` / `glassOpacity` / `wallpaperImage` /
  `wallpaperBlur` / `wallpaperPixelate` / 排版字段），属 **dsh 插件域**；
- 上游 `dsh-client-ui-theme` host 已注册该命名空间，bridge 只 bind、绝不重复注册；
- 壳侧为启动页/窗口背景读取同一分节（`get_ui_theme` / `dsh-ui-theme`），属 **Tauri 壳域**；
- 窗口控制按钮经 `windowAction` / `onWindowState` 受控桥接调用 `window_action` /
  `dsh-window-state`。

## 桌面壳形态

- `applyDesktopShell` 在 bridge client 装载时设置
  `html[data-dsh-desktop]` / `html[data-dsh-desktop-platform]`，并创建拖动条、
  非 macOS 窗口控制按钮；Shell 注入的 `HARNESS_CHROME_SCRIPT` 只保留侧边栏右键菜单。
- 普通 `dsh web`（未挂载 bridge）不会应用桌面形态；桌面壳通过 `--patch` 挂载 bridge 后，
  由插件自己负责平台标记与标题栏，不再由 Rust 注入窗口控制 DOM。

## 维护约定

- 新增桥接能力必须是**能力的最小切片**，须同步 `apps/shell/src-tauri/capabilities/bridge.json`
  白名单、`apps/shell/src-tauri/src/lib.rs` 的 `BRIDGE_SCRIPT`、`src/index.ts` 的
  `DshDesktopBridge`、client 侧 `BridgeLike`（如需使用）与本文件的桥接对象表
- 桥接契约不并入 `packages/contracts`（native 契约与桥接契约分离；桥接可以复用 contracts
  中已有的 native 快照/配置类型，但桥接对象形状与命令切片由本插件自持）
- client UI 设计遵循 `../design.md`；事件名必须是 cordis `Events` 接口里的键

## 样式构建约定

- dsh web 只静态托管 client bundle，不托管插件独立 `style.css`；bridge 按 dsh
  client module 的样式约定，把构建后的 `style.css` 文本嵌入 `client.js`，
  `client-kit/inject.ts` 在 factory 物化时以 `<style data-plugin-css>` 注入。
- 新增/修改 `*.module.css` 时不需要手动引入 CSS；`tsdown` 会自动生成 scoped
  类名并保留 `style.css` 构建产物。不要在 client 组件里直接写全局 DOM/外壳样式。
