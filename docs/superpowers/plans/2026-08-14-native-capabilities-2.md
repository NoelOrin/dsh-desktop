# 原生能力增强（二期：窗口状态/深链/自动更新/开机自启/快捷键/退出确认）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 DSH Desktop 补上六项原生能力：窗口状态记忆、深链 `dsh-desktop://`、自动更新、开机自启（经 dsh 插件在 WebUI 设置面板提供开关，默认关闭）、自定义全局快捷键（桥接层暴露）、托盘退出二次确认。

**Architecture:** 能力分三层落地：Tauri 壳侧（Rust 插件注册 + 最小命令 + capabilities 白名单）、桥接层（`window.__DSH_DESKTOP__` 扩展）、dsh 插件侧（`packages/plugins/bridge` 的 host settings 注册 + client 设置面板 UI）。开机自启按用户要求**不做在壳侧控制中心**，而是做成 dsh 插件在 dsh WebUI 设置面板里提供开关，默认关闭。

**Tech Stack:** Tauri 2.11.5（Rust）+ tauri-plugin-window-state/deep-link/updater/autostart + SolidJS 控制中心 + dsh（cordis）插件生态（schemastery schema + settings 服务 + client slots）。

**Spec:** 本计划实现用户在会话中确认的需求清单（无独立 spec 文件）；功能边界与分层遵循 `docs/plugin-tauri-boundary.md`（§5 受控桥接、§6 bridge 插件定位）。每项功能的"需求/行为"在各任务开头内联说明。

## Global Constraints

- Tauri 2.11.5；Rust 1.97；前端 TypeScript strict；所有 user-facing 文案为中文
- 新增 IPC 能力必须同步 `packages/contracts/src/index.ts`、Rust serde 类型、`apps/shell/src/main.ts`、`apps/control-center/src/lib/ipc.ts`，以及 `apps/shell/src-tauri/capabilities/*.json` 权限声明
- 桥接能力只经 `capabilities/bridge.json` 的 remote 白名单（`http://127.0.0.1:*`）开放最小命令切片；不开放 `core:default` 给远程
- 应用命令 ACL 由 `apps/shell/src-tauri/build.rs` 的 `AppManifest::commands` 生成（权限 id 形如 `allow-get-status`，下划线转连字符）
- `tauri-plugin-single-instance` 必须最先注册；本次需为其启用 `deep-link` feature 以支持 Windows/Linux 深链转发
- 自动更新需要 GitHub Release 签名密钥（`TAURI_SIGNING_PRIVATE_KEY` CI secret）与 `latest.json` 清单，签名产物须与安装包一起上传
- 保持 `cargo fmt` / `cargo clippy` 零警告；`yarn typecheck` 全绿；提交遵循 Conventional Commits（中文 scope 注释）
- `dist/`、`gen/`、`target/` 为生成产物，勿手改

---

### Task 1: 壳侧注册 window-state / deep-link / updater / autostart 四个插件

**Files:**
- Modify: `apps/shell/src-tauri/Cargo.toml`
- Modify: `apps/shell/src-tauri/src/lib.rs`（插件注册 + capabilities 无关部分）
- Modify: `apps/shell/src-tauri/tauri.conf.json`（deep-link 配置；updater 的 pubkey 用占位值待 Task 4 替换）

**Interfaces:**
- Consumes: 现有 `tauri::Builder` 链、`build.rs` 的 `AppManifest`
- Produces: 四个插件注册到 `run()`；`tauri.conf.json` 的 `plugins.deep-link` 段；后续任务可调用插件 API（`AppHandleExt` / `DeepLinkExt` / `UpdaterExt` / `ManagerExt`）

- [ ] **Step 1: Cargo.toml 启用 single-instance 的 deep-link feature**

`apps/shell/src-tauri/Cargo.toml`：

```toml
tauri-plugin-single-instance = { version = "2", features = ["deep-link"] }
```

（window-state/deep-link/updater/autostart 四个依赖已在本会话前一步加入。）

- [ ] **Step 2: lib.rs 注册四个插件**

在 `apps/shell/src-tauri/src/lib.rs` 的 `run()` 中，紧随 `tauri_plugin_dialog::init()` 之后（保持 single-instance 最先）加入：

```rust
.plugin(tauri_plugin_window_state::Builder::default().build())
.plugin(tauri_plugin_deep_link::init())
.plugin(tauri_plugin_autostart::init(
    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
    Some(&["--autostart"]),
))
.plugin(tauri_plugin_updater::Builder::new().build())
```

> updater 的 pubkey 与 endpoints 由 tauri.conf.json 的 `plugins.updater` 段提供，`Builder::new().build()` 的 setup 会自动读取 `api.config()`（见 tauri-plugin-updater 源码 build 实现）。若 Task 1 Step 3 的 pubkey 仍是占位字符串，编译期会报 pubkey 缺失——此时先注释 updater 插件行，Task 4 完成密钥生成后启用。

