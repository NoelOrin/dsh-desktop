# AGENTS.md — src-tauri（Tauri 2 + Rust 后端）

DSH Desktop 的 Rust 后端。职责：检测 / 一键安装 `dsh`、以子进程方式拉起并监控 `dsh web`、就绪后把主窗口导航到 Web UI，同时将状态与日志实时推送给前端。

## 文件地图

- `src/lib.rs` — 全部后端逻辑：状态机、进程管理、IPC 命令
- `src/main.rs` — 仅入口：调用 `dsh_desktop_lib::run()`
- `Cargo.toml` — 依赖：`tauri 2`、`serde`、`serde_json`；lib 名为 `dsh_desktop_lib`
- `tauri.conf.json` — 窗口配置（label `main`）、构建前后命令、bundle 目标
- `capabilities/default.json` — IPC 权限（当前仅 `core:default`）
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

- 命令：`get_status` / `restart` / `install_dsh` / `open_log_directory`
- 事件（向前端 emit）：`dsh-status`（RuntimeSnapshot）、`dsh-log`（单行文本）
- 修改契约时，必须同步更新 `../src/AGENTS.md` 的通信契约表

## 关键行为与陷阱

- 常量：`READY_TIMEOUT=120s`、`POLL_INTERVAL=250ms`、`MAX_LOGS=500`（环形日志）
- `reserve_port()`：绑定 `127.0.0.1:0` 拿到空闲端口后立即 drop —— 存在极小竞态窗口，子进程应尽快接管
- 窗口关闭：发送 `Stop`，300ms 后 `app.exit(0)`；`RunEvent::Exit` 时发送 `Shutdown` 并清理子进程
- 环境变量：`DSH_BIN`（dsh 入口）、`DSH_NODE`（Node 解释器）、`DSH_HOME`（透传给子进程）；数据目录为 `app_data_dir()/runtime` 与 `.../logs`
- `resolve_dsh` 的查找顺序：`DSH_BIN` → runtime 内 `bin/dsh` → `node_modules/@deepseek-ai/dsh/lib/bin.js` → PATH → `~/.vite-plus/bin/dsh`

## 开发与验证

- 编译检查：`cd src-tauri && cargo check`；完整启动：仓库根 `yarn dev`
- 保持 `cargo fmt` 与 `cargo clippy` 无告警
- 跨平台注意：`open_external` 按目标平台分派（macOS `open` / Linux `xdg-open` / Windows `cmd start`）；`exit_summary` 在 unix 下读取 signal
