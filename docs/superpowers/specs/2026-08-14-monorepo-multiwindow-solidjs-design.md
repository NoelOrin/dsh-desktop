# DSH Desktop Monorepo 改造 + 多窗口 + SolidJS 控制中心 — 设计文档

- 日期：2026-08-14
- 分支：feature/window
- 状态：已获用户确认的设计

## 1. 背景与目标

将现有的 Tauri 2 单窗口应用（dsh-desktop）改造为 Yarn 4 workspaces monorepo，支持多窗口：

- **main 窗口**：dsh 即主窗口内容——启动页完成 dsh 检测/安装后，导航到 dsh web UI
- **control 窗口**（新）：SolidJS SPA 控制中心，按需打开（菜单项 + 全局快捷键），包含仪表盘（dsh 状态/日志/控制）、环境设置（可编辑并持久化）、辅助工具入口

统一约束：**所有前端包使用 Vite 8**（已确认 vite 8.2.1 为 latest；vite-plugin-solid 2.11.14 peer 依赖支持 vite ^8.0.0、solid-js ^1.7.2）。

## 2. 目录结构

```text
dsh-desktop/
├── package.json              # workspaces: ["apps/*", "packages/*"]，根脚本编排
├── apps/
│   ├── shell/                # Tauri 壳（现项目主体迁移至此）
│   │   ├── src/              # 启动页：vanilla JS → TypeScript（逻辑不变）
│   │   ├── src-tauri/        # Rust 后端（lib.rs / Cargo.toml / tauri.conf.json 迁移）
│   │   ├── vite.config.ts    # Vite 8
│   │   └── package.json      # @dsh-desktop/shell
│   └── control-center/       # SolidJS 控制中心（新）
│       ├── src/              # SolidJS SPA：仪表盘 / 设置 / 工具 三 Tab
│       ├── vite.config.ts    # Vite 8 + vite-plugin-solid
│       └── package.json      # @dsh-desktop/control-center
└── packages/
    └── contracts/            # 共享 IPC 类型（TS）
        └── src/index.ts      # RuntimePhase / RuntimeSnapshot / DshConfig 等
```

## 3. 构建与开发模式（全 Vite 8）

- 两个前端包均使用 Vite 8。
- 构建输出共用根 dist/：
  - shell 构建到 dist/（build.outDir 指向 ../../dist）
  - control-center 构建到 dist/control-center/
- apps/shell/src-tauri/tauri.conf.json：frontendDist: "../../dist"。
- 窗口 URL：
  - main：默认（/index.html）
  - control：/control-center/index.html
- 开发模式：根 dev:web 并发起两个 Vite dev server：
  - shell @ http://localhost:5173
  - control-center @ http://localhost:5174
- Rust 在 debug_assertions 下把 control 窗口导航到 http://localhost:5174，release 使用配置的相对 URL /control-center/index.html（由 Tauri 按 frontendDist 解析）。

### 根脚本（package.json）

| 脚本 | 命令 | 说明 |
| --- | --- | --- |
| dev | tauri dev（在 apps/shell） | 完整开发（含 dev:web） |
| build | tauri build（在 apps/shell） | 打包 |
| dev:web | 并发启动两个 Vite dev server | 前端开发 |
| build:web | 并发构建两个前端到 dist/ | beforeBuildCommand 调用 |

> Tauri 的 beforeDevCommand / beforeBuildCommand 以 tauri.conf.json 所在目录（apps/shell）为 cwd 执行，
> 因此 **apps/shell 的 package.json 负责定义 dev:web / build:web 编排脚本**（用 concurrently 并发
> 启动/构建本包与 @dsh-desktop/control-center 两个工作区），根 package.json 的对应脚本委托给
> apps/shell（yarn workspace @dsh-desktop/shell run dev:web 等）。

## 4. 窗口管理

- **main 窗口**：行为保持不变——启动页检测/安装 dsh，就绪后 navigate 到 http://127.0.0.1:<port>（dsh web）。
- **control 窗口**：
  - 在 tauri.conf.json 的 app.windows 中定义（label: control），启动即创建但 visible: false。
  - 调起方式（两者共用 Rust 端 open_control_window()）：
    - 应用菜单项“控制中心”，加速键 CmdOrCtrl+Shift+C（Tauri 2 内置 Menu API）
    - 全局快捷键（tauri-plugin-global-shortcut）注册 CmdOrCtrl+Shift+C
  - open_control_window() 行为：窗口存在 → show() + set_focus()；已可见 → 仅 set_focus()。
  - 窗口关闭（CloseRequested）→ 隐藏而非销毁，保持单实例。
