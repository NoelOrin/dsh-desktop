# 插件域与 Tauri 壳域的边界

> 目的：划清 **dsh 插件**（运行在 dsh web 子进程内、由 dsh 的 cordis 体系管理）与
> **Tauri 桌面壳能力**（本仓库 Rust 后端 + 本地窗口）之间的职责边界，避免两端重复实现
> 或互相耦合，并为“新功能该放哪边”提供可执行的判断依据。
>
> 一句话概括：**dsh 插件扩展 dsh 产品，Tauri 壳能力服务桌面外壳；跨界数据只通过
> `packages/contracts` 声明的 IPC 契约对话。`packages/plugins` 是本仓库的 dsh 插件容器
> （桥接 Tauri 壳能力的插件 + 自定义插件），与契约相互独立。**

## 1. 背景与目标

DSH Desktop 是 dsh 的桌面套壳：Rust 后端拉起 `dsh web` 子进程，就绪后主窗口整页
导航进 Harness Web UI。因此产品天然存在**两层扩展点**：

- **dsh 本身**就是可扩展的——所有功能都是 cordis 插件（模型适配、工具注册、会话、
  agent 循环、Web UI 组件……），用户通过 `dsh plugin` / profile / patch 层装配。
- **桌面壳**也提供能力——进程生命周期、窗口、原生交互、配置持久化、打包分发。

两层都很容易做出“看起来能实现同一件事”的错觉（例如 Tauri 也能执行命令、插件也能
开服务器），于是出现重复实现与耦合。本文档把这些边界写死，让后续开发有据可依。

## 2. 两个扩展域

### 2.1 dsh 插件域

| 项 | 说明 |
| --- | --- |
| 运行位置 | `dsh web` 子进程（Node 进程）内 |
| 框架 | `@deepseek-ai/cordis`（v4），一切皆插件 |
| 形态 | npm 包，`package.json` 声明 `dsh.bundle.patch` 指向一份 `cordis.patch.yml` |
| 装配 | profile（`$DSH_HOME/profiles/<name>`）的 `dsh.profile.bundles` 有序列表 + 用户 patch 层 |
| 安装/管理 | `dsh plugin --profile <name> <pnpm args>`（转发给 pnpm）；web 设置里的插件管理 |
| 扩展什么 | agent 行为、工具、模型适配、会话处理、系统提示、Web UI 组件、设置页、命令行列 |
| 本仓库载体 | `packages/plugins`（插件容器）：`bridge/` 桥接 Tauri 壳能力、`hello/` 自定义示例，每个子目录一个 dsh 插件 |

**红线**：插件运行在 `http://127.0.0.1:<port>` 的远端页面/独立 Node 进程里，
默认**不接触**桌面壳任何能力（见 §5）。唯一的例外是本仓库自己的桥接插件
（`packages/plugins`，见 §6），且只通过受控桥接，不是全量开放。

### 2.2 Tauri 壳域

| 项 | 说明 |
| --- | --- |
| 运行位置 | Rust 后端（`apps/shell/src-tauri`）+ 两个本地 WebView 窗口 |
| 职责 | 检测/安装/拉起/监控/停止 dsh 子进程；窗口/菜单/全局快捷键；应用数据目录；`config.json`；原生交互（打开日志目录）；打包分发 |
| 已声明能力 | capabilities `core:default` + `global-shortcut:default`（覆盖 main/control 窗口） |
| 实现契约 | `get_status` / `install_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config`（类型与常量见 `packages/contracts`） |
| 配置 | `config.json`（应用数据目录）→ 环境变量（`DSH_BIN`/ `DSH_NODE`/ `DSH_HOME`）→ PATH 检测 |

**红线**：Tauri 壳不实现任何 dsh 产品功能——不做 agent、不做工具、不接 LLM、
不解析 dsh 的配置树、**不重复实现 dsh 的插件管理**。这些永远是 dsh 自己的事。

## 3. 归属判断规则

给新功能定归属时，按顺序回答三个问题：

1. **它扩展的是 dsh 产品的行为/界面吗？**（agent 怎么思考、能调什么工具、Web UI 长什么样、
   插件管理/清单/设置页）
   → 是 → **dsh 插件域**，交给 dsh 的 cordis 体系（bundle + patch + `dsh plugin`）。
