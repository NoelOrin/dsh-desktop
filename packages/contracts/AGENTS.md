# AGENTS.md — packages/contracts（共享 IPC 类型）

前后端 IPC 契约的 TypeScript 类型与常量定义，供 `apps/shell`（启动页）与
`apps/control-center`（控制中心）共用。

## 内容

- `RuntimePhase` — phase 联合类型（snake_case，与 Rust `RuntimePhase` serde 一致）
- `RuntimeSnapshot` — `get_status` 返回（phase/message/url/dsh_installed/node_found/log_dir/logs）
- `DshConfig` — `get_config` / `set_config` 的配置结构（dsh_bin/dsh_node/dsh_home，null 表示未设置）
- `COMMANDS` / `EVENTS` — IPC 命令名与事件名字符串常量（命令含 `open_external`；事件含 `dsh-file-drop` / `dsh-theme`）

## 边界

- 本包只负责 **native（桌面壳原生）IPC 契约**：Rust 后端与本地窗口（main/control）之间的命令与事件
- **桥接不属于本包**：dsh 插件调用 Tauri 壳能力的桥接命令契约，由 `packages/plugins/bridge` 自持，
  Rust 侧在 `apps/shell/src-tauri/capabilities/bridge.json` 声明，不并入本包

## 同步规则

修改任何 IPC 契约时，必须同步四处：`packages/contracts/src/index.ts`、
Rust 端 `apps/shell/src-tauri/src/lib.rs`（及 config.rs）的 serde 类型、
`apps/shell/src/main.ts`、`apps/control-center/src/lib/ipc.ts`。
字段命名统一 snake_case（Rust serde `rename_all = "snake_case"`，TS 侧直接写 snake_case）。
