# AGENTS.md - packages/plugins/projects

`@dsh-desktop/plugin-projects`：提供壳侧侧边栏右键菜单所需的 dsh workspace/session loopback 端点。

## 职责

- 注册 `/dsh-desktop/workspaces`、`/dsh-desktop/sessions`、`/dsh-desktop/workspaces/action` 三个 host 端点。
- 端点只接受来自当前 dsh web loopback origin 且携带 `x-dsh-desktop-token` 的请求。
- Rust 启动 dsh 时注入 `DSH_DESKTOP_HOST_TOKEN`；壳注入脚本从 URL `dsh_desktop_token` 读取并在 fetch 时携带。
- 所有 `webServer.register` 返回的 disposer 收集进 `sctx.effect`，插件卸载时全部执行。

## 维护约定

- 不新增任意 loopback 可信请求；新端点必须走同一 token + origin 校验。
- 修改端点或令牌协议时同步更新 `apps/shell/src-tauri/src/lib.rs`、`apps/shell/src-tauri/AGENTS.md` 与本文件。
- `lib/` 为构建产物，不手改。
