# AGENTS.md — packages/contracts（共享 IPC 类型）

前后端 IPC 契约的 TypeScript 类型与常量定义，供 `apps/shell`（启动页）与
`apps/control-center`（控制中心）共用。

## 内容

- `RuntimePhase` — phase 联合类型（snake_case，与 Rust `RuntimePhase` serde 一致）
- `RuntimeSnapshot` — `get_status` 返回（phase/message/url/dsh_installed/node_found/log_dir/logs）
- `DshConfig` — `get_config` / `set_config` 的配置结构（dsh_bin/dsh_node/dsh_home，null 表示未设置）
- `COMMANDS` / `EVENTS` — IPC 命令名与事件名字符串常量

## 同步规则

修改任何 IPC 契约时，必须同步四处：`packages/contracts/src/index.ts`、
Rust 端 `apps/shell/src-tauri/src/lib.rs`（及 config.rs）的 serde 类型、
`apps/shell/src/main.ts`、`apps/control-center/src/lib/ipc.ts`。
字段命名统一 snake_case（Rust serde `rename_all = "snake_case"`，TS 侧直接写 snake_case）。
