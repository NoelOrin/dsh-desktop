# AGENTS.md — apps/shell/src-tauri（Tauri 2 + Rust 后端）

DSH Desktop 的 Rust 后端。职责：检测 / 一键安装 `dsh`、以子进程方式拉起并监控 `dsh web`、就绪后把 main 窗口导航到 Web UI，同时将状态与日志实时推送给前端；另负责 `config.json` 的读写（`get_config` / `set_config`）与桥接能力（`window.__DSH_DESKTOP__`）。

## 文件地图

- `src/lib.rs` — 全部后端逻辑：状态机、进程管理、IPC 命令、桥接注入
- `src/config.rs` — `DshConfig` 结构、`config.json` 的 load/save 与 effective 合并逻辑
- `src/main.rs` — 仅入口：调用 `dsh_desktop_lib::run()`
- `Cargo.toml` — 依赖：`tauri 2`（`tray-icon` / `image-png`）、`tauri-plugin-global-shortcut` / `notification` / `single-instance` / `clipboard-manager` / `dialog` / `window-state` / `deep-link` / `updater` / `autostart`、`libc`（unix 优雅退出）、`serde`、`serde_json`；lib 名为 `dsh_desktop_lib`
- `build.rs` — 用 `AppManifest::commands` 为应用命令生成 `allow-*` ACL 权限（远程桥接与本地窗口共用）
- `tauri.conf.json` — main 窗口配置、构建前后命令、bundle 目标
- `capabilities/default.json` — 本地窗口（main）IPC 权限：`core:default` + `global-shortcut:default` + `updater:default` + 插件权限 + 应用命令 `allow-*`（含 `get_autostart` / `set_autostart` / `register_shortcut` / `unregister_shortcut`）
- `capabilities/bridge.json` — dsh web（remote `http://127.0.0.1:*` 白名单）受控桥接权限，仅授予最小命令切片（含 `allow-get-autostart` / `allow-set-autostart` / `allow-register-shortcut` / `allow-unregister-shortcut`）
- `gen/` — 构建生成的 schema（勿手改，已在 `.gitignore`）
- `icons/` — 应用图标（由 `tauri icon` 生成，勿手改）

## 架构

- `AppState`（Tauri State）：共享 `Arc<Mutex<Inner>>` + `mpsc::Sender<ManagerMessage>` 消息入口
- `DshManager`：独立线程 `run()` 事件循环，用 `recv_timeout(POLL_INTERVAL)` 消费消息并轮询子进程退出状态
- `Inner`：`Arc<Mutex<...>>` 共享可变状态（phase / message / url / logs 等），`snapshot()` 生成 `RuntimeSnapshot`（日志最多回看 200 条）
- 子进程管道：stdout/stderr 各起一个 reader 线程，经 `append_line` 写入 `logs/dsh.log`、维护环形缓冲（`MAX_LOGS=500`）并 emit `dsh-log`
- 就绪检测：独立线程每 250ms 用原始 TCP 发 `GET / HTTP/1.0`，收到 `HTTP/1.x 200` 即视为就绪；`READY_TIMEOUT=120s`

## 消息协议（ManagerMessage）

`Start` / `Stop` / `Shutdown` / `Ready{generation,url}` / `ReadyTimeout{generation,port}` / `InstallFinished{result}`

> 关键：`generation` 计数器用于丢弃过期消息——重启 / 清理子进程后，旧线程发来的 `Ready` / `ReadyTimeout` 必须与当前 generation 比对后才生效。

## IPC 命令与事件

- 命令：`get_status` / `restart` / `install_dsh` / `open_log_directory` / `get_config` / `set_config` / `open_external`（系统默认应用打开目标）/ `get_autostart`（查询开机自启状态）/ `set_autostart`（开启/关闭开机自启）/ `register_shortcut`（注册系统级全局快捷键）/ `unregister_shortcut`（注销全局快捷键）
- 事件（向前端 emit）：`dsh-status`（RuntimeSnapshot）、`dsh-log`（单行文本）、`dsh-file-drop`（拖入 main 窗口的真实路径数组）、`dsh-theme`（系统主题 `light`/`dark`）、`dsh-deeplink`（`dsh-desktop://` 深链原始 URL）、`dsh-shortcut`（按下的已注册快捷键字符串）、`dsh-update-available`（自动更新发现的新版本号）
- `get_config` 返回“生效配置”：config.json 有值则用之，未设置的字段回退到环境变量；`set_config` 只写 `config.json`，不会修改环境变量
- 修改契约时，必须同步更新 `../../../AGENTS.md` 的通信契约表与 `../../../packages/contracts/src/index.ts`

## 原生能力

