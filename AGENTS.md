# AGENTS.md — dsh-desktop

用 Tauri 2 为 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)（`dsh`）打造的跨平台桌面套壳：自动拉起 `dsh web`，未安装时一键安装，就绪后直接在系统 WebView 中进入 Harness Web UI。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `yarn dev` | `tauri dev` 启动完整开发模式（同时拉起 Vite） |
| `yarn build` | `tauri build` 打包，产物在 `apps/shell/src-tauri/target/release/bundle/` |
| `yarn dev:web` | 仅启动前端 dev server（shell :5173） |
| `yarn build:web` | 构建前端到 `dist/`（Tauri 的 `beforeBuildCommand` 会调用） |
| `yarn build:plugins` | 编译 `packages/plugins` 各插件并把自包含 dist 包装配进 `apps/shell/src-tauri/resources/plugins/`（Tauri 的 `beforeBuildCommand` 也会调用） |
| `yarn typecheck` | 对所有 workspace 执行 TypeScript 类型检查（shell / contracts / plugins） |
| `yarn lint` / `yarn format` / `yarn format:check` | Biome 检查 / 格式化（格式约定见 `biome.json`） |
| `corepack yarn install` | 安装依赖（Yarn 4 + node-modules，生成 `node_modules` 目录） |

## 模块划分

- `apps/shell/` — main 窗口：启动页前端（Vite + TypeScript）与 `src-tauri/`（Tauri 2 + Rust 后端）
- `packages/contracts/` — 前后端共享的 IPC 契约类型与常量（`@dsh-desktop/contracts`）
- `packages/plugins/` — dsh 插件容器：`bridge/`（桥接 Tauri 壳能力）+ `hello/`（自定义示例），每个子目录一个 cordis 插件
- `.github/` — 三平台 CI 构建、自动发布与版本号脚本
- `docs/` — 截图等文档资源

## 窗口与关键流程

1. **main 窗口**：应用启动 300ms 后，Rust 侧 `DshManager` 发送 `Start` 消息开始检测 `dsh`
2. 已安装 → 保留一个空闲 loopback 端口，以 `dsh web --host 127.0.0.1 --port <port>` 拉起子进程
3. 未安装 → 前端显示“安装 DSH”按钮，一键执行 `npm install -g @deepseek-ai/dsh`（全局安装）
4. 子进程端口返回 HTTP 200 即视为就绪，主窗口 `navigate` 到 `http://127.0.0.1:<port>`
5. **桌面壳设置**：经 `packages/plugins/bridge` 插件在 dsh WebUI 设置面板提供“桌面”页（状态 / 配置 / 工具 / 开机自启），通过 `window.__DSH_DESKTOP__` 桥接调用壳能力；原独立控制中心（SolidJS）已迁移至此并移除
6. **内嵌插件自动挂载**：`packages/plugins` 各插件经 `yarn build:plugins` 编译打包进 Tauri resources（`resources/plugins/`），应用启动时 Rust 侧把插件包装配到 `$DSH_HOME/profiles/node_modules/@dsh-desktop/<name>/`（版本键控幂等、原子替换、best-effort）并生成 `embedded-plugins.patch.yml` overlay，以 `dsh web --patch <overlay>` 挂载（host 走 loader、client 走 dsh-client-modules）；不写 profile manifest、不下载；单独 `dsh web`（不带 `--patch`）不挂载内嵌插件

## 配置优先级

`config.json`（应用数据目录）→ 环境变量（`DSH_BIN` / `DSH_NODE` / `DSH_HOME`）→ PATH 检测。
前端通过 `get_config` / `set_config` 读写配置；`set_config` 只写 `config.json`，不覆盖环境变量。

## 项目约定

- 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)（`feat:` / `fix:` / `chore:`），版本号与 CHANGELOG 由 CI 依据提交信息自动生成
- 用户可见文案与代码注释使用中文
- 前后端通过固定契约通信：IPC 命令 `get_status` / `install_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config` / `open_external` / `get_autostart` / `set_autostart` / `register_shortcut` / `unregister_shortcut`，事件 `dsh-status` / `dsh-log` / `dsh-file-drop` / `dsh-theme` / `dsh-deeplink` / `dsh-shortcut` / `dsh-update-available`（类型与常量见 `packages/contracts`，各模块契约表见对应 AGENTS.md）
- 系统托盘常驻后台（关闭到托盘），关键节点弹原生通知；dsh web 通过受控桥接 `window.__DSH_DESKTOP__` 调用最小能力（白名单见 `apps/shell/src-tauri/capabilities/bridge.json`，边界见 `docs/plugin-tauri-boundary.md`）
- 原生能力：窗口状态记忆（重启恢复窗口大小/位置）、深链 `dsh-desktop://`、自动更新（后台检查 + “桌面”页“检查更新”）、开机自启（dsh 插件设置面板，默认关闭）、自定义全局快捷键（dsh 插件经桥接注册/注销）、托盘“退出”二次确认
- 后端是唯一状态源，前端只做渲染
- 代码规范：Biome（`biome.json`）负责 TS/TSX/JSON 的 lint 与格式；Lefthook（`lefthook.yml`）接入 git hooks——`pre-commit` 对暂存文件自动 `biome check --write`，`pre-push` 跑 `yarn typecheck` + `cargo fmt --check` + `cargo clippy`；`package.json` 的 `postinstall` 会在每次 `yarn install` 时自动执行 `lefthook install` 装好 hooks（Yarn 4 默认禁用依赖的 build scripts，需靠它兜底）

## 注意事项

- 前端在普通浏览器中无法完整运行（依赖 Tauri 注入的 `window.__TAURI_INTERNALS__`），只能调试样式/布局
- `dist/`、`apps/shell/src-tauri/target/`、`apps/shell/src-tauri/gen/` 均为生成产物，勿手改（已在 `.gitignore`）
- 新增 Tauri IPC 能力需同步修改 `apps/shell/src-tauri/capabilities/default.json` 权限声明
- 本仓库只有 `release` 分支；push 到 `release` 会触发自动发版
