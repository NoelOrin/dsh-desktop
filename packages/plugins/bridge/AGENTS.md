# AGENTS.md — packages/plugins/bridge（桥接 Tauri 壳能力的 dsh 插件）

`@dsh-desktop/plugin-bridge`：让 dsh web 内的插件/页面能调用 Tauri 壳能力的**双面 dsh 插件**
（cordis bundle）。**桥接命令契约由本插件自持**，Rust 侧在
`apps/shell/src-tauri/capabilities/bridge.json` 声明实现与权限；不并入
`packages/contracts`——contracts 只负责 native 内容（边界见 `docs/plugin-tauri-boundary.md`）。

## 双面结构

| 面 | 文件 | 职责 |
| --- | --- | --- |
| host | `src/index.ts` | 插件入口（`export const name` + `apply(ctx)`）；定义 `DshDesktopBridge` 类型；注册 `desktop` settings 命名空间（`autostart: z.boolean().default(false)`，默认关闭） |
| client | `src/client.tsx` | 在 dsh WebUI 设置面板注册“桌面”设置节（状态 / 配置 / 工具 / 开机自启）与“外观”设置节（主题偏好 / 主题库 / 背景图 / 玻璃透明度 / 自定义主题 / 排版）；`ui-theme` 命名空间只 bind 不注册，快照经 `createThemeStore` 防抖写回，变化时 `applyThemeSection` 实时应用 |

`package.json` 通过 `dsh.client`（platform web）声明 client 面并导出 `./client`；
client 面依赖 `@deepseek-ai/dsh-client-ui-settings` 等 dsh client 生态（peer 声明）。

## 桥接对象（window.__DSH_DESKTOP__）

壳侧 `BRIDGE_SCRIPT` 注入到 dsh web 页面的受控 API。与原生能力二期相关的最小切片：

| 成员 | 壳命令 / 事件 | 说明 |
| --- | --- | --- |
| `autostart.get()` / `autostart.set(enabled)` | `get_autostart` / `set_autostart` | 查询 / 设置开机自启 |
| `shortcuts.register(s, cb)` / `shortcuts.unregister(s)` | `register_shortcut` / `unregister_shortcut` | 注册 / 注销系统级全局快捷键 |
| `onShortcut(cb)` | 事件 `dsh-shortcut` | 订阅快捷键按下（payload 为快捷键字符串） |
| `onDeepLink(cb)` | 事件 `dsh-deeplink` | 订阅 `dsh-desktop://` 深链（payload 为原始 URL） |
| `windowAction(action)` | `window_action` | 无边框窗口控制（minimize / maximize / close） |
| `onWindowState(cb)` | 事件 `dsh-window-state` | 订阅窗口最大化状态（payload 为 `{ maximized }`） |

## 开机自启设置项归属

- 设置项**状态**存放于 dsh settings（`desktop.autostart`，默认关闭）——属 **dsh 插件域**；
- OS 级**启停**是壳能力（autostart 插件 + `get_autostart` / `set_autostart` 命令）——属 **Tauri 壳域**；
- 切换开关时 client 面同时写 settings 与调壳命令，两者解耦（壳不可达时仅写 settings，
  由下次启动 / 桥接补偿）。

## 外观设置项归属

- 外观设置项存于 dsh settings 的 `ui-theme` 命名空间（`preference` / `activeLightThemeId` /
  `activeDarkThemeId` / `customThemes` / `glassOpacity` / `wallpaperImage` /
  `wallpaperBlur` / `wallpaperPixelate` / 排版字段），属 **dsh 插件域**；
- 上游 `dsh-client-ui-theme` host 已注册该命名空间，bridge 只 bind、绝不重复注册；
- 壳侧为启动页/窗口背景读取同一分节（`get_ui_theme` / `dsh-ui-theme`），属 **Tauri 壳域**；
- 窗口控制按钮经 `windowAction` / `onWindowState` 受控桥接调用 `window_action` /
  `dsh-window-state`。

## 维护约定

- 新增桥接能力必须是**能力的最小切片**，须同步 `apps/shell/src-tauri/capabilities/bridge.json`
  白名单、`apps/shell/src-tauri/src/lib.rs` 的 `BRIDGE_SCRIPT` 与本文件的桥接对象表
- 桥接契约不并入 `packages/contracts`（native 契约与桥接契约分离）
- 事件名必须是 cordis `Events` 接口里的键（骨架阶段不要注册未声明的事件）