- **系统托盘**：`setup_tray()` 用 `TrayIconBuilder` 创建（`ManagedTray` 经 `app.manage` 保活）；菜单含 显示主窗口 / 重启 dsh / 退出；main 窗口关闭请求被拦截，默认**关闭到托盘**（仅隐藏），托盘"退出"置 `exiting` 后真正退出
- **原生通知**：`DshManager::notify()` 在 就绪 / 失败 / 安装完成 时经 notification 插件发系统通知
- **单实例锁**：`tauri-plugin-single-instance` 最先注册，二次启动聚焦主窗口
- **看门狗**：子进程在 `starting`/`ready` 阶段意外退出时自动重启，`MAX_AUTO_RESTARTS=3` 次内连续重试，超限转 `failed`；手动 Start / 安装完成清零计数
- **主题跟随**：CSS `prefers-color-scheme` 深色变量 + Rust `ThemeChanged` 事件 emit `dsh-theme`
- **优雅退出**：`cleanup_child()` unix 下先对进程组（`process_group(0)` 启动）发 SIGTERM，`GRACE_PERIOD=2s` 宽限后 SIGKILL；Windows 直接 TerminateProcess
- **拖放**：main 窗口 `DragDropEvent::Drop` 把真实路径 emit `dsh-file-drop`
- **桥接**：`Builder::on_page_load` 在 dsh web 页面（127.0.0.1）加载完成后 `eval` `BRIDGE_SCRIPT`，注入 `window.__DSH_DESKTOP__`（最小能力：通知 / 剪贴板 / 对话框 / openExternal / 状态事件 / 文件拖放 / 开机自启 autostart / 快捷键 shortcuts / 深链 onDeepLink），权限由 `capabilities/bridge.json` remote 白名单收口
- **窗口状态记忆**：window-state 插件在 `RunEvent::Exit`（托盘“退出”）时自动保存 main 窗口大小/位置/最大化状态，下次启动恢复——关闭到托盘仅隐藏不销毁，不触发 CloseRequested 保存路径
- **深链**：`tauri.conf.json` 注册 `dsh-desktop://` scheme；`on_open_url` 收到 URL 后 emit `dsh-deeplink`（原始 URL 字符串），未就绪时暂存 `Inner.pending_deeplinks`、就绪后补发；桥接暴露 `onDeepLink(cb)`
- **自动更新**：updater 插件（endpoints 指向 GitHub Release 的 `latest.json`）；启动后就绪后异步 `check()`，发现新版本 emit `dsh-update-available`（payload 为新版本号），失败仅记日志；桌面壳设置“桌面”页的“检查更新”走 `@tauri-apps/plugin-updater` 的 `downloadAndInstall` + `@tauri-apps/plugin-process` 的 `relaunch`（经桥接或插件侧调用）
- **开机自启**：autostart 插件（macOS LaunchAgent，`--autostart` 参数）注册；`get_autostart` / `set_autostart` 命令经 `autolaunch()` 查/改；桥接暴露 `autostart.get()/set()`；设置项 UI 属 dsh 插件（`packages/plugins/bridge` 的 `desktop` settings 命名空间，默认关闭）
- **自定义快捷键**：`register_shortcut` / `unregister_shortcut` 命令（内部用 global-shortcut 插件，注册表 `HashMap<String, Shortcut>` 存 AppState），按下时 emit `dsh-shortcut`；桥接暴露 `shortcuts.register/unregister/onShortcut`
- **退出确认**：托盘“退出”先经 dialog 弹系统确认框（防误触导致 dsh 会话丢失），确认后才置 `exiting`、发 `Stop` 并退出；取消则无操作

## 窗口与快捷键

- **main 窗口**：`tauri.conf.json` 中声明（label `main`），普通不透明窗口

## config.json 与配置优先级

- 位置：`app_data_dir()/config.json`；字段 `dsh_bin` / `dsh_node` / `dsh_home`（null 表示未设置）
- 优先级：**config.json → 环境变量（`DSH_BIN` / `DSH_NODE` / `DSH_HOME`）→ PATH 检测**
- `DshConfig::effective()` 实现合并：config.json 有值则用之，否则回退到环境变量；`resolve_dsh` 再在合并结果之上做文件级检测

## 关键行为与陷阱

- 常量：`READY_TIMEOUT=120s`、`POLL_INTERVAL=250ms`、`MAX_LOGS=500`（环形日志）
- `reserve_port()`：绑定 `127.0.0.1:0` 拿到空闲端口后立即 drop —— 存在极小竞态窗口，子进程应尽快接管
- 窗口关闭：main 窗口默认**关闭到托盘**（仅隐藏，不退出）；托盘"退出"或 `RunEvent::Exit` 时发送 `Stop`/`Shutdown` 并优雅清理子进程后退出
- 环境变量：`DSH_BIN`（dsh 入口）、`DSH_NODE`（Node 解释器）、`DSH_HOME`（透传给子进程）；数据目录为 `app_data_dir()/logs` 与 `.../config.json`（`dsh` 通过一键安装全局安装，不写入应用数据目录）
- `resolve_dsh` 的查找顺序：`dsh_bin`（config.json 优先于环境变量）→ PATH → npm 全局安装目录（`npm prefix -g`，含 `bin/dsh` 与 `lib/node_modules/@deepseek-ai/dsh/lib/bin.js`）→ `~/.vite-plus/bin/dsh`

## 开发与验证

- 编译检查：`cd apps/shell/src-tauri && cargo check`；完整启动：仓库根 `yarn dev`
- 保持 `cargo fmt` 与 `cargo clippy` 无告警
- 跨平台注意：`open_external` 按目标平台分派（macOS `open` / Linux `xdg-open` / Windows `cmd start`）；`exit_summary` 在 unix 下读取 signal
