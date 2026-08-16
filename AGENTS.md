# AGENTS.md - dsh-desktop

用 Tauri 2 为 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)（`dsh`）打造的跨平台桌面套壳：自动拉起 `dsh web`，未安装时一键安装，就绪后直接在系统 WebView 中进入 Harness Web UI。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `yarn dev` | `tauri dev` 启动完整开发模式（先构建插件，再启动 Vite 与插件 watch） |
| `yarn build` | `tauri build` 打包，产物在 `apps/shell/src-tauri/target/release/bundle/` |
| `yarn dev:web` | 仅启动前端 dev server（shell :5173） |
| `yarn dev:shell` | Tauri `beforeDevCommand` 使用的完整开发前置命令：初始构建插件 + 插件 watch + Vite |
| `yarn dev:plugins` | 独立插件 watch：监听 `packages/plugins` 全部源码（含 `client-kit/` 与 tsdown 配置），变化后重建并热部署到 dsh profile（`$DSH_HOME` 或 `~/.dsh`） |
| `yarn build:web` | 构建前端到 `dist/`（Tauri 的 `beforeBuildCommand` 会调用） |
| `yarn build:plugins` | 编译 `packages/plugins` 各插件并把自包含 dist 包装配进 `apps/shell/src-tauri/resources/plugins/`，自动清理已删除插件的旧产物；可传 `--home <path>` 同步热部署 |
| `yarn tauri` | 透传 Tauri CLI 到 shell workspace |
| `yarn lan-proxy` | 启动零依赖局域网反向代理（支持 Bearer token 门禁，详见 README） |
| `yarn typecheck` | 对所有 workspace 执行 TypeScript 类型检查（shell / contracts / plugins） |
| `yarn lint` / `yarn format` / `yarn format:check` | Biome 检查 / 格式化（格式约定见 `biome.json`） |
| `corepack yarn install` | 安装依赖（Yarn 4 + node-modules，生成 `node_modules` 目录） |

## 模块划分

- `apps/shell/` - main 窗口：启动页前端（Vite + TypeScript）与 `src-tauri/`（Tauri 2 + Rust 后端）
- `apps/shell/src-tauri/` - Rust 后端：进程管理、IPC、窗口/托盘、主题、插件装配与桥接注入
- `packages/contracts/` - 前后端共享的 native IPC 契约类型与常量（`@dsh-desktop/contracts`）
- `packages/plugins/` - dsh 插件容器：`bridge/`（桥接 Tauri 壳能力）+ `projects/`（项目列表与右键菜单 host 端点）+ `shortcuts/`（全局快捷键设置页）+ `reasoning/`（模型设置页，含第三方思考强度），每个含 `dsh` 字段的子目录一个 cordis 插件；`client-kit/` 为共享注入/构建 helper，不进入插件 resources；`packages/external-plugins/` 是外部远程插件预设目录（每个顶层 JSON 文件一个 group）
- `scripts/` - 根级开发脚本：`dev.mjs` / `watch-plugins.mjs` / `build-plugins.mjs` / `lan-proxy.mjs`
- `.github/` - 三平台 CI 构建、自动发布与版本号脚本
- `docs/` - 插件边界文档、设计/实施记录与截图等资源

## 窗口与关键流程