- [ ] **Step 3: tauri.conf.json 配置 deep-link 与 updater 骨架**

`apps/shell/src-tauri/tauri.conf.json` 顶层新增 `plugins` 段：

```json
"plugins": {
  "deep-link": {
    "desktop": { "schemes": ["dsh-desktop"] }
  },
  "updater": {
    "endpoints": [
      "https://github.com/NoelOrin/dsh-desktop/releases/latest/download/latest.json"
    ],
    "pubkey": "REPLACE_WITH_GENERATED_PUBKEY"
  }
}
```

（`REPLACE_WITH_GENERATED_PUBKEY` 在 Task 4 Step 1 用 `tauri signer generate` 生成的公钥替换；deep-link 配置即刻生效。）

- [ ] **Step 4: 编译验证**

Run: `cd apps/shell/src-tauri && cargo check`
Expected: 编译通过（updater pubkey 若报错，先注释 updater 插件行，Task 4 再启用）。

- [ ] **Step 5: 提交**

```bash
git add apps/shell/src-tauri/Cargo.toml apps/shell/src-tauri/Cargo.lock apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/tauri.conf.json
git commit -m "feat: 注册 window-state/deep-link/updater/autostart 插件"
```

---

### Task 2: 窗口状态记忆（window-state）

**需求/行为:** 应用重启后，main/control 窗口记住上次的大小、位置、最大化/最小化状态。由于关闭到托盘不销毁窗口，状态在 `RunEvent::Exit` 时由插件自动保存。

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`
- Modify: `apps/shell/src-tauri/capabilities/default.json`（可选，前端 JS 若用 save/restore 才需要；本任务只用 Rust 侧自动行为）

**Interfaces:**
- Consumes: Task 1 注册的 `tauri_plugin_window_state`
- Produces: 窗口状态自动保存/恢复（无新增 IPC）

- [ ] **Step 1: 确认插件默认行为覆盖关闭到托盘场景**

window-state 插件在 `WindowEvent::CloseRequested`（真正销毁窗口）与 `RunEvent::Exit` 时保存状态。本应用 main 窗口关闭被拦截为隐藏（不销毁），因此依赖 `RunEvent::Exit` 路径——托盘"退出"调用 `app.exit(0)` 会触发 `RunEvent::Exit`，插件自动保存。

- [ ] **Step 2: 编译验证**

Run: `cd apps/shell/src-tauri && cargo check && cargo test`
Expected: 通过；现有 5 个测试全绿。

- [ ] **Step 3: 提交**

```bash
git add apps/shell/src-tauri/src/lib.rs
git commit -m "feat: 窗口状态记忆（window-state 插件自动保存/恢复）"
```

---

### Task 3: 深链 dsh-desktop://

**需求/行为:** 注册 `dsh-desktop://` scheme。应用运行时收到深链 URL（如 `dsh-desktop://open/workspace/xxx`），壳侧解析后 emit `dsh-deeplink` 事件，桥接层向 dsh web 页面暴露 `onDeepLink(cb)` 订阅；dsh web 处于 ready 时由页面消费，未就绪时暂存到 Inner，就绪后补发。Windows/Linux 上 URL 作为 CLI 参数由 single-instance（deep-link feature）转发给已运行实例。

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`（`on_open_url` 回调 + Inner 增 `pending_deeplinks` + 事件 emit）
- Modify: `apps/shell/src-tauri/src/lib.rs` 的 `BRIDGE_SCRIPT`（`onDeepLink`）
- Modify: `apps/shell/src-tauri/capabilities/default.json` 与 `bridge.json`（无需新权限；事件经 core 已放行）
- Modify: `packages/contracts/src/index.ts`（EVENTS 增 `dshDeeplink`）
- Modify: `packages/plugins/bridge/src/index.ts`（`DshDesktopBridge` 增 `onDeepLink`）

**Interfaces:**
- Consumes: Task 1 注册的 `tauri_plugin_deep_link`（`DeepLinkExt::deep_link().on_open_url`）
- Produces: 事件 `dsh-deeplink`（payload: string，原始 URL）；桥接方法 `onDeepLink(cb: (url: string) => void): Promise<() => void>`

- [ ] **Step 1: Inner 增加待处理深链缓冲**

`lib.rs` 中 `Inner` 增加字段：

```rust
pending_deeplinks: VecDeque<String>,
```

并在 `Inner::default()` 派生下自然为空（`VecDeque` 已 derive Default）。

- [ ] **Step 2: 注册 on_open_url 回调**

`run()` 的 `setup` 中，拿到 `app_handle` 后注册（需在插件 setup 之后）：

```rust
use tauri_plugin_deep_link::DeepLinkExt;

