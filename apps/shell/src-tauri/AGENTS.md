# AGENTS.md — apps/shell/src-tauri（Tauri 2 + Rust 后端）

DSH Desktop 的 Rust 后端。职责：检测 / 一键安装 `dsh`、以子进程方式拉起并监控 `dsh web`、就绪后把 main 窗口导航到 Web UI，同时将状态与日志实时推送给前端；另负责 `config.json` 的读写（`get_config` / `set_config`）与 control 窗口的按需打开（菜单 / 全局快捷键）。

## 文件地图

- `src/lib.rs` — 全部后端逻辑：状态机、进程管理、IPC 命令、control 窗口打开
- `src/config.rs` — `DshConfig` 结构、`config.json` 的 load/save 与 effective 合并逻辑
- `src/main.rs` — 仅入口：调用 `dsh_desktop_lib::run()`
- `Cargo.toml` — 依赖：`tauri 2`、`tauri-plugin-global-shortcut`、`serde`、`serde_json`；lib 名为 `dsh_desktop_lib`
- `tauri.conf.json` — control 窗口配置、构建前后命令、bundle 目标（main 窗口在 setup 中手动构建）
- `capabilities/default.json` — IPC 权限（`core:default` + `global-shortcut:default`，覆盖 main 与 control 窗口）
- `resources/titlebar-transparency.js` — macOS 透明标题栏样式注入
- `build.rs` — 仅调用 `tauri_build::build()`
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

- 命令：`get_status` / `restart` / `install_dsh` / `open_log_directory` / `get_config` / `set_config`
- 事件（向前端 emit）：`dsh-status`（RuntimeSnapshot）、`dsh-log`（单行文本）
- `get_config` 返回“生效配置”：config.json 有值则用之，未设置的字段回退到环境变量；`set_config` 只写 `config.json`，不会修改环境变量
- 修改契约时，必须同步更新 `../../../AGENTS.md` 的通信契约表与 `../../../packages/contracts/src/index.ts`

## 窗口与快捷键

- **main 窗口**：在 `setup` 中手动构建；macOS 启用透明窗口 + Overlay 标题栏 + vibrancy 毛玻璃（注入 `titlebar-transparency.js`），其他平台为普通不透明窗口
- **control 窗口**：`tauri.conf.json` 中声明（label `control`，`visible: false`）；菜单项“控制中心”与全局快捷键 `CmdOrCtrl+Shift+C` 都调用 `open_control_window()`——已存在则显示并聚焦（重复调用只聚焦）；关闭请求被拦截（`prevent_close`），仅隐藏不销毁
- 菜单在 `setup` 中通过 `build_menu()` 构建（“DSH” 子菜单），`on_menu_event` 打开 control 窗口
- dev 模式下 control 窗口导航到 `http://localhost:5174/control-center/`（`debug_assertions`）

## config.json 与配置优先级

- 位置：`app_data_dir()/config.json`；字段 `dsh_bin` / `dsh_node` / `dsh_home`（null 表示未设置）
- 优先级：**config.json → 环境变量（`DSH_BIN` / `DSH_NODE` / `DSH_HOME`）→ PATH 检测**
- `DshConfig::effective()` 实现合并：config.json 有值则用之，否则回退到环境变量；`resolve_dsh` 再在合并结果之上做文件级检测

## 关键行为与陷阱

- 常量：`READY_TIMEOUT=120s`、`POLL_INTERVAL=250ms`、`MAX_LOGS=500`（环形日志）
- `reserve_port()`：绑定 `127.0.0.1:0` 拿到空闲端口后立即 drop —— 存在极小竞态窗口，子进程应尽快接管
- 窗口关闭：发送 `Stop`，300ms 后 `app.exit(0)`；`RunEvent::Exit` 时发送 `Shutdown` 并清理子进程
- 环境变量：`DSH_BIN`（dsh 入口）、`DSH_NODE`（Node 解释器）、`DSH_HOME`（透传给子进程）；数据目录为 `app_data_dir()/runtime`、`.../logs` 与 `.../config.json`
- `resolve_dsh` 的查找顺序：`dsh_bin`（config.json 优先于环境变量）→ runtime 内 `bin/dsh` → `node_modules/@deepseek-ai/dsh/lib/bin.js` → PATH → `~/.vite-plus/bin/dsh`

## 开发与验证

- 编译检查：`cd apps/shell/src-tauri && cargo check`；完整启动：仓库根 `yarn dev`
- 保持 `cargo fmt` 与 `cargo clippy` 无告警
- 跨平台注意：`open_external` 按目标平台分派（macOS `open` / Linux `xdg-open` / Windows `cmd start`）；`exit_summary` 在 unix 下读取 signal