1. **main 窗口**：应用启动 300ms 后，Rust 侧 `DshManager` 发送 `Start` 消息开始检测 `dsh`
2. 启动/重启前会先清理本应用历史残留的 `dsh web` 实例（通过应用数据目录 overlay 与 `DSH_DESKTOP_MANAGED` 标记识别，只清本应用自管理进程）
3. 已安装 → 先检测外部远程插件路径（`packages/external-plugins` / `remote_plugins_path` / `$DSH_HOME/remote-plugins(.json)`）并按 group 加载固定预设，再自动安装启用的远程插件预设（`dsh plugin --profile <active> add <url>`），随后装配内嵌插件并生成 `embedded-plugins.patch.yml` overlay，最后以 `dsh --profile <active> [--patch <overlay>] --host 127.0.0.1 --port 0` 拉起子进程（端口由 dsh 自选，默认 profile 为 `web`）
4. 未安装 → 前端显示“安装 DSH”按钮，一键执行 `npm install -g @deepseek-ai/dsh`（全局安装）
5. 子进程输出 `dsh web: http://127.0.0.1:<port>` 后，壳侧持续探测 `/dsh-desktop/health`；返回 HTTP 200 即视为就绪，主窗口 `navigate` 到该 URL
6. **桌面壳设置**：经 `packages/plugins/bridge` 插件在 dsh WebUI 设置面板提供“插件”页（外部固定 + 本地分组预设 / 整组同步 / 已安装插件 / 移除 / 更新）、“桌面”页（状态 / 配置 / 运行环境 profile / 界面模式 / 工具 / 开机自启与启动模式）与“外观”页（主题偏好 / 主题库 / 背景图 / 玻璃透明度 / 自定义主题 / 排版），通过 `window.__DSH_DESKTOP__` 桥接调用壳能力；原独立控制中心已迁移至此并移除
7. **内嵌插件自动挂载**：`packages/plugins` 各插件经 `yarn build:plugins` 编译打包进 Tauri resources（`apps/shell/src-tauri/resources/plugins/`），应用启动时 Rust 侧把插件包装配到 `$DSH_HOME/profiles/node_modules/@dsh-desktop/<name>/`（内容一致性幂等、原子替换、best-effort）并生成 `embedded-plugins.patch.yml` overlay，以 `dsh --profile <active> --patch <overlay>` 挂载（host 走 loader、client 走 dsh-client-modules）；不写 profile manifest、不下载依赖；单独 `dsh`（不带 `--patch`）不挂载内嵌插件
   - 发布模式按文件内容一致性幂等装配，版本相同但内容不同也会自愈重装；开发模式 `assemble_dev` 强制重装，避免源码更新后仍加载旧产物
   - `yarn dev` 下由 `scripts/watch-plugins.mjs` 监听插件源码并热部署到 profile，client bundle 经 dsh 自带 `dsh-client-hmr` 热替换，开发模式收到 rebuilt 帧后自动刷新 Web UI
8. **主题与无边框窗口**：壳侧读取 `settings.yaml` 的 `ui-theme` 分节与壳侧 `desktop-settings.json` 的 `startupMode`，通过 `get_ui_theme` / `dsh-ui-theme` 让启动页和窗口背景跟随；main 窗口无系统边框（macOS 使用 Overlay title bar），启动页与 dsh web 注入自绘标题栏（拖动、双击最大化、窗口控制按钮）

## 配置优先级

`config.json`（应用数据目录，含 `remote_plugins_path` 外部插件路径与 `remote_plugins` 本地分组预设）→ 环境变量（`DSH_BIN` / `DSH_NODE` / `DSH_HOME` / `DSH_DESKTOP_REMOTE_PLUGINS_PATH`）→ PATH 检测。
PATH 检测会合并桌面进程自身 PATH、macOS 系统 PATH（`/etc/paths` + `/etc/paths.d`）、Linux `/etc/environment`、Windows 用户/系统注册表 PATH，以及非 Windows 用户 shell PATH；合并结果也会注入 dsh/npm 子进程。
前端通过 `get_config` / `set_config` 读写配置；`set_config` 只写 `config.json`，不覆盖环境变量。

## 项目约定

- 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)（`feat:` / `fix:` / `chore:`），版本号与 CHANGELOG 由 CI 依据提交信息自动生成
- 用户可见文案与代码注释使用中文
- dsh 官方插件开发、UI 规范与组件使用约定已固定于 `packages/plugins/AGENTS.md`
  （含官方文档链接，新增插件/修改 client 面前先读该文件；client UI 设计范式另见 `packages/plugins/design.md`）