let deep_app = app_handle.clone();
app_handle.deep_link().on_open_url(move |event| {
    // OpenUrlEvent::urls(self) 消费事件并返回 URL 列表（字段私有，必须走方法）
    for url in event.urls() {
        let url = url.to_string();
        let state = deep_app.state::<AppState>();
        {
            let mut inner = state.inner.lock().unwrap();
            inner.pending_deeplinks.push_back(url.clone());
        }
        // 立即转发给 dsh web 页面（若已就绪则页面消费；未就绪由 ready 路径补发）
        let _ = deep_app.emit("dsh-deeplink", url);
    }
});
```

- [ ] **Step 3: 就绪时补发未消费的深链**

在 `DshManager::run` 的 `Ready` 分支（`self.set_phase(...)` 与 `self.open_window(url)` 之间）加：

```rust
{
    let mut inner = self.inner.lock().unwrap();
    while let Some(link) = inner.pending_deeplinks.pop_front() {
        let _ = self.app.emit("dsh-deeplink", link);
    }
}
```

- [ ] **Step 4: BRIDGE_SCRIPT 增加 onDeepLink**

`BRIDGE_SCRIPT` 的对象里加：

```js
onDeepLink: function (cb) { return listen("dsh-deeplink", cb); },
```

- [ ] **Step 5: contracts 与 bridge 插件类型同步**

`packages/contracts/src/index.ts` 的 `EVENTS` 加 `dshDeeplink: "dsh-deeplink"`。

`packages/plugins/bridge/src/index.ts` 的 `DshDesktopBridge` 加：

```ts
onDeepLink(cb: (url: string) => void): Promise<() => void>;
```

- [ ] **Step 6: 编译 + 类型检查**

Run: `cd apps/shell/src-tauri && cargo check && cargo test` 与根 `yarn typecheck`
Expected: 全绿。

- [ ] **Step 7: 提交**

```bash
git add apps/shell/src-tauri/src/lib.rs packages/contracts/src/index.ts packages/plugins/bridge/src/index.ts
git commit -m "feat: 注册 dsh-desktop:// 深链并桥接 onDeepLink"
```

---

### Task 4: 自动更新（updater + GitHub Release 签名）

**需求/行为:** 应用启动后后台检查 GitHub Release 是否有新版本；控制中心"设置"或"工具"页提供"检查更新"按钮，发现更新后提示下载、安装并重启。需要：`tauri signer generate` 生成签名密钥对（私钥作 CI secret，公钥写 tauri.conf.json）、CI 在发布时生成 `latest.json` 与 `.sig` 签名并上传、壳侧 `updater:default` 权限、控制中心前端 `@tauri-apps/plugin-updater` + `@tauri-apps/plugin-process`。

**Files:**
- Modify: `apps/shell/src-tauri/tauri.conf.json`（pubkey + endpoints）
- Modify: `apps/shell/src-tauri/src/lib.rs`（启用 updater 插件；启动时异步 `check` 事件）
- Modify: `apps/shell/src-tauri/capabilities/default.json`（`updater:default`）
- Modify: `.github/workflows/build.yml`（release job 生成 latest.json + .sig 并上传）
- Modify: `apps/control-center/package.json`（`@tauri-apps/plugin-updater`、`@tauri-apps/plugin-process`）
- Modify: `apps/control-center/src/pages/Tools.tsx` 或新增 `src/pages/Updates.tsx`（检查更新 UI）
- Modify: `apps/control-center/src/App.tsx`（如新增页需挂 Tab）

**Interfaces:**
- Consumes: `tauri_plugin_updater`（`UpdaterExt::updater().check()` / `update.download_and_install()`）
- Produces: 前端 `checkForUpdates()` 流程（check → 有更新则 download_and_install → `relaunch()`）；CI 发布物含 `latest.json` + 各平台 `.sig`

- [ ] **Step 1: 生成签名密钥并把公钥写入配置**

Run: `cd apps/shell/src-tauri && cargo tauri signer generate -w ~/.tauri/dsh-desktop.key`（或 `yarn tauri signer generate`）
Expected: 输出公钥（`-----BEGIN PUBLIC KEY-----...`）并生成私钥文件。
将输出公钥替换 `tauri.conf.json` 的 `REPLACE_WITH_GENERATED_PUBKEY`。私钥内容**不要提交**——配到 GitHub 仓库 secret `TAURI_SIGNING_PRIVATE_KEY`（值含 `BEGIN PRIVATE KEY` 的 PEM）。

- [ ] **Step 2: 启用 updater 插件（去掉 Task 1 的占位 pubkey 写法）**

`lib.rs` 改为：

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
```

并在 `setup` 中启动后台检查（就绪后），失败仅记日志不打扰：

