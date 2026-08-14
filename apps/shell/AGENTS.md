# AGENTS.md — src（启动页前端）

DSH Desktop 的启动页：Vite + 原生 JS/CSS 的单页应用（无框架）。在 Rust 后端就绪前展示 dsh 的检测 / 安装 / 启动状态，就绪后整页跳转到 Harness Web UI。

## 文件

- `../index.html` — 入口 HTML：状态点、按钮、日志面板的挂载点
- `src/main.js` — 全部前端逻辑：Tauri IPC 调用与事件监听
- `src/style.css` — 全部样式（CSS 变量 + 轻量动画，含 reduced-motion 适配）

## 与后端的通信契约

| 方向 | 名称 | 说明 |
| --- | --- | --- |
| 调用 | `invoke("get_status")` | 拉取当前状态快照 `RuntimeSnapshot` |
| 调用 | `invoke("install_dsh")` | 触发一键安装 dsh |
| 调用 | `invoke("restart")` | 重新检测并启动 dsh |
| 调用 | `invoke("open_log_directory")` | 打开日志目录 |
| 监听 | `dsh-status` | 状态快照（phase / message / url / logs 等） |
| 监听 | `dsh-log` | 追加一行日志文本 |

> 修改 IPC 契约时，必须同步更新本表与 `../src-tauri/AGENTS.md`。

## 状态机（前端渲染依据）

phase 取值（snake_case）：`detecting` → `missing`（缺 dsh 或 node）| `installing` | `starting` → `ready`（跳转 `status.url`）| `failed` | `stopped`

- 仅 `missing` 显示“安装 DSH”按钮，仅 `failed` 显示“重试”与“日志目录”
- `installing` / `starting` / `failed` 显示日志面板
- `data-phase` 驱动按钮显隐与状态点颜色（见 `style.css` 的 `.status[data-phase=...]` 规则）
- phase 为 `ready` 且有 url 时，用 `window.location.href = status.url` 整页跳转进入 dsh web

## 关键实现点

- `isTauri()` 检查 `window.__TAURI_INTERNALS__`；普通浏览器中打开只会渲染“正在检测运行环境...”，不会调用 IPC
- 前端不做状态管理——后端是唯一状态源，本模块只负责渲染与触发操作
- 日志区通过 `appendLog` 追加并自动滚动到底部；初始日志来自 `get_status().logs`
- UI 文案为中文

## 开发调试

- 单独调样式/布局：`yarn dev:web`，浏览器打开 http://localhost:5173（无 IPC，无法走完整链路）
- 完整链路：`yarn dev`（tauri dev 同时启动 Vite 并注入 Tauri 环境）
- 构建产物输出到 `dist/`，由 `yarn build:web` 生成
