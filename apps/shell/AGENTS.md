# AGENTS.md - apps/shell（启动页前端）

DSH Desktop 的 main 窗口启动页：Vite + TypeScript 的单页应用（无框架）。在 Rust 后端就绪前展示 dsh 的检测 / 安装 / 启动状态，就绪后整页跳转到 Harness Web UI。`apps/shell` 同时承载前端（`index.html` / `src/`）与后端（`src-tauri/`）。

## 文件

- `index.html` - 入口 HTML：自绘标题栏、状态点、按钮、日志面板的挂载点
- `src/main.ts` - 全部前端逻辑：Tauri IPC 调用、事件监听、主题应用与窗口控制
- `src/style.css` - 全部样式（CSS 变量 + 轻量动画，含 reduced-motion 适配与 `--theme-*` 主题变量）
- `vite.config.ts` - Vite 配置：端口 5173、输出到仓库根 `dist/`

## 与后端的通信契约

| 方向 | 名称 | 说明 |
| --- | --- | --- |
| 调用 | `invoke("get_status")` | 拉取当前状态快照 `RuntimeSnapshot` |
| 调用 | `invoke("install_dsh")` | 触发一键安装 dsh |
| 调用 | `invoke("restart")` | 重新检测并启动 dsh |
| 调用 | `invoke("open_log_directory")` | 打开日志目录 |
| 调用 | `invoke("get_ui_theme")` | 拉取壳侧主题快照 `UiThemeSnapshot` |
| 调用 | `invoke("window_action")` | 无边框窗口控制（minimize / maximize / close） |
| 监听 | `dsh-status` | 状态快照（phase / message / url / logs 等） |
| 监听 | `dsh-log` | 追加一行日志文本 |
| 监听 | `dsh-theme` | 系统主题（light / dark，供启动页深色适配） |
| 监听 | `dsh-ui-theme` | 壳侧主题快照更新 |
| 监听 | `dsh-window-state` | 窗口最大化状态（`WindowState`） |

> 本页只使用上述命令；`get_config` / `set_config` 等完整契约由 dsh 插件设置面板经桥接使用，类型定义见 `../../packages/contracts`。修改 IPC 契约时，必须同步更新 `../../packages/contracts/src/index.ts`、Rust 端 serde 类型（`apps/shell/src-tauri/src/lib.rs` / `config.rs`）、根 `AGENTS.md` 与 `./src-tauri/AGENTS.md` 的契约表。

## 状态机（前端渲染依据）

phase 取值（snake_case）：`detecting` → `missing`（缺 dsh 或 node）| `installing` | `starting` → `ready`（跳转 `status.url`）| `failed` | `stopped`

- 仅 `missing` 且 `node_found` 为 true（已检测到 Node.js）时显示“安装 DSH”按钮；缺 Node.js 时隐藏按钮、仅提示“请先安装 Node.js”；仅 `failed` 显示“重试”与“日志目录”
- `installing` / `starting` / `failed` 显示日志面板
- `data-phase` 驱动按钮显隐与状态点颜色（见 `style.css` 的 `.status[data-phase=...]` 规则）
- phase 为 `ready` 且有 url 时，用 `window.location.href = status.url` 整页跳转进入 dsh web

## 关键实现点

- `isTauri()` 检查 `window.__TAURI_INTERNALS__`；普通浏览器中打开只会渲染“正在检测运行环境...”，不会调用 IPC
- 前端不做状态管理——后端是唯一状态源，本模块只负责渲染与触发操作
- 日志区通过 `appendLog` 追加并自动滚动到底部；初始日志来自 `get_status().logs`
- 自绘标题栏使用 `data-tauri-drag-region="deep"`，按钮经 `window_action` 调用窗口控制，并通过 `dsh-window-state` 切换最大化/还原图标；非 Tauri 环境隐藏标题栏
- 启动页主题经 `get_ui_theme` 拉取快照并监听 `dsh-ui-theme`，把 `--theme-*` 变量写入根节点
- UI 文案为中文

## 开发调试

- 单独调样式/布局：`yarn dev:web`（本 workspace 内为 `vite`），浏览器打开 http://localhost:5173（无 IPC，无法走完整链路）
- 完整链路：仓库根 `yarn dev`（tauri dev 同时启动 Vite 并注入 Tauri 环境）
- 构建产物输出到仓库根 `dist/`（`vite.config.ts` 的 `outDir: "../../dist"`），由根 `yarn build:web` 生成