- 前后端通过固定契约通信：
  - native 命令：`get_status` / `install_dsh` / `update_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config` / `open_external` / `get_ui_theme` / `window_action` / `get_pending_deeplinks` / `ack_deeplink` / `request_notification_permission` / `open_paths` / `import_paths` / `get_projects` / `add_project` / `update_project` / `remove_project` / `set_project_pinned` / `mark_project_read` / `archive_project_chats` / `create_project_worktree` / `show_project_in_finder` / `get_profiles` / `get_active_profile` / `select_profile` / `get_remote_plugins` / `set_remote_plugins` / `sync_remote_plugins` / `get_installed_plugins` / `install_profile_plugin` / `remove_profile_plugin` / `update_profile_plugins`
  - 桥接/壳能力命令（remote 白名单以 `capabilities/bridge.json` 与 `shortcuts.json` 为准）：`get_status` / `restart` / `install_dsh` / `open_log_directory` / `get_config` / `set_config` / `open_external` / `get_autostart` / `set_autostart` / `get_desktop_settings` / `set_desktop_settings` / `get_profiles` / `get_active_profile` / `select_profile` / `get_remote_plugins` / `set_remote_plugins` / `sync_remote_plugins` / `get_installed_plugins` / `install_profile_plugin` / `remove_profile_plugin` / `update_profile_plugins` / `register_shortcut` / `unregister_shortcut` / `get_shortcuts` / `unregister_all_shortcuts` / `check_update` / `install_update` / `window_action`
  - 事件：`dsh-status` / `dsh-log` / `dsh-file-drop` / `dsh-theme` / `dsh-deeplink` / `dsh-shortcut` / `dsh-update-available` / `dsh-ui-theme` / `dsh-window-state` / `dsh-notification-action`
  - 类型与常量见 `packages/contracts`；桥接对象 `window.__DSH_DESKTOP__` 的契约由 `packages/plugins/bridge/AGENTS.md` 自持
- 系统托盘常驻后台（关闭到托盘），关键节点弹原生通知；dsh web 通过受控桥接 `window.__DSH_DESKTOP__` 调用最小能力（白名单见 `apps/shell/src-tauri/capabilities/bridge.json`，边界见 `docs/plugin-tauri-boundary.md`）
- 原生能力：无边框窗口 + 自绘标题栏、窗口状态记忆（重启恢复窗口大小/位置）、主题/背景图跟随、深链 `dsh-desktop://`、自动更新（后台检查 + “桌面”页“检查更新”，静默下载安装包后由用户手动安装）、`update_dsh` 更新 dsh 本体、开机自启与启动模式（dsh 插件设置面板，默认关闭）、profile 选择与回滚、远程插件预设与受管插件操作、自定义全局快捷键（dsh 插件经桥接注册/注销）、文件打开/导入、通知点击动作、托盘“退出”二次确认
- 后端是唯一状态源，前端只做渲染
- 代码规范：Biome（`biome.json`）负责 TS/TSX/JSON 的 lint 与格式；Lefthook（`lefthook.yml`）接入 git hooks——`pre-commit` 对暂存文件自动 `biome check --write`，`pre-push` 跑 `yarn typecheck` + `cargo fmt --check` + `cargo clippy`；`package.json` 的 `postinstall` 会在每次 `yarn install` 时自动执行 `lefthook install` 装好 hooks（Yarn 4 默认禁用依赖的 build scripts，需靠它兜底）

## 注意事项

- 前端在普通浏览器中无法完整运行（依赖 Tauri 注入的 `window.__TAURI_INTERNALS__`），只能调试样式/布局
- `dist/`、`apps/shell/src-tauri/target/`、`apps/shell/src-tauri/gen/` 与 `apps/shell/src-tauri/resources/plugins/` 均为生成产物，勿手改（已在 `.gitignore` 或由脚本生成）
- 新增 Tauri IPC 能力需同步修改 `apps/shell/src-tauri/capabilities/default.json` 和/或 `bridge.json`、`packages/contracts/src/index.ts`、Rust 命令、相关前端/桥接调用与各 AGENTS 契约表
- 修改根级脚本、CI、插件装配或桥接面时，同步更新对应子目录 `AGENTS.md`，避免文档与实现脱节
- 本仓库只有 `release` 分支；push 到 `release` 会触发自动发版
