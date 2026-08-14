import { Context } from "@deepseek-ai/cordis";

// 桥接 Tauri 壳能力的 dsh 插件（骨架）。
// 目标：让 dsh web 内的插件/页面能调用 Tauri 壳能力——经 packages/contracts
// 声明的 IPC 命令 + tauri.conf.json 端口白名单（见 docs/plugin-tauri-boundary.md）。
export const name = "bridge";

export function apply(ctx: Context) {
  // TODO: 调用壳能力最小切片（如 pick_directory），契约先在 packages/contracts 声明。
  // 骨架阶段不注册事件；实际开发时按 cordis 的 Events 接口扩展。
  void ctx;
}