2. **它需要系统原生能力或桌面壳生命周期吗？**（窗口、菜单、快捷键、原生对话框、
   应用数据目录、进程管理、打包分发）
   → 是 → **Tauri 壳域**，交给 Rust 后端 + 本地窗口。
3. **两端需要共享数据吗？**（状态、配置、日志、事件）
   → 是 → **只通过 `packages/contracts` 的 IPC 契约**，两端各自实现，见 §4。

### 常见反例（禁止项）

| ❌ 不要做 | 原因 |
| --- | --- |
| 在 Tauri 侧重复实现 agent 循环 / 工具注册 / LLM 调用 | 那是 dsh 插件的职责，壳只负责拉起进程 |
| 在 Tauri 侧解析 dsh 的 profile / patch 配置树 | 配置树归 dsh 管，壳应通过 `dsh` 命令/契约获取结果 |
| 在 Tauri 侧重复实现 dsh 的插件管理（清单/安装/设置 UI） | dsh 的插件管理**本身就是 cordis 插件**（`dsh-host-plugin-inventory`、`dsh-client-ui-settings-plugin-inventory` 等，由 `dsh-web-app` 装配），壳侧直接消费 dsh web 现成界面 |
| 在 dsh 插件里直接读 `config.json`、开原生窗口/对话框 | 那是壳的能力，插件默认拿不到；只能通过本仓库桥接插件 + 已声明命令 + 端口白名单（见 §5/§6） |
| 前端直接 import 另一个模块的内部实现 | native 跨界数据走 `packages/contracts` 命令/事件；桥接命令由 `packages/plugins/bridge` 自持 |
| 新增 IPC 却不改 capabilities 声明 | 权限收口，新增能力必须同步声明 |

## 4. 边界缝：`packages/contracts` 与 IPC

壳侧两端（Rust 后端 ↔ 本地窗口）的唯一合法通道是 **`@dsh-desktop/contracts` 中声明的
IPC 命令与事件**。本包只负责 **native 内容**：dsh 运行态的检测/安装/配置/日志等壳侧命令
与事件。所有 native 跨界数据都必须是契约的一部分，两端各自实现，不允许直接 import
对方内部或另开通道。

> **桥接不经过本契约**：dsh 插件调用 Tauri 壳能力的桥接命令，由 `packages/plugins/bridge`
> 自持契约，Rust 侧在 `capabilities/bridge.json` 声明（见 §5/§6）。`packages/contracts`
> 只负责 native 内容，不负责桥接。

修改任何 IPC 契约时，必须同步四处（已有约定，此处重申为边界规则）：

1. `packages/contracts/src/index.ts` — TS 类型与常量（唯一源头）
2. `apps/shell/src-tauri/src/lib.rs`（及 `config.rs`）— Rust serde 类型
3. `apps/shell/src/main.ts` — main 窗口前端
4. `apps/control-center/src/lib/ipc.ts` — control 窗口前端

字段命名统一 snake_case（Rust serde `rename_all = "snake_case"`，TS 侧直接写
snake_case）。

**新增 IPC 能力**必须同时修改 `apps/shell/src-tauri/capabilities/default.json`
权限声明——这是壳侧能力的收口点，任何新命令/事件没有权限声明即为无效。

## 5. 安全现状与受控桥接

### 现状（刻意隔离）

- 主窗口就绪后整页导航到 `http://127.0.0.1:<port>`，dsh web 是**远端 origin**。
- Tauri 2 的 IPC 只对本地窗口页面默认开放；远端 origin 没有 `dangerousRemoteDomainIpcAccess`
  声明就拿不到 `window.__TAURI_INTERNALS__`。
- capabilities 仅覆盖 main/control 两个本地窗口，**dsh web（及其插件）默认永远拿不到
  Tauri 能力**。

这是**有意为之**的安全边界：第三方插件运行在 dsh 的 Node 进程和远端页面里，不授予它们
桌面壳权限，插件就永远无法碰 `config.json`、窗口、宿主资源。

### 受控桥接（本仓库桥接插件的落地方式）

本仓库自己的桥接插件要调用 Tauri 壳能力，按以下顺序收紧，而不是一刀切开放：

1. 在桥接插件（`packages/plugins/bridge`）内定义**最小化桥接命令**（例如 `pick_directory`），
   **契约由桥接插件自持**，Rust 侧在 `capabilities/bridge.json` 声明实现与权限——
   不写进 `packages/contracts`（contracts 只负责 native 内容，不负责桥接）。