- apps/shell/src-tauri/capabilities/default.json：windows 增加 "control"，使其可调用 IPC 命令与核心权限。

## 5. Rust 后端改动

### 5.1 DshConfig 与持久化

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct DshConfig {
    dsh_bin: Option<String>,   // DSH_BIN
    dsh_node: Option<String>,  // DSH_NODE
    dsh_home: Option<String>,  // DSH_HOME
}
```

- 配置文件：app_data()/config.json。
- 生效配置解析优先级：**config.json → 环境变量 → PATH 检测**。

### 5.2 新增 IPC 命令

| 命令 | 入参 | 返回 | 说明 |
| --- | --- | --- | --- |
| get_config | 无 | DshConfig | 返回当前生效配置（config + env 合并结果） |
| set_config | DshConfig | Result<(), String> | 写 config.json |

- 保存后由前端提示“重启 dsh 生效”，并引导调用现有 restart 命令。
- resolve_dsh / resolve_node / resolve_npm 改为优先读取 DshConfig，其次环境变量，最后 PATH 检测（现逻辑迁移）。

## 6. contracts 共享包（packages/contracts）

- TypeScript 类型与常量：
  - RuntimePhase（union，snake_case 与 Rust serde 一致）
  - RuntimeSnapshot（phase / message / url / dsh_installed / node_found / install_dir / log_dir / logs）
  - DshConfig（dsh_bin / dsh_node / dsh_home）
  - 命令名与事件名常量：get_status / install_dsh / restart / open_log_directory / get_config / set_config，dsh-status / dsh-log
- 消费方：shell（启动页）与 control-center。
- Rust serde 类型与 contracts 手工保持同步；同步契约写入 src-tauri/AGENTS.md。

## 7. 控制中心 UI（SolidJS SPA）

- 技术栈：SolidJS ^1.7.2 + vite-plugin-solid + Vite 8 + TypeScript。
- 三 Tab（无路由库，Tab 切换状态即可）：
  - **仪表盘**：dsh 状态（phase/消息/URL）、实时日志区（自动滚动）、按钮：安装 DSH（仅 missing 时）、重试（仅 failed 时）、打开日志目录（仅 failed 时）。
  - **设置**：DSH_BIN / DSH_NODE / DSH_HOME 表单（get_config 回填，set_config 保存），保存后提示重启 dsh 生效。
  - **工具**：辅助入口占位（列表项，后续扩展）。
- 复用现有事件/命令：dsh-status / dsh-log 事件、get_status / install_dsh / restart / open_log_directory。

## 8. 启动页（apps/shell/src）迁移

- vanilla JS → TypeScript，行为与 DOM 结构保持不变。
- 消费 @dsh-desktop/contracts 的类型（RuntimeSnapshot 等）。
- 样式（style.css）与 index.html 迁移至 apps/shell 根。

## 9. 依赖与配置变更清单

- 根 package.json：workspaces、根脚本（dev / build / dev:web / build:web）、devDependency：concurrently（或等效并发工具）。
- apps/shell：vite@^8、typescript、@tauri-apps/api@^2、@dsh-desktop/contracts。
- apps/control-center：vite@^8、vite-plugin-solid@^2.11、solid-js@^1.7、@tauri-apps/api@^2、@dsh-desktop/contracts。
- src-tauri：新增 tauri-plugin-global-shortcut；capabilities 增加 control 窗口。
- .gitignore：dist/ 已在列；新增 apps/*/dist（如需要）。

## 10. 验证

- yarn build:web 双前端构建成功，产物位于 dist/ 与 dist/control-center/。
- cargo check / cargo clippy / cargo fmt 无告警。
- 手动验证（yarn dev）：
  - main 窗口启动页 → dsh 就绪 → 导航到 dsh web（行为不变）
  - 菜单项 / 快捷键可打开 control 窗口，重复调用只聚焦不重复创建
  - 设置保存后 config.json 生成，重启后配置生效
  - 控制中心日志与状态随事件实时更新

## 11. 风险与回退

- Tauri 2 多窗口 + 双 Vite dev server：debug 导航逻辑（control → :5174）需实测；若 Tauri 不保留绝对 URL，则改为 setup 中 navigate 或运行时 set_url。
- 全局快捷键在部分 Linux 桌面环境可能不生效：菜单项始终可用作回退。
- tauri-plugin-global-shortcut 需要注册到 capability 权限。
- config.json 优先级高于环境变量：需在文档/AGENTS.md 中明确说明，避免误判。