```rust
let updater_app = app_handle.clone();
tauri::async_runtime::spawn(async move {
    if let Ok(updater) = updater_app.updater() {
        if let Ok(Some(update)) = updater.check().await {
            let _ = updater_app.emit("dsh-update-available", update.version);
        }
    }
});
```

（需要 `use tauri_plugin_updater::UpdaterExt;`）

- [ ] **Step 3: capabilities 声明 updater 权限**

`capabilities/default.json` 的 `permissions` 加 `"updater:default"`。

- [ ] **Step 4: 控制中心前端接入检查更新**

`apps/control-center/package.json` dependencies 加：

```json
"@tauri-apps/plugin-updater": "^2",
"@tauri-apps/plugin-process": "^2"
```

在 `apps/control-center/src/pages/Tools.tsx` 增加"检查更新"工具项（复用现有工具列表结构），点击执行：

```ts
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

async function checkForUpdates(): Promise<string> {
  const update = await check();
  if (!update) return "当前已是最新版本";
  const confirmed = confirm(`发现新版本 ${update.version}，是否下载并安装？`);
  if (!confirmed) return "已取消";
  await update.downloadAndInstall();
  await relaunch();
  return "已安装，正在重启…";
}
```

- [ ] **Step 5: CI 发布 job 生成 latest.json 与签名**

`.github/workflows/build.yml` 的 `release` job，在 `Publish release` step 之前插入：

```yaml
- name: Generate updater artifacts
  shell: bash
  env:
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
  run: |
    VERSION="${{ steps.version.outputs.VERSION }}"
    node .github/scripts/generate-updater-artifacts.mjs "$VERSION"
```

创建 `.github/scripts/generate-updater-artifacts.mjs`：扫描构建产物（`*.dmg` / `*.exe` / `*.AppImage` / `*.deb`），对每个文件调用 `tauri signer sign -f <file>`（或使用 `@tauri-apps/cli` 的 `signer sign`）生成 `<file>.sig`，并写出 `latest.json`：

```json
{
  "version": "<VERSION>",
  "notes": "see CHANGELOG",
  "pub_date": "<ISO8601>",
  "platforms": {
    "darwin-aarch64": { "url": "<release-asset-url>.dmg", "signature": "<sig-content>" },
    "windows-x86_64": { "url": "<release-asset-url>.exe", "signature": "<sig-content>" },
    "linux-x86_64": { "url": "<release-asset-url>.AppImage", "signature": "<sig-content>" }
  }
}
```

并在 `files:` 列表中加入 `**/*.sig` 与 `latest.json`。脚本内对缺少密钥时打印警告并跳过签名（开发期不阻塞）。

- [ ] **Step 6: 验证**

Run: 根 `yarn typecheck`（控制中心）、`cd apps/shell/src-tauri && cargo check`（壳侧）。
Expected: 全绿。（CI 签名流程本地无法端到端跑，需 push 后观察；本任务以编译 + typecheck 通过为完成标准。）

- [ ] **Step 7: 提交**

```bash
git add apps/shell/src-tauri/tauri.conf.json apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/capabilities/default.json apps/control-center/package.json apps/control-center/src .github/workflows/build.yml .github/scripts/generate-updater-artifacts.mjs
git commit -m "feat: 接入自动更新（updater + GitHub Release 签名清单）"
```

---

### Task 5: 开机自启壳侧命令与桥接暴露

**需求/行为（壳侧部分）:** 壳侧注册 autostart 插件（Task 1 已做），并暴露两个最小命令 `get_autostart` / `set_autostart`，写入 bridge 白名单，供 dsh 插件经 `window.__DSH_DESKTOP__` 调用。默认关闭由 dsh 插件侧（Task 6）的设置 schema 默认值保证。

**Files:**
- Modify: `apps/shell/src-tauri/build.rs`（commands 增 `get_autostart` / `set_autostart`）
- Modify: `apps/shell/src-tauri/src/lib.rs`（两个 `#[tauri::command]` + `invoke_handler` 注册 + BRIDGE_SCRIPT 暴露 `autostart`）
- Modify: `apps/shell/src-tauri/capabilities/default.json` + `bridge.json`（`allow-get-autostart` / `allow-set-autostart`）
- Modify: `packages/plugins/bridge/src/index.ts`（`DshDesktopBridge` 增 `autostart`）

**Interfaces:**
- Consumes: `tauri_plugin_autostart::ManagerExt::autolaunch()`（State）
- Produces: 命令 `get_autostart() -> Result<bool, String>`、`set_autostart(enabled: bool) -> Result<(), String>`；桥接对象 `autostart.get(): Promise<boolean>` / `autostart.set(enabled: boolean): Promise<void>`

- [ ] **Step 1: build.rs 命令清单扩展**

`apps/shell/src-tauri/build.rs` 的 commands 数组加 `"get_autostart"` 与 `"set_autostart"`。

- [ ] **Step 2: 实现两个命令**

