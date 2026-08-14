# AGENTS.md — packages/plugins（dsh 插件容器）

dsh 插件的开发容器：**每个子目录一个 dsh 插件**（cordis bundle），各自独立开发。
插件不是 IPC 契约——契约在 `packages/contracts`。

## 现有插件

- `bridge/`（`@dsh-desktop/plugin-bridge`）— **桥接 Tauri 壳能力的双面插件**（host + client）：
  让 dsh web 内的插件/页面能调用 Tauri 壳能力。**桥接命令契约由本插件自持**
  （Rust 侧在 `apps/shell/src-tauri/capabilities/bridge.json` 声明），不并入
  `packages/contracts`——contracts 只负责 native 内容（边界见 `docs/plugin-tauri-boundary.md`）。
  host 侧注册 `desktop` settings 命名空间（`autostart` 默认关闭），client 侧在 dsh WebUI
  设置面板渲染“开机自启”开关（详见 `./bridge/AGENTS.md`）
- `hello/`（`@dsh-desktop/plugin-hello`）— **自定义插件示例**（骨架），演示开发结构

## 新增插件

1. 在 `packages/plugins/` 下建子目录（如 `custom-foo/`），包名 `@dsh-desktop/plugin-<name>`
2. 照 `hello/` 的模板：`package.json`（含 `dsh: { bundle: { patch: "./cordis.patch.yml" } }`、
   依赖 `@deepseek-ai/cordis`）、`tsconfig.json`、`cordis.patch.yml`、`src/index.ts`
3. 无需改根 workspaces——`packages/plugins/*` 已通配
4. `yarn install` 注册后，根 `yarn typecheck` 自动覆盖

## 插件格式要点（dsh bundle）

- `package.json` 声明 `dsh.bundle.patch` 指向 `cordis.patch.yml`
- `cordis.patch.yml` 是 YAML 配置层：`- insert:` 向 profile 插入插件行（`id` / `name`）
- 插件源码：`export const name` + `export function apply(ctx, config)`；事件名必须是
  cordis `Events` 接口里的键（骨架阶段不要注册未声明的事件）
- 安装：**内嵌装配（主交付路径）**——`yarn build:plugins` 编译打包进 Tauri resources，桌面应用启动时自动复制进 `$DSH_HOME/profiles/node_modules/@dsh-desktop/<name>/` 并以 `--patch` overlay 挂载（见 `docs/plugin-tauri-boundary.md` §6）；开发期亦可 `dsh plugin --profile web add <包>` 单独安装
- 构建与装配：`yarn build:plugins`（根脚本）经 tsdown 编译各插件——host ESM 产出 `lib/index.js`，可选 client UMD 产出 `lib/client.js`——并把自包含 dist 包（`package.json` 白名单字段 + `cordis.patch.yml` + `lib/`）装配进 `apps/shell/src-tauri/resources/plugins/<name>/`；桌面应用启动时 Rust 侧自动装配进 dsh 的 profile 模块兜底目录并以 `--patch` overlay 挂载（机制见 `docs/plugin-tauri-boundary.md` §6）
- 注意：dsh 从 Git 安装时跑 `prepare` 而非 `build`，TS 插件需自包含构建产物
  （见官方 publish 文档）

## 边界

- 本包不做业务：只负责插件自身逻辑；桥接命令契约由 `bridge/` 插件自持（不并入 `packages/contracts`，contracts 只负责 native）
- 插件管理（清单 / 安装 UI）是 dsh 现成的 cordis 插件（`dsh-host-plugin-inventory` 等），
  不在本包重复实现
- `@deepseek-ai/cordis` 等新版本发布不足 1 天会被 yarn 的 npmMinimalAgeGate 隔离；
  已在仓库 `.yarnrc.yml` 对 `@deepseek-ai/*` 定向放行
