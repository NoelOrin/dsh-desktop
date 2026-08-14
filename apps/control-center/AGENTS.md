# AGENTS.md — apps/control-center（SolidJS 控制中心）

DSH Desktop 的第二个窗口：SolidJS SPA（Vite 8 + vite-plugin-solid + TypeScript），
按需打开（应用菜单项“控制中心” / CmdOrCtrl+Shift+C），包含仪表盘 / 设置 / 工具三个 Tab。

## 文件

- `src/index.tsx` — 入口，挂载 App
- `src/App.tsx` — 三 Tab 布局（无路由库，状态切换）
- `src/pages/Dashboard.tsx` — 状态 / 日志 / 控制按钮
- `src/pages/Settings.tsx` — DSH_BIN / DSH_NODE / DSH_HOME 表单（get_config/set_config）
- `src/pages/Tools.tsx` — 工具入口占位
- `src/lib/ipc.ts` — 全部 Tauri 调用与事件监听的唯一入口
- `src/styles.css` — 全部样式

## 构建与开发

- dev server @ http://localhost:5174（strictPort）；`base: "/control-center/"`
- 构建输出 `../../dist/control-center`（构建顺序：shell 先，control-center 后）
- 窗口 URL：release 为 `/control-center/index.html`，dev 下 Rust 导航到 :5174/control-center/

## 通信契约

- IPC 命令：`get_status` / `install_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config`
- 事件：`dsh-status`（RuntimeSnapshot）、`dsh-log`（文本行）
- 类型与常量来自 `@dsh-desktop/contracts`；修改契约须同步 Rust serde 类型与 `apps/shell/src`