`lib.rs`：

```rust
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}
```

`invoke_handler` 列表加 `get_autostart, set_autostart`。

- [ ] **Step 3: BRIDGE_SCRIPT 暴露 autostart**

`BRIDGE_SCRIPT` 对象加：

```js
autostart: {
  get: function () { return invoke("get_autostart"); },
  set: function (enabled) { return invoke("set_autostart", { enabled: enabled }); },
},
```

- [ ] **Step 4: capabilities 权限**

`default.json` 与 `bridge.json` 的 permissions 均加 `"allow-get-autostart"`、`"allow-set-autostart"`。

- [ ] **Step 5: bridge 插件类型**

`DshDesktopBridge` 加：

```ts
autostart: {
  get(): Promise<boolean>;
  set(enabled: boolean): Promise<void>;
};
```

- [ ] **Step 6: 编译验证**

Run: `cd apps/shell/src-tauri && cargo check && cargo test` 与根 `yarn typecheck`。Expected: 全绿（含 ACL 权限解析）。

- [ ] **Step 7: 提交**

```bash
git add apps/shell/src-tauri/build.rs apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/capabilities/default.json apps/shell/src-tauri/capabilities/bridge.json packages/plugins/bridge/src/index.ts
git commit -m "feat: 壳侧暴露开机自启 get/set 命令并桥接"
```

---

### Task 6: 开机自启 dsh 插件设置面板项（WebUI，默认关闭）

**需求/行为:** 在 dsh WebUI 设置面板增加"开机自启"开关，**默认关闭**。实现为 `packages/plugins/bridge` 的双面插件：host 侧注册 settings 命名空间（schema `{ autostart: z.boolean().default(false) }`），client 侧注册 `settings.section` 渲染开关，切换时经 `window.__DSH_DESKTOP__.autostart` 调壳。插件需声明 `dsh.client`（platform web）并导出 `./client`。

**Files:**
- Modify: `packages/plugins/bridge/package.json`（`dsh.client`、exports `./client`、依赖 `@deepseek-ai/schemastery`、`@deepseek-ai/dsh-settings`、`@deepseek-ai/dsh-client-runtime`、`@deepseek-ai/dsh-client-ui-settings`、`react`）
- Create: `packages/plugins/bridge/src/client.tsx`（React 组件：开关 + settingsScope 绑定 + 调桥接）
- Modify: `packages/plugins/bridge/src/index.ts`（host apply：注册 settings 命名空间）
- Modify: `packages/plugins/bridge/cordis.patch.yml`（如需 client 行；按官方模式 host 行即可，client 面由 `dsh.client` 自动装配）

**Interfaces:**
- Consumes: Task 5 的 `window.__DSH_DESKTOP__.autostart.get()/set()`；dsh host `ctx.inject(["settings"])` + `ctx.settings.register(settingsNamespace(ns), schema)`；dsh client `ctx.slots.inject("settings.section")` + `ctx.settingsScope.bind({ namespace })`
- Produces: WebUI 设置面板出现"开机自启"开关（默认 false）；切换写入 settings 文档并同步壳侧 autostart

> **实现模板（权威来源，实施时对照）：**
> - section 注册模式：`node_modules/@deepseek-ai/dsh-client-ui-settings-general/lib/client.js` 第 589 行的 `ctx.slots.inject("settings.section", () => ctx.slots.register({ name, id, order, label, locale }, Component))`
> - settingsScope 绑定：`node_modules/@deepseek-ai/dsh-client-ui-settings-plugins/lib/client.js` 第 1152 行的 `ctx.settingsScope.bind({ namespace: NS })`，返回 `SettingsScope<T>`（`getSnapshot()` / `set(field, value)` / `subscribe(listener)`）
> - host settings 注册：`node_modules/@deepseek-ai/dsh-client-ui-settings-general/lib/index.js` 的 `ctx.inject(["settings"], (s) => s.settings.register(settingsNamespace(NS), Schema))`
> - 版本差异以本机 `node_modules/@deepseek-ai/dsh-client-*` 的 `.d.ts` 为准调整 import 路径

- [ ] **Step 1: host 侧注册 settings 命名空间**

`packages/plugins/bridge/src/index.ts` 改为：

```ts
import { z } from "@deepseek-ai/schemastery";
import { settingsNamespace } from "@deepseek-ai/dsh-settings";
import type { Context } from "@deepseek-ai/cordis";

export const name = "bridge";

/** 桌面壳设置命名空间：autostart 默认关闭。 */
const DesktopSchema = z.object({
  autostart: z.boolean().default(false),
});

export function apply(ctx: Context) {
  ctx.inject(["settings"], (settingsCtx) => {
    settingsCtx.settings.register(settingsNamespace("desktop"), DesktopSchema);
  });
}
```