2. 在 `tauri.conf.json` 的 `app.security.dangerousRemoteDomainIpcAccess` 里
   **只对该 loopback 端口白名单**，并在对应 capability 里只授予桥接需要的命令。
3. 桥接命令必须是**能力的最小切片**（一次只暴露一个动作），不允许把整个 `core:default`
   或 `shell` 权限开放给远端。

> 除本仓库桥接插件外的一切 dsh 插件：**不开放，不预埋**。

## 6. `packages/plugins` 的定位

`packages/plugins` 是 **dsh 插件容器**——不是 IPC 契约（契约在 `packages/contracts`）。
下面可放多个插件独立开发，每个子目录一个 cordis 插件（bundle）。

| | `packages/plugins/`（容器） |
| --- | --- |
| 归属 | **dsh 插件域**（本仓库维护，交付给 dsh 装配） |
| 结构 | `bridge/`（桥接 Tauri 壳能力）、`hello/`（自定义示例）；新增插件在容器下建子目录即可 |
| bridge 唯一职责 | 让 dsh 插件/页面能调用 Tauri 壳能力（桥接命令契约由 bridge 自持 + §5 端口白名单） |
| 消费者 | dsh web 内的插件/页面 |
| 依赖方向 | 依赖 `@deepseek-ai/cordis` 等 dsh 生态包；**不依赖 `@dsh-desktop/contracts`**（native 契约与桥接契约分离） |
| 安装 | 每个插件 `dsh plugin --profile web add <包>`（或随桌面应用自动装配） |

**约定**：

- 本包**不做业务**：它只负责“能调用 Tauri 能力”这件事本身；具体能力（读配置、开目录
  等）由壳侧已声明的命令实现，桥接层只做转发与类型化。
- **插件管理不是本包的职责**：dsh 的插件管理（清单、安装/卸载 UI、设置页）已经是
  dsh 现成的 cordis 插件（`dsh-host-plugin-inventory` /
  `dsh-client-ui-settings-plugin-inventory` / `dsh-client-ui-settings-plugins`），
  由 `dsh-web-app` 装配，壳侧与本包都不重复实现。
- 若未来有第二个需要壳能力的 dsh 插件，能力暴露仍走 §5 的受控桥接流程，不因“是自家的”
  而放宽。

## 7. 新增能力走哪条路（变更流程）

拿到一个新需求时，按 §3 判断后走对应流程：

**A. 扩展 dsh 产品行为（含插件管理）**
→ 开发 cordis 插件包（`dsh.bundle` + `cordis.patch.yml`），用
`dsh plugin --profile web add <包>` 安装到用户 profile；壳侧无改动。

**B. 扩展桌面壳能力**
→ Rust 后端新增逻辑 + `packages/contracts` 声明契约 + capabilities 声明权限 +
main/control 前端按需调用；dsh 侧无改动。

**C. 两端联动（状态/事件/配置）**
→ 只允许通过 `packages/contracts` 已声明（或本次新增并同步四处）的 IPC 命令/事件；
两端各自实现，禁止互相 import。

**D. 让 dsh 插件用到壳能力**
→ 通过 `packages/plugins` 的桥接插件实现：壳侧先落最小命令（B 流程），再走
§5 的端口白名单；除桥接插件外不开放。

## 8. 参考资料

- dsh 官方插件开发文档：
  [架构](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)、
  [cordis 入门](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/cordis-primer.md)、
  [打包安装插件](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/develop/basic/publish.md)
- dsh 插件管理（均为 cordis 插件，由 `dsh-web-app` 装配）：
  `@deepseek-ai/dsh-host-plugin-inventory`（宿主侧只读清单）、
  `@deepseek-ai/dsh-client-ui-settings-plugin-inventory` /
  `@deepseek-ai/dsh-client-ui-settings-plugins`（Web 设置页插件管理 UI）
- 本地 dsh 安装：`lib/plugin-9h8shc4d.js`（`dsh plugin` 实现）、
  `node_modules/@deepseek-ai/dsh-app-boot/lib/types/profile.d.ts`（manifest 类型）、
  `node_modules/@deepseek-ai/dsh-base/cordis.patch.yml`（bundle patch 样例）
- 本仓库契约：`packages/contracts/src/index.ts`；
  壳能力：`apps/shell/src-tauri/capabilities/default.json`
