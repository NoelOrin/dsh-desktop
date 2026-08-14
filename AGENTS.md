# AGENTS.md — dsh-desktop

用 Tauri 2 为 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)（`dsh`）打造的跨平台桌面套壳：自动拉起 `dsh web`，未安装时一键安装，就绪后直接在系统 WebView 中进入 Harness Web UI。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `yarn dev` | `tauri dev` 启动完整开发模式（同时拉起 Vite） |
| `yarn build` | `tauri build` 打包，产物在 `src-tauri/target/release/bundle/` |
| `yarn dev:web` | 仅启动 Vite 前端 dev server（http://localhost:5173） |
| `yarn build:web` | 构建前端到 `dist/`（Tauri 的 `beforeBuildCommand` 会调用） |
| `corepack yarn install` | 安装依赖（Yarn 4 + PnP，无 `node_modules` 目录） |

## 模块划分

- `src/` — 启动页前端（Vite + 原生 JS/CSS，无框架）：展示检测/安装/启动状态，就绪后跳转
- `src-tauri/` — Tauri 2 + Rust 后端：dsh 检测、一键安装、子进程管理与窗口导航
- `.github/` — 三平台 CI 构建、自动发布与版本号脚本
- `docs/` — 截图等文档资源

## 关键流程

1. 应用启动 300ms 后，Rust 侧 `DshManager` 发送 `Start` 消息开始检测 `dsh`
2. 已安装 → 保留一个空闲 loopback 端口，以 `dsh web --host 127.0.0.1 --port <port>` 拉起子进程
3. 未安装 → 前端显示“安装 DSH”按钮，一键执行 `npm install --prefix <runtime> @deepseek-ai/dsh`
4. 子进程端口返回 HTTP 200 即视为就绪，主窗口 `navigate` 到 `http://127.0.0.1:<port>`

## 项目约定

- 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)（`feat:` / `fix:` / `chore:`），版本号与 CHANGELOG 由 CI 依据提交信息自动生成
- 用户可见文案与代码注释使用中文
- 前后端通过固定契约通信：IPC 命令 `get_status` / `restart` / `install_dsh` / `open_log_directory`，事件 `dsh-status` / `dsh-log`
- 后端是唯一状态源，前端只做渲染

## 注意事项

- 前端在普通浏览器中无法完整运行（依赖 Tauri 注入的 `window.__TAURI_INTERNALS__`），只能调试样式/布局
- `dist/`、`src-tauri/target/`、`src-tauri/gen/` 均为生成产物，勿手改（已在 `.gitignore`）
- 新增 Tauri IPC 能力需同步修改 `src-tauri/capabilities/default.json` 权限声明
- 本仓库只有 `release` 分支；push 到 `release` 会触发自动发版