（`@deepseek-ai/schemastery` 的 `z` 与 `@deepseek-ai/dsh-settings` 的 `settingsNamespace` 为 dsh 官方 settings 体系入口，已在 `dsh-client-ui-settings-general` 的 index.js 中验证使用方式一致。）

- [ ] **Step 2: client 侧注册设置面板开关**

创建 `packages/plugins/bridge/src/client.tsx`：

```tsx
/** 桥接对象类型（与 ./index 的 DshDesktopBridge 一致；client 侧内联读取 window，不跨侧 import）。 */
interface BridgeLike {
  autostart: { get(): Promise<boolean>; set(enabled: boolean): Promise<void> };
}

function getBridge(): BridgeLike | null {
  return (window as unknown as { __DSH_DESKTOP__?: BridgeLike }).__DSH_DESKTOP__ ?? null;
}

/** 开机自启开关：绑定 desktop.autostart 命名空间，切换时同步 Tauri 壳。 */
export function AutostartSwitch(props: { scope: any; t: (key: string) => string }): JSX.Element {
  const snapshot = props.scope.getSnapshot();
  const value = (snapshot.value as { autostart?: boolean } | undefined)?.autostart ?? false;
  const onChange = async (next: boolean) => {
    const bridge = getBridge();
    if (bridge) {
      try {
        await bridge.autostart.set(next);
      } catch {
        // 壳不可达时仍写 settings（由下一次启动/桥接补偿）
      }
    }
    await props.scope.set("autostart", next);
  };
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 0" }}>
      <div style={{ flex: 1 }}>
        <div style={{ fontWeight: 600 }}>{props.t("autostart.title")}</div>
        <div style={{ fontSize: 12, opacity: 0.6 }}>{props.t("autostart.desc")}</div>
      </div>
      <input
        type="checkbox"
        checked={value}
        onChange={(e) => void onChange(e.target.checked)}
        aria-label={props.t("autostart.title")}
      />
    </div>
  );
}

/** 所需服务（cordis fiber inject；dsh client 惯例如 settings-plugins 的 inject 数组）。 */
export const inject = ["slots", "locale", "settingsScope"];

export function apply(ctx: any) {
  const NS = "desktop";
  const t = ctx.locale.bind("settings.desktop");
  // settingsScope 由 @deepseek-ai/dsh-client-ui-settings 注入，bind 返回 SettingsScope<T>
  const settingsScope = ctx.settingsScope.bind({ namespace: NS });
  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "desktop",
        order: 90,
        label: () => t("nav"),
        locale: "settings.desktop",
        children: {},
      },
      () => <AutostartSwitch scope={settingsScope} t={t} />,
    ),
  );
}
```

> **实现说明（非占位，是给执行者的具体路径）：** `settingsScope` 服务由 `@deepseek-ai/dsh-client-ui-settings` 提供（`SettingsScopeBinder`）；`ctx.slots.inject("settings.section")` 的模式取自 `dsh-client-ui-settings-general/lib/client.js` 第 589 行。若注入名 `settingsScope` 或 `locale` 与当前版本不符，对照本机 `dsh-client-ui-settings/lib/types/client/` 的 `.d.ts` 修正注入名（历史上为 `settingsScope` / `locale`）。locale 字典可省略（用常量字符串），最小实现优先保证开关可用。

- [ ] **Step 3: package.json 声明 client 面与依赖**

`packages/plugins/bridge/package.json`：

```json
{
  "name": "@dsh-desktop/plugin-bridge",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "exports": {
    ".": "./src/index.ts",
    "./client": "./src/client.tsx"
  },
  "scripts": { "typecheck": "tsc --noEmit" },
  "dependencies": {
    "@deepseek-ai/cordis": "^4.0.1",
    "@deepseek-ai/dsh-settings": "^0.1.0-rc.6",
    "@deepseek-ai/schemastery": "^3.18.1",
    "@dsh-desktop/contracts": "workspace:^"
  },
  "peerDependencies": {
    "react": "^18.2.0",
    "@deepseek-ai/dsh-client-runtime": "^0.1.0-rc.6",
    "@deepseek-ai/dsh-client-ui-settings": "^0.1.0-rc.6",
    "@deepseek-ai/dsh-client-locale": "^0.1.0-rc.6",
    "@deepseek-ai/dsh-api-remotes": "^0.1.0-rc.6"
  },
  "dsh": {
    "bundle": { "patch": "./cordis.patch.yml" },
    "client": {
      "inject": [
        "@deepseek-ai/dsh-client-runtime",
        "@deepseek-ai/dsh-client-ui-settings",
        "@deepseek-ai/dsh-client-locale",
        "@deepseek-ai/dsh-api-remotes"
      ],
      "platform": "web"
    }
  }
}
```

（peerDependencies 与 dsh.client.inject 参照 `dsh-client-ui-settings-general` 的 package.json；react 用 peer 避免与 dsh web 的 react 双份。）

- [ ] **Step 4: 安装依赖并类型检查**

Run: 根 `yarn install` 注册新依赖，然后 `cd packages/plugins/bridge && yarn typecheck`
Expected: 编译通过。若 `@deepseek-ai/dsh-settings` / `schemastery` 版本差异导致类型错误，以本机安装版本（`node_modules/@deepseek-ai/schemastery` 的 d.ts）调整 import 与 schema 构造。

- [ ] **Step 5: 端到端验证（需 dsh 环境）**

- 用 `yarn dev` 启动桌面应用（或先 `dsh web` 到 `packages/plugins/bridge` 用 `dsh plugin --profile web add` 装配插件）。
- 打开 dsh WebUI 设置面板，确认出现"桌面"页签且含"开机自启"开关，默认关闭。
- 切换开关：`log show` 或壳侧日志确认 `set_autostart` 被调用；重启桌面应用确认开关状态保持。
- 若 client 注入服务名不匹配导致页面报错，按 Step 2 的实现说明修正。

- [ ] **Step 6: 提交**

```bash
git add packages/plugins/bridge/package.json packages/plugins/bridge/src/index.ts packages/plugins/bridge/src/client.tsx packages/plugins/bridge/tsconfig.json packages/plugins/bridge/cordis.patch.yml
git commit -m "feat: dsh 插件 WebUI 设置面板开机自启开关（默认关闭）"
```

### Task 7: 自定义全局快捷键（桥接层暴露）

**需求/行为:** 允许 dsh 插件注册/注销系统级全局快捷键（如 `CmdOrCtrl+Shift+D`），触发时壳侧 emit `dsh-shortcut` 事件，桥接层暴露 `onShortcut`。实现为两个最小命令 `register_shortcut` / `unregister_shortcut`（内部用已注册的 global-shortcut 插件），注册表存于 AppState，回调触发时 emit。

**Files:**
- Modify: `apps/shell/src-tauri/build.rs`
- Modify: `apps/shell/src-tauri/src/lib.rs`（命令 + `HashMap<String, ShortcutId>` 注册表 + 事件 + BRIDGE_SCRIPT）
- Modify: `apps/shell/src-tauri/capabilities/default.json` + `bridge.json`
- Modify: `packages/contracts/src/index.ts`（EVENTS 增 `dshShortcut`）
- Modify: `packages/plugins/bridge/src/index.ts`

**Interfaces:**
- Consumes: `tauri_plugin_global_shortcut`（已注册；`app.global_shortcut().register(Shortcut, handler)`）
- Produces: 命令 `register_shortcut(shortcut: String) -> Result<(), String>`、`unregister_shortcut(shortcut: String) -> Result<(), String>`；事件 `dsh-shortcut`（payload: string 快捷键字符串）；桥接 `shortcuts.register(s, cb)` / `shortcuts.unregister(s)` / `onShortcut(cb)`

- [ ] **Step 1: build.rs 命令扩展**

commands 数组加 `"register_shortcut"` / `"unregister_shortcut"`。

- [ ] **Step 2: AppState 增加快捷键注册表**

`AppState` 增 `shortcuts: Arc<Mutex<HashMap<String, tauri_plugin_global_shortcut::Shortcut>>>`（setup 中初始化）。

- [ ] **Step 3: 实现命令**

```rust
use std::str::FromStr;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[tauri::command]
fn register_shortcut(state: State<AppState>, shortcut: String) -> Result<(), String> {
    let s = Shortcut::from_str(&shortcut).map_err(|e| e.to_string())?;
    let app = state.app.clone();
    // 带 handler 的注册用 on_shortcut；回调签名 Fn(&AppHandle, &Shortcut, ShortcutEvent)
    app.global_shortcut().on_shortcut(s.clone(), move |app, _s, event| {
        if event.state() == ShortcutState::Pressed {
            let _ = app.emit("dsh-shortcut", shortcut.clone());
        }
    }).map_err(|e| e.to_string())?;
    state.shortcuts.lock().unwrap().insert(shortcut, s);
    Ok(())
}

#[tauri::command]
fn unregister_shortcut(state: State<AppState>, shortcut: String) -> Result<(), String> {
    let app = state.app.clone();
    if let Some(s) = state.shortcuts.lock().unwrap().remove(&shortcut) {
        app.global_shortcut().unregister(s).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

`invoke_handler` 加两个命令；BRIDGE_SCRIPT 加 `shortcuts` 对象（register/unregister/onShortcut）。

- [ ] **Step 4: capabilities 与契约**

default.json / bridge.json 加 `"allow-register-shortcut"` / `"allow-unregister-shortcut"`；contracts `EVENTS` 加 `dshShortcut: "dsh-shortcut"`；bridge 插件类型加 `shortcuts`。

- [ ] **Step 5: 验证**

Run: `cd apps/shell/src-tauri && cargo check && cargo test` + 根 `yarn typecheck`。Expected: 全绿。

- [ ] **Step 6: 提交**

```bash
git add apps/shell/src-tauri/ packages/contracts/src/index.ts packages/plugins/bridge/src/index.ts
git commit -m "feat: 桥接层暴露自定义全局快捷键注册"
```

---

### Task 8: 托盘"退出"二次确认

**需求/行为:** 点击托盘"退出"时先弹系统对话框确认（避免误触导致 dsh 会话丢失），确认后才执行 `exiting=true` + 停止子进程 + 退出。

**Files:**
- Modify: `apps/shell/src-tauri/src/lib.rs`（`setup_tray` 的 `TRAY_QUIT` 分支改用 dialog 确认）

**Interfaces:**
- Consumes: `tauri_plugin_dialog::DialogExt::message()`
- Produces: 无新 IPC；托盘退出增加确认步骤

- [ ] **Step 1: TRAY_QUIT 分支改为异步确认**

`lib.rs` 的 `TRAY_QUIT` 分支替换为：

```rust
TRAY_QUIT => {
    let app = app.clone();
    let exiting = exiting.clone();
    app.dialog().message("退出后将停止当前 dsh 会话，确认退出？")
        .title("退出 DSH Desktop")
        .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom("退出".into(), "取消".into()))
        .show(move |confirmed| {
            if !confirmed { return; }
            exiting.store(true, Ordering::Relaxed);
            let state = app.state::<AppState>();
            let _ = state.tx.send(ManagerMessage::Stop);
            thread::sleep(Duration::from_millis(300));
            app.exit(0);
        });
}
```

需要 `use tauri_plugin_dialog::DialogExt;`。

- [ ] **Step 2: 编译验证**

Run: `cd apps/shell/src-tauri && cargo check && cargo test`
Expected: 通过。确认 `MessageDialogButtons` 枚举名与当前插件版本一致（不一致则以 `d.ts`/源码为准调整）。

- [ ] **Step 3: 提交**

```bash
git add apps/shell/src-tauri/src/lib.rs
git commit -m "feat: 托盘退出增加二次确认"
```

---

### Task 9: 文档与契约同步收尾

**Files:**
- Modify: `AGENTS.md`（root：IPC 命令/事件表补新命令与事件、原生能力清单补六项）
- Modify: `apps/shell/src-tauri/AGENTS.md`（契约表、原生能力节、插件清单）
- Modify: `apps/control-center/AGENTS.md`（契约表）
- Modify: `packages/contracts/AGENTS.md`
- Modify: `packages/plugins/AGENTS.md` 与 `packages/plugins/bridge/AGENTS.md`（新增插件说明）
- Modify: `docs/plugin-tauri-boundary.md`（如需记录开机自启设置项归属）
- Modify: `README.md`（特性清单补六项）

- [ ] **Step 1: 逐文件同步契约与能力清单**

按各文件现有表格结构，补：命令 `get_autostart` / `set_autostart` / `register_shortcut` / `unregister_shortcut`；事件 `dsh-deeplink` / `dsh-shortcut` / `dsh-update-available`；原生能力节补 窗口状态记忆 / 深链 / 自动更新 / 开机自启(dsh 插件) / 自定义快捷键 / 退出确认。

- [ ] **Step 2: 全量回归**

Run: 根 `yarn typecheck`；`cd apps/shell/src-tauri && cargo test && cargo clippy && cargo fmt --check`
Expected: 全绿、零警告。

- [ ] **Step 3: 提交**

```bash
git add AGENTS.md README.md apps/shell/src-tauri/AGENTS.md apps/control-center/AGENTS.md packages/contracts/AGENTS.md packages/plugins/AGENTS.md packages/plugins/bridge/AGENTS.md docs/plugin-tauri-boundary.md
git commit -m "docs: 同步原生能力二期契约与文档"
```

---

## 自审记录（self-review）

- **Spec 覆盖：** 六项需求各自落在 Task 2–8；Task 1 打底，Task 9 收尾。开机自启按要求走 dsh 插件设置面板（Task 5 壳命令 + Task 6 插件 UI），默认关闭由 schema 默认值保证。
- **占位扫描：** 全部步骤含可执行代码或明确验证命令；Task 6 的 client 组件为完整可编译实现，并在"实现模板"块列出 dsh 官方插件的参照位置（settings-general 的 section 注册 / settings-plugins 的 settingsScope 绑定），版本差异以本机 d.ts 修正——无 TODO/TBD。
- **类型一致性：** 命令名（`get_autostart` 等）、事件名（`dsh-deeplink` 等）、权限 id（`allow-get-autostart` 等）在任务间保持一致；build.rs 命令清单与 capabilities 同步。