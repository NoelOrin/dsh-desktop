# Monorepo + 多窗口 + SolidJS 控制中心 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 dsh-desktop 从单窗口 Tauri 应用改造为 Yarn 4 monorepo，支持多窗口：main 窗口承载 dsh web（行为不变），新增按需打开的 SolidJS 控制中心窗口（仪表盘 / 设置 / 工具），所有前端统一 Vite 8。

**Architecture:** Yarn 4 workspaces（`apps/*` + `packages/*`）。`apps/shell` 承载 Tauri 壳（src-tauri + 启动页，启动页由 vanilla JS 迁移为 TypeScript），`apps/control-center` 为 SolidJS SPA（Vite 8 + vite-plugin-solid），`packages/contracts` 提供共享 IPC 类型。两个前端构建进根 `dist/`（shell → dist/，control-center → dist/control-center/），Tauri `frontendDist` 指向根 dist。Rust 侧新增 DshConfig 持久化（config.json，优先级高于环境变量）与 get_config/set_config 命令；control 窗口启动时创建但隐藏，由应用菜单项 + 全局快捷键（tauri-plugin-global-shortcut）按需调起。

**Tech Stack:** Yarn 4.17.1 (PnP)、Vite 8、TypeScript 5、SolidJS ^1.7.2、vite-plugin-solid ^2.11、Tauri 2.11（Rust 1.97）、tauri-plugin-global-shortcut 2、serde/serde_json、concurrently。

**Spec:** `docs/superpowers/specs/2026-08-14-monorepo-multiwindow-solidjs-design.md`

## Global Constraints

- 所有前端包使用 Vite 8（当前最新 8.2.x）。
- Yarn 4.17.1 workspaces；每个 workspace package.json 都声明 `"packageManager": "yarn@4.17.1"`。
- 工作区依赖使用 `"workspace:^"` 协议。
- 提交信息遵循 Conventional Commits；用户可见文案与代码注释使用中文。
- 窗口 label：`main`（dsh 主窗口，行为不变）、`control`（SolidJS 控制中心，`visible: false`）。
- 配置解析优先级：config.json → 环境变量 → PATH 检测。
- 构建顺序：先 shell（清空根 dist/），后 control-center（只清空 dist/control-center/）。
- control-center 的 Vite `base` 为 `/control-center/`，dev 端口 5174（strictPort）；shell dev 端口 5173（strictPort）。
- 修改 IPC 契约必须同步 `packages/contracts`、`apps/shell/src`、`apps/control-center/src` 与 Rust serde 类型。

---

## Task 1: Monorepo 骨架 —— 根 workspaces + 迁移 shell 到 apps/shell

**Files:**
- Create: `package.json`（重写）、`apps/shell/package.json`、`apps/shell/vite.config.ts`、`apps/shell/tsconfig.json`
- Move (git mv 保留历史): `src/` `index.html` `src-tauri/` → `apps/shell/`；`src/AGENTS.md` → `apps/shell/AGENTS.md`；`src-tauri/AGENTS.md` → `apps/shell/src-tauri/AGENTS.md`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: 无（首个任务）。
- Produces: 根 `package.json` 脚本（dev / build / dev:web / build:web / tauri / typecheck）；shell 脚本（dev / build / tauri / dev:web / build:web / web:all / web:build / typecheck）；`apps/shell/src-tauri` 在 `apps/shell` 下可被 `cargo` 与 `tauri` 使用。

- [ ] **Step 1: 创建目录并 git mv 现有文件**

```bash
mkdir -p apps/shell apps/control-center packages/contracts
git mv src apps/shell/src
git mv index.html apps/shell/index.html
git mv src-tauri apps/shell/src-tauri
git mv src/AGENTS.md apps/shell/AGENTS.md   # src/AGENTS.md 此时已随 src 移入 apps/shell/src，改为从新位置移
git mv apps/shell/src/AGENTS.md apps/shell/AGENTS.md
git mv apps/shell/src-tauri/AGENTS.md apps/shell/src-tauri/AGENTS.md
```

> 注：若上一步 `git mv src apps/shell/src` 已把 `src/AGENTS.md` 一并移入 `apps/shell/src/AGENTS.md`，则后续用 `git mv apps/shell/src/AGENTS.md apps/shell/AGENTS.md`。执行后 `git status` 确认路径再继续。

- [ ] **Step 2: 重写根 `package.json`**

```json
{
  "name": "dsh-desktop",
  "private": true,
  "version": "0.1.0",
  "packageManager": "yarn@4.17.1",
  "type": "module",
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev": "yarn workspace @dsh-desktop/shell dev",
    "build": "yarn workspace @dsh-desktop/shell build",
    "dev:web": "yarn workspace @dsh-desktop/shell web:all",
    "build:web": "yarn workspace @dsh-desktop/shell web:build",
    "tauri": "yarn workspace @dsh-desktop/shell tauri",
    "typecheck": "yarn workspaces foreach -pt run typecheck"
  }
}
```

- [ ] **Step 3: 创建 `apps/shell/package.json`**

```json
{
  "name": "@dsh-desktop/shell",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "yarn@4.17.1",
  "scripts": {
    "dev": "tauri dev",
    "build": "tauri build",
    "tauri": "tauri",
    "dev:web": "vite",
    "build:web": "vite build",
    "web:all": "yarn dev:web",
    "web:build": "yarn build:web",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "@tauri-apps/api": "^2"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "concurrently": "^9",
    "typescript": "^5",
    "vite": "^8"
  }
}
```

- [ ] **Step 4: 创建 `apps/shell/vite.config.ts` 与 `apps/shell/tsconfig.json`**

```ts
// apps/shell/vite.config.ts
import { defineConfig } from "vite";

export default defineConfig({
  root: __dirname,
  build: {
    outDir: "../../dist",
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    strictPort: true,
  },
});
```

```json
// apps/shell/tsconfig.json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noEmit": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "types": ["vite/client"]
  },
  "include": ["src"]
}
```

- [ ] **Step 5: 更新 `.gitignore`**

```text
node_modules
.yarn/
dist
apps/shell/src-tauri/target
apps/shell/src-tauri/gen
.DS_Store
.idea
```

- [ ] **Step 6: 调整 `apps/shell/src-tauri/tauri.conf.json` 的构建命令与 frontendDist**

编辑 `apps/shell/src-tauri/tauri.conf.json`：
- `build.beforeDevCommand` → `"corepack yarn web:all"`
- `build.beforeBuildCommand` → `"corepack yarn web:build"`
- `build.frontendDist` → `"../../dist"`
- `build.devUrl` 保持 `"http://localhost:5173"`

- [ ] **Step 7: 安装依赖并验证**

```bash
yarn install
yarn workspace @dsh-desktop/shell build:web   # 应生成 dist/index.html
yarn workspace @dsh-desktop/shell typecheck   # 暂无不通过项（main.js 未在 tsconfig include 内，可先跳过）
cd apps/shell/src-tauri && cargo check        # 后端编译通过
```

预期：`dist/index.html` 存在；`cargo check` 无错误（提示：cwd 在 apps/shell/src-tauri）。若 `src/AGENTS.md` 未随移动，检查 git status。

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: migrate to yarn workspaces monorepo with apps/shell"
```

---

## Task 2: packages/contracts 共享 IPC 类型包

**Files:**
- Create: `packages/contracts/package.json`、`packages/contracts/tsconfig.json`、`packages/contracts/src/index.ts`
- Modify: `apps/shell/package.json`（增加 @dsh-desktop/contracts 依赖）

**Interfaces:**
- Consumes: Task 1 的 workspace 布局。
- Produces: 命名导出 `RuntimePhase`、`RuntimeSnapshot`、`DshConfig`、`COMMANDS`、`EVENTS`（与 Rust serde snake_case 字段一一对应）。

- [ ] **Step 1: 创建 `packages/contracts/package.json`**

```json
{
  "name": "@dsh-desktop/contracts",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "yarn@4.17.1",
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "exports": { ".": "./src/index.ts" }
}
```

- [ ] **Step 2: 创建 `packages/contracts/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noEmit": true,
    "lib": ["ES2022"],
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

- [ ] **Step 3: 创建 `packages/contracts/src/index.ts`**

```ts
export type RuntimePhase =
  | "detecting"
  | "missing"
  | "installing"
  | "starting"
  | "ready"
  | "failed"
  | "stopped";

export interface RuntimeSnapshot {
  phase: RuntimePhase;
  message: string;
  url: string | null;
  dsh_installed: boolean;
  node_found: boolean;
  install_dir: string | null;
  log_dir: string | null;
  logs: string[];
}

export interface DshConfig {
  dsh_bin: string | null;
  dsh_node: string | null;
  dsh_home: string | null;
}

export const COMMANDS = {
  getStatus: "get_status",
  installDsh: "install_dsh",
  restart: "restart",
  openLogDirectory: "open_log_directory",
  getConfig: "get_config",
  setConfig: "set_config",
} as const;

export const EVENTS = {
  dshStatus: "dsh-status",
  dshLog: "dsh-log",
} as const;
```

- [ ] **Step 4: 给 `apps/shell` 加 contracts 依赖**

在 `apps/shell/package.json` 的 dependencies 中增加：`"@dsh-desktop/contracts": "workspace:^"`，然后：

```bash
yarn install
cd packages/contracts && npx tsc --noEmit && cd ../..  # 类型检查通过
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add shared IPC contracts package"
```
## Task 3: 启动页迁移 TypeScript（apps/shell）

**Files:**
- Create: `apps/shell/src/main.ts`（由 main.js 迁移）、`apps/shell/index.html` 更新 script 引用
- Delete: `apps/shell/src/main.js`

**Interfaces:**
- Consumes: `@dsh-desktop/contracts` 的 `RuntimeSnapshot`（Task 2）。
- Produces: `apps/shell/src/main.ts` 保持与旧 main.js 完全一致的行为（IPC 命令与事件不变）。

- [ ] **Step 1: 创建 `apps/shell/src/main.ts`（迁移 + 类型化）**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RuntimePhase, RuntimeSnapshot } from "@dsh-desktop/contracts";

const messageEl = document.getElementById("message") as HTMLSpanElement;
const statusEl = document.getElementById("status") as HTMLDivElement;
const installBtn = document.getElementById("install") as HTMLButtonElement;
const retryBtn = document.getElementById("retry") as HTMLButtonElement;
const openLogsBtn = document.getElementById("open-logs") as HTMLButtonElement;
const logsEl = document.getElementById("logs") as HTMLPreElement;

const SHOW_LOGS = new Set<RuntimePhase>(["installing", "starting", "failed"]);

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function render(status: Partial<RuntimeSnapshot>): void {
  const phase = (status.phase ?? "detecting") as RuntimePhase;
  statusEl.dataset.phase = phase;
  messageEl.textContent = status.message || "DSH 状态未知";

  installBtn.classList.toggle("hidden", phase !== "missing");
  retryBtn.classList.toggle("hidden", phase !== "failed");
  openLogsBtn.classList.toggle("hidden", phase !== "failed");
  logsEl.classList.toggle("hidden", !SHOW_LOGS.has(phase));

  if (phase === "ready" && status.url) {
    window.location.href = status.url;
  }
}

function appendLog(line: string): void {
  logsEl.textContent += `${line}\n`;
  logsEl.scrollTop = logsEl.scrollHeight;
}

async function init(): Promise<void> {
  if (!isTauri()) {
    render({ phase: "detecting", message: "正在检测运行环境..." });
    return;
  }

  try {
    const status = await invoke<RuntimeSnapshot>("get_status");
    if (status.logs?.length) {
      logsEl.textContent = status.logs.join("\n");
      logsEl.scrollTop = logsEl.scrollHeight;
    }
    render(status);
  } catch (error) {
    render({ phase: "failed", message: String(error) });
  }

  await listen<RuntimeSnapshot>("dsh-status", (event) => render(event.payload));
  await listen<string>("dsh-log", (event) => appendLog(String(event.payload)));
}

installBtn.addEventListener("click", () => {
  render({ phase: "installing", message: "正在安装 DeepSeek Harness..." });
  invoke("install_dsh").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

retryBtn.addEventListener("click", () => {
  render({ phase: "detecting", message: "正在重新检测运行环境..." });
  invoke("restart").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

openLogsBtn.addEventListener("click", () => {
  invoke("open_log_directory").catch((error) => {
    render({ phase: "failed", message: String(error) });
  });
});

init().catch((error) => {
  render({ phase: "failed", message: String(error) });
});
```

- [ ] **Step 2: 更新 `apps/shell/index.html` 的脚本引用**

把 `<script type="module" src="/src/main.js"></script>` 改为 `<script type="module" src="/src/main.ts"></script>`，删除 `apps/shell/src/main.js`。

- [ ] **Step 3: 验证**

```bash
yarn workspace @dsh-desktop/shell typecheck   # tsc --noEmit 通过
yarn workspace @dsh-desktop/shell build:web   # 构建成功，dist/index.html 引用 /assets/*.js
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: migrate shell splash page to TypeScript"
```

---

## Task 4: Rust —— DshConfig 持久化 + get_config/set_config + 解析优先级

**Files:**
- Create: `apps/shell/src-tauri/src/config.rs`、`apps/shell/src-tauri/src/config.rs` 内嵌单元测试
- Modify: `apps/shell/src-tauri/src/lib.rs`（mod config、AppState/DshManager 增加 config_path、handle_start/start/resolve_* 改为读配置、新增 get_config/set_config 命令并注册）

**Interfaces:**
- Consumes: Task 1 迁移后的 lib.rs。
- Produces:
  - `config::DshConfig { dsh_bin: Option<String>, dsh_node: Option<String>, dsh_home: Option<String> }`（Serialize + Deserialize，snake_case）
  - `config::load(path: &Path) -> DshConfig`、`config::save(path: &Path, cfg: &DshConfig) -> Result<(), String>`、`DshConfig::effective(getenv: impl Fn(&str) -> Option<String>) -> DshConfig`
  - IPC 命令 `get_config() -> DshConfig`、`set_config(config: DshConfig) -> Result<(), String>`
  - 解析顺序：config.json → 环境变量 → PATH 检测

- [ ] **Step 1: 创建 `apps/shell/src-tauri/src/config.rs`（含单元测试）**

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DshConfig {
    pub dsh_bin: Option<String>,
    pub dsh_node: Option<String>,
    pub dsh_home: Option<String>,
}

impl DshConfig {
    /// config.json 中的值优先于环境变量；返回“生效配置”。
    pub fn effective(&self, getenv: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            dsh_bin: self.dsh_bin.clone().or_else(|| getenv("DSH_BIN")),
            dsh_node: self.dsh_node.clone().or_else(|| getenv("DSH_NODE")),
            dsh_home: self.dsh_home.clone().or_else(|| getenv("DSH_HOME")),
        }
    }
}

pub fn load(path: &Path) -> DshConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, config: &DshConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_prefers_stored_over_env() {
        let config = DshConfig {
            dsh_bin: Some("/stored/bin".into()),
            dsh_node: None,
            dsh_home: None,
        };
        let effective = config.effective(|k| match k {
            "DSH_BIN" => Some("/env/bin".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_bin.as_deref(), Some("/stored/bin"));
    }

    #[test]
    fn effective_falls_back_to_env() {
        let config = DshConfig::default();
        let effective = config.effective(|k| match k {
            "DSH_NODE" => Some("/env/node".into()),
            _ => None,
        });
        assert_eq!(effective.dsh_node.as_deref(), Some("/env/node"));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("dsh-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let config = DshConfig {
            dsh_bin: Some("/a".into()),
            dsh_node: Some("/b".into()),
            dsh_home: Some("/c".into()),
        };
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded, config);
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: 修改 `apps/shell/src-tauri/src/lib.rs`**

在文件顶部加 `mod config;` 与 `use config::DshConfig;`。做以下修改：

(a) `AppState` 增加字段 `config_path: PathBuf`；`DshManager` 增加字段 `config_path: PathBuf`。

(b) `setup()` 中创建 `let config_path = app_data.join("config.json");`，分别传入 `AppState` 与 `DshManager`。

(c) `handle_start()` 开头改为：

```rust
let config = config::load(&self.config_path).effective(|k| std::env::var(k).ok());
let (node, entry) = match resolve_dsh(&config, &self.runtime_dir) {
    Some(pair) => pair,
    None => {
        let node_found = resolve_node(&config).is_some();
        // ... 其余逻辑不变
    }
};
// ... self.start(node, entry) 不变
```

(d) `start()` 中 `let home = std::env::var("DSH_HOME").ok();` 改为接收 config：给 `start` 增加 `home: Option<String>` 参数，由 `handle_start` 从 `config.dsh_home` 传入；`if let Some(home) = &home { cmd.env("DSH_HOME", home); }` 保持不变。

(e) 把 `resolve_dsh`、`resolve_node`、`resolve_npm` 改造为以 `config: &DshConfig` 为首参：

```rust
fn resolve_dsh(config: &DshConfig, runtime_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let node = resolve_node(config)?;
    let entry = config
        .dsh_bin
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let candidate = runtime_dir.join("bin/dsh");
            candidate.is_file().then_some(candidate)
        })
        .or_else(|| {
            let candidate = runtime_dir.join("node_modules/@deepseek-ai/dsh/lib/bin.js");
            candidate.is_file().then_some(candidate)
        })
        .or_else(|| find_in_path("dsh"))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/dsh"))
                .filter(|path| path.is_file())
        })?;
    Some((node, entry))
}

fn resolve_node(config: &DshConfig) -> Option<PathBuf> {
    config
        .dsh_node
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_in_path("node"))
        .or_else(|| {
            home_dir()
                .map(|home| home.join(".vite-plus/bin/node"))
                .filter(|path| path.is_file())
        })
}

fn resolve_npm(config: &DshConfig) -> PathBuf {
    find_in_path("npm")
        .or_else(|| {
            resolve_node(config)
                .and_then(|node| node.parent().map(|dir| dir.join("npm")))
                .filter(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("npm"))
}
```

> 注意：`resolve_dsh` 内原来的 `std::env::var("DSH_BIN")` 分支已由 `config.dsh_bin` 覆盖（config 已是 effective 合并结果）。`resolve_npm` 改为 `fn resolve_npm(config: &DshConfig) -> PathBuf` 后，`run_install` 需要新增 `config: &DshConfig` 参数并在内部调用 `resolve_npm(config)`；`install_dsh` 命令是独立命令（不经 handle_start），因此它自己加载配置：`let config = config::load(&state.config_path).effective(|k| std::env::var(k).ok());`，再传给 `run_install`。

(f) 新增命令（放在其他 `#[tauri::command]` 旁）：

```rust
#[tauri::command]
fn get_config(state: State<AppState>) -> DshConfig {
    let stored = config::load(&state.config_path);
    stored.effective(|k| std::env::var(k).ok())
}

#[tauri::command]
fn set_config(state: State<AppState>, config: DshConfig) -> Result<(), String> {
    config::save(&state.config_path, &config)
}
```

(g) 在 `invoke_handler(tauri::generate_handler![...])` 中追加 `get_config, set_config`。

- [ ] **Step 3: 运行测试与静态检查**

```bash
cd apps/shell/src-tauri
cargo test            # 3 个 config 单测通过
cargo check
cargo clippy -- -D warnings   # 无告警
cargo fmt --check
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: persist DshConfig and add get_config/set_config commands"
```

---

## Task 5: Rust —— control 窗口 + 应用菜单 + 全局快捷键

**Files:**
- Modify: `apps/shell/src-tauri/tauri.conf.json`（新增 control 窗口）、`apps/shell/src-tauri/capabilities/default.json`（windows + 权限）、`apps/shell/src-tauri/Cargo.toml`（新增 tauri-plugin-global-shortcut）、`apps/shell/src-tauri/src/lib.rs`（菜单、菜单事件、快捷键、open_control_window、control 窗口关闭隐藏、debug 导航）

**Interfaces:**
- Consumes: Task 4 的 AppState / DshManager。
- Produces: `open_control_window(app: &AppHandle)`（幂等：show + focus）；菜单项 id `open-control`（加速键 CmdOrCtrl+Shift+C）；全局快捷键 CmdOrCtrl+Shift+C（macOS META+SHIFT+C，其余 CONTROL+SHIFT+C）。

- [ ] **Step 1: 修改 `apps/shell/src-tauri/tauri.conf.json`**

在 `app.windows` 数组中 `main` 之后追加：

```json
{
  "label": "control",
  "title": "DSH 控制中心",
  "url": "/control-center/index.html",
  "width": 960,
  "height": 680,
  "minWidth": 480,
  "minHeight": 480,
  "center": true,
  "resizable": true,
  "visible": false,
  "backgroundColor": "#f7f8fa"
}
```

- [ ] **Step 2: 修改 `apps/shell/src-tauri/capabilities/default.json`**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "DSH Desktop main and control windows",
  "windows": ["main", "control"],
  "permissions": ["core:default", "global-shortcut:default"]
}
```

- [ ] **Step 3: 修改 `apps/shell/src-tauri/Cargo.toml`**

在 `[dependencies]` 增加：`tauri-plugin-global-shortcut = "2"`。

- [ ] **Step 4: 修改 `apps/shell/src-tauri/src/lib.rs`**

(a) 顶部增加导入：

```rust
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, Code, Modifiers, ShortcutState};
```

(b) 新增函数：

```rust
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu> {
    let menu = Menu::default(app)?;
    let item = MenuItem::with_id(app, "open-control", "控制中心", true, Some("CmdOrCtrl+Shift+C"))?;
    let submenu = Submenu::with_items(app, "DSH", true, &[&item])?;
    menu.append(&submenu)?;
    Ok(menu)
}

fn open_control_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_focus();
    }
}
```

(c) 在 `setup()` 的 `Ok(())` 之前追加：

```rust
let menu = build_menu(app.handle())?;
app.set_menu(menu)?;
app.on_menu_event(|app, event| {
    if event.id().as_ref() == "open-control" {
        open_control_window(app);
    }
});

// 全局快捷键：macOS 用 Cmd+Shift+C，其他平台用 Ctrl+Shift+C
let shortcut = {
    #[cfg(target_os = "macos")]
    {
        tauri_plugin_global_shortcut::Shortcut::new(
            Some(Modifiers::META | Modifiers::SHIFT),
            Code::KeyC,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri_plugin_global_shortcut::Shortcut::new(
            Some(Modifiers::CONTROL | Modifiers::SHIFT),
            Code::KeyC,
        )
    }
};

let shortcut_plugin = ShortcutBuilder::new()
    .with_shortcuts([shortcut])
    .expect("invalid global shortcut")
    .with_handler(move |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            open_control_window(app);
        }
    })
    .build()
    .expect("failed to build global shortcut plugin");
app.plugin(shortcut_plugin)?;

#[cfg(debug_assertions)]
if let Some(control) = app.get_webview_window("control") {
    if let Ok(url) = tauri::Url::parse("http://localhost:5174/control-center/") {
        let _ = control.navigate(url);
    }
}
```

> 说明：`event.id().as_ref()` 依赖 `MenuId: AsRef<str>`；若编译报错，改用 `event.id().0.as_str() == "open-control"`（MenuId 是 `pub struct MenuId(pub String)`）。`set_menu`/`plugin`/`navigate` 等调用失败仅打日志即可，不中断启动。

(d) 修改 `on_window_event`：control 窗口关闭时隐藏而非退出。现有回调开头改为：

```rust
.on_window_event(|window, event| {
    if window.label() == "control" {
        if matches!(event, WindowEvent::CloseRequested { .. }) {
            let _ = window.hide();
        }
        return;
    }
    // ... 原有 main 窗口逻辑不变
})
```

- [ ] **Step 5: 验证**

```bash
cd apps/shell/src-tauri
cargo check          # 拉取并编译 tauri-plugin-global-shortcut
cargo clippy -- -D warnings
cargo fmt --check
```

> 若 `with_shortcuts`/`with_handler`/`build` 的签名与已安装的 tauri-plugin-global-shortcut 2.x 不一致，以该 crate 源码为准修正（`~/.cargo/registry/src/*/tauri-plugin-global-shortcut-2*/`）。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: add on-demand control window with menu and global shortcut"
```
## Task 6: control-center 骨架（SolidJS SPA + Vite 8）

**Files:**
- Create: `apps/control-center/package.json`、`apps/control-center/vite.config.ts`、`apps/control-center/tsconfig.json`、`apps/control-center/index.html`、`apps/control-center/src/index.tsx`、`apps/control-center/src/App.tsx`、`apps/control-center/src/styles.css`
- Modify: `apps/shell/package.json`（web:all / web:build 加入 control-center）

**Interfaces:**
- Consumes: `@dsh-desktop/contracts`（Task 2）、Task 1 的根脚本。
- Produces: `apps/control-center` 的 dev server @ :5174、构建产物 `dist/control-center/index.html`；App 默认 Tab `dashboard`；页面组件 `Dashboard` / `Settings` / `Tools`（默认导出）供后续任务替换。

- [ ] **Step 1: 创建 `apps/control-center/package.json`**

```json
{
  "name": "@dsh-desktop/control-center",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "yarn@4.17.1",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "@dsh-desktop/contracts": "workspace:^",
    "@tauri-apps/api": "^2",
    "solid-js": "^1.7.2"
  },
  "devDependencies": {
    "typescript": "^5",
    "vite": "^8",
    "vite-plugin-solid": "^2.11.14"
  }
}
```

- [ ] **Step 2: 创建 `apps/control-center/vite.config.ts`**

```ts
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  base: "/control-center/",
  build: {
    outDir: "../../dist/control-center",
    emptyOutDir: true,
  },
  server: {
    port: 5174,
    strictPort: true,
  },
});
```

- [ ] **Step 3: 创建 `apps/control-center/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "strict": true,
    "noEmit": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "types": ["vite/client"]
  },
  "include": ["src"]
}
```

- [ ] **Step 4: 创建入口 HTML 与 TSX**

`apps/control-center/index.html`：

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>DSH 控制中心</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/index.tsx"></script>
  </body>
</html>
```

`apps/control-center/src/index.tsx`：

```tsx
import { render } from "solid-js/web";
import App from "./App";
import "./styles.css";

render(() => <App />, document.getElementById("root")!);
```

`apps/control-center/src/App.tsx`：

```tsx
import { createSignal } from "solid-js";
import Dashboard from "./pages/Dashboard";
import Settings from "./pages/Settings";
import Tools from "./pages/Tools";

type Tab = "dashboard" | "settings" | "tools";

export default function App() {
  const [tab, setTab] = createSignal<Tab>("dashboard");
  return (
    <div class="app">
      <header class="app__header">
        <h1>DSH 控制中心</h1>
        <nav>
          <button
            class={tab() === "dashboard" ? "tab active" : "tab"}
            onClick={() => setTab("dashboard")}
          >
            仪表盘
          </button>
          <button
            class={tab() === "settings" ? "tab active" : "tab"}
            onClick={() => setTab("settings")}
          >
            设置
          </button>
          <button
            class={tab() === "tools" ? "tab active" : "tab"}
            onClick={() => setTab("tools")}
          >
            工具
          </button>
        </nav>
      </header>
      <main class="app__main">
        {tab() === "dashboard" && <Dashboard />}
        {tab() === "settings" && <Settings />}
        {tab() === "tools" && <Tools />}
      </main>
    </div>
  );
}
```

`apps/control-center/src/pages/Dashboard.tsx`（占位，Task 7 填充）：

```tsx
export default function Dashboard() {
  return <section class="page">仪表盘（Task 7 实现）</section>;
}
```

`apps/control-center/src/pages/Settings.tsx`（占位，Task 8 填充）：

```tsx
export default function Settings() {
  return <section class="page">设置（Task 8 实现）</section>;
}
```

`apps/control-center/src/pages/Tools.tsx`（占位，Task 9 填充）：

```tsx
export default function Tools() {
  return <section class="page">工具（Task 9 实现）</section>;
}
```

`apps/control-center/src/styles.css`（基础样式，后续任务补充）：

```css
:root {
  color-scheme: light;
  --bg: #f7f8fa;
  --ink: #171c22;
  --muted: #6d7480;
  --line: #e3e6eb;
  --accent: #3f63f4;
  --ok: #0f9d58;
  --danger: #d92d20;
  --mono: ui-monospace, "SFMono-Regular", Menlo, monospace;
}

* { box-sizing: border-box; }
html, body { height: 100%; margin: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "PingFang SC", sans-serif;
  background: var(--bg);
  color: var(--ink);
}
.app { display: flex; flex-direction: column; height: 100vh; }
.app__header {
  display: flex; align-items: center; gap: 20px;
  padding: 12px 20px; border-bottom: 1px solid var(--line); background: #fff;
}
.app__header h1 { font-size: 16px; margin: 0; }
.app__main { flex: 1; overflow: auto; padding: 20px; }
.tab {
  appearance: none; border: 1px solid var(--line); background: #fff;
  border-radius: 5px; padding: 6px 14px; font-size: 13px; cursor: pointer;
}
.tab.active { background: var(--accent); border-color: var(--accent); color: #fff; }
.page { max-width: 720px; }
```

- [ ] **Step 5: 更新 `apps/shell/package.json` 的编排脚本**

把 `web:all` 改为：

```json
"web:all": "concurrently -k -n shell,control -c blue,magenta \"yarn dev:web\" \"yarn workspace @dsh-desktop/control-center dev\""
```

把 `web:build` 改为：

```json
"web:build": "yarn build:web && yarn workspace @dsh-desktop/control-center build"
```

- [ ] **Step 6: 安装并验证**

```bash
yarn install
yarn workspace @dsh-desktop/control-center typecheck
yarn workspace @dsh-desktop/control-center build   # 生成 dist/control-center/index.html
yarn build:web   # shell 先构建（清空 dist/），control-center 后构建（只清空 dist/control-center）
ls dist/control-center/index.html dist/index.html
```

预期：`dist/index.html` 与 `dist/control-center/index.html` 同时存在；构建顺序 shell → control-center。

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: scaffold solidjs control-center app with vite 8"
```

---

## Task 7: 控制中心仪表盘（状态 + 日志 + 控制按钮）

**Files:**
- Create: `apps/control-center/src/lib/ipc.ts`
- Modify: `apps/control-center/src/pages/Dashboard.tsx`（完整实现）、`apps/control-center/src/styles.css`（补充状态/按钮/日志样式）

**Interfaces:**
- Consumes: `@dsh-desktop/contracts` 的 `RuntimeSnapshot`/`RuntimePhase`/`COMMANDS`/`EVENTS`；Rust 命令 `get_status`/`install_dsh`/`restart`/`open_log_directory` 与事件 `dsh-status`/`dsh-log`（均来自 Task 1 迁移的既有实现 + Task 4 注册）。
- Produces: `lib/ipc.ts` 导出 `getStatus(): Promise<RuntimeSnapshot>`、`installDsh(): Promise<void>`、`restart(): Promise<void>`、`openLogDirectory(): Promise<void>`、`getConfig(): Promise<DshConfig>`、`setConfig(config): Promise<void>`、`onStatus(listener): Promise<UnlistenFn>`、`onLog(listener): Promise<UnlistenFn>`（getConfig/setConfig 供 Task 8 使用）。

- [ ] **Step 1: 创建 `apps/control-center/src/lib/ipc.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { COMMANDS, EVENTS, type DshConfig, type RuntimeSnapshot } from "@dsh-desktop/contracts";

export function getStatus(): Promise<RuntimeSnapshot> {
  return invoke<RuntimeSnapshot>(COMMANDS.getStatus);
}

export function installDsh(): Promise<void> {
  return invoke(COMMANDS.installDsh);
}

export function restart(): Promise<void> {
  return invoke(COMMANDS.restart);
}

export function openLogDirectory(): Promise<void> {
  return invoke(COMMANDS.openLogDirectory);
}

export function getConfig(): Promise<DshConfig> {
  return invoke<DshConfig>(COMMANDS.getConfig);
}

export function setConfig(config: DshConfig): Promise<void> {
  return invoke(COMMANDS.setConfig, { config });
}

export function onStatus(listener: (snapshot: RuntimeSnapshot) => void): Promise<UnlistenFn> {
  return listen<RuntimeSnapshot>(EVENTS.dshStatus, (event) => listener(event.payload));
}

export function onLog(listener: (line: string) => void): Promise<UnlistenFn> {
  return listen<string>(EVENTS.dshLog, (event) => listener(event.payload));
}
```

- [ ] **Step 2: 实现 `apps/control-center/src/pages/Dashboard.tsx`**

```tsx
import { createEffect, createSignal, onCleanup } from "solid-js";
import type { RuntimeSnapshot } from "@dsh-desktop/contracts";
import { getStatus, installDsh, onLog, onStatus, openLogDirectory, restart } from "../lib/ipc";

const SHOW_LOGS = new Set(["installing", "starting", "failed"]);

export default function Dashboard() {
  const [snapshot, setSnapshot] = createSignal<RuntimeSnapshot | null>(null);
  const [logs, setLogs] = createSignal<string[]>([]);

  createEffect(() => {
    let disposed = false;
    getStatus().then((s) => {
      if (disposed) return;
      setSnapshot(s);
      setLogs(s.logs ?? []);
    });
    const un1 = onStatus((s) => setSnapshot(s));
    const un2 = onLog((line) => setLogs((prev) => [...prev, line]));
    onCleanup(() => {
      disposed = true;
      un1.then((u) => u());
      un2.then((u) => u());
    });
  });

  const phase = () => snapshot()?.phase ?? "detecting";
  const message = () => snapshot()?.message ?? "正在检测运行环境...";
  const url = () => snapshot()?.url;
  const showLogs = () => SHOW_LOGS.has(phase());

  return (
    <section class="dashboard">
      <div class="status">
        <span class={`dot dot--${phase()}`} />
        <span class="status__message">{message()}</span>
        {url() && <code class="status__url">{url()}</code>}
      </div>
      <div class="actions">
        <button class="primary" hidden={phase() !== "missing"} onClick={() => installDsh().catch((e) => console.error(e))}>
          安装 DSH
        </button>
        <button hidden={phase() !== "failed"} onClick={() => restart().catch((e) => console.error(e))}>重试</button>
        <button hidden={phase() !== "failed"} onClick={() => openLogDirectory().catch((e) => console.error(e))}>日志目录</button>
      </div>
      <pre class="logs" hidden={!showLogs()}>{logs().join("\n")}</pre>
    </section>
  );
}
```

- [ ] **Step 3: 补充 `apps/control-center/src/styles.css`**

```css
.dashboard .status { display: flex; align-items: center; gap: 10px; margin-bottom: 16px; }
.dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); }
.dot--ready { background: var(--ok); }
.dot--failed { background: var(--danger); }
.dot--missing { background: #b7791f; }
.status__url { font-family: var(--mono); font-size: 12px; color: var(--muted); }
.actions { display: flex; gap: 10px; margin-bottom: 16px; }
button {
  appearance: none; border: 1px solid var(--line); background: #fff; color: var(--ink);
  border-radius: 5px; padding: 8px 18px; font-size: 13px; cursor: pointer;
}
button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
.logs {
  margin: 0; max-height: 300px; overflow: auto; padding: 12px 14px;
  border: 1px solid var(--line); border-radius: 6px; background: #fff;
  color: #4b5463; font-family: var(--mono); font-size: 11px; line-height: 1.55;
  white-space: pre-wrap; word-break: break-all;
}
```

- [ ] **Step 4: 验证**

```bash
yarn workspace @dsh-desktop/control-center typecheck
yarn build:web
ls dist/control-center/index.html
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement control-center dashboard with status and logs"
```

---

## Task 8: 控制中心设置 Tab（get_config / set_config）

**Files:**
- Modify: `apps/control-center/src/pages/Settings.tsx`（完整实现）、`apps/control-center/src/styles.css`（补充表单样式）

**Interfaces:**
- Consumes: `lib/ipc.ts` 的 `getConfig`/`setConfig`/`restart`（Task 7）；Rust 命令 `get_config`/`set_config`（Task 4）。
- Produces: 设置表单（DSH_BIN / DSH_NODE / DSH_HOME），保存后提示重启 dsh 生效。

- [ ] **Step 1: 实现 `apps/control-center/src/pages/Settings.tsx`**

```tsx
import { createEffect, createSignal, onCleanup } from "solid-js";
import type { DshConfig } from "@dsh-desktop/contracts";
import { getConfig, restart, setConfig } from "../lib/ipc";

export default function Settings() {
  const [dshBin, setDshBin] = createSignal("");
  const [dshNode, setDshNode] = createSignal("");
  const [dshHome, setDshHome] = createSignal("");
  const [saved, setSaved] = createSignal(false);

  createEffect(() => {
    let disposed = false;
    getConfig().then((c) => {
      if (disposed) return;
      setDshBin(c.dsh_bin ?? "");
      setDshNode(c.dsh_node ?? "");
      setDshHome(c.dsh_home ?? "");
    });
    onCleanup(() => {
      disposed = true;
    });
  });

  const save = async () => {
    const config: DshConfig = {
      dsh_bin: dshBin() || null,
      dsh_node: dshNode() || null,
      dsh_home: dshHome() || null,
    };
    await setConfig(config);
    setSaved(true);
  };

  return (
    <section class="settings">
      <label class="field">
        <span>DSH 入口 (DSH_BIN)</span>
        <input value={dshBin()} onInput={(e) => setDshBin(e.currentTarget.value)} placeholder="例如 /path/to/dsh" />
      </label>
      <label class="field">
        <span>Node 解释器 (DSH_NODE)</span>
        <input value={dshNode()} onInput={(e) => setDshNode(e.currentTarget.value)} placeholder="例如 /path/to/node" />
      </label>
      <label class="field">
        <span>Harness 数据目录 (DSH_HOME)</span>
        <input value={dshHome()} onInput={(e) => setDshHome(e.currentTarget.value)} placeholder="留空则继承环境变量" />
      </label>
      <div class="settings__actions">
        <button class="primary" onClick={save}>保存</button>
        <button onClick={() => restart().catch((e) => console.error(e))}>重启 dsh 生效</button>
      </div>
      {saved() && <p class="hint">已保存，重启 dsh 后生效。</p>}
    </section>
  );
}
```

- [ ] **Step 2: 补充 `apps/control-center/src/styles.css`**

```css
.settings { display: flex; flex-direction: column; gap: 14px; max-width: 520px; }
.field { display: flex; flex-direction: column; gap: 6px; font-size: 13px; }
.field input {
  padding: 8px 10px; border: 1px solid var(--line); border-radius: 5px;
  font-size: 13px; font-family: var(--mono); background: #fff;
}
.settings__actions { display: flex; gap: 10px; }
.hint { font-size: 12px; color: var(--muted); margin: 0; }
```

- [ ] **Step 3: 验证**

```bash
yarn workspace @dsh-desktop/control-center typecheck
yarn build:web
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement control-center settings tab with config persistence"
```

---

## Task 9: 控制中心工具 Tab（占位入口）

**Files:**
- Modify: `apps/control-center/src/pages/Tools.tsx`（完整实现占位）、`apps/control-center/src/styles.css`（补充列表样式）

**Interfaces:**
- Consumes: 无外部依赖。
- Produces: 工具入口列表（占位，后续扩展）。

- [ ] **Step 1: 实现 `apps/control-center/src/pages/Tools.tsx`**

```tsx
const TOOLS = [
  { key: "logs", title: "打开日志目录", desc: "打开 dsh 运行日志目录", action: "open_logs" },
];

export default function Tools() {
  return (
    <section class="tools">
      <h2>工具</h2>
      <ul>
        {TOOLS.map((tool) => (
          <li class="tool">
            <span class="tool__title">{tool.title}</span>
            <span class="tool__desc">{tool.desc}</span>
          </li>
        ))}
      </ul>
      <p class="hint">更多工具入口将在此扩展。</p>
    </section>
  );
}
```

- [ ] **Step 2: 补充 `apps/control-center/src/styles.css`**

```css
.tools ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 10px; }
.tool {
  display: flex; flex-direction: column; gap: 4px; padding: 12px 14px;
  border: 1px solid var(--line); border-radius: 6px; background: #fff;
}
.tool__title { font-size: 14px; }
.tool__desc { font-size: 12px; color: var(--muted); }
```

- [ ] **Step 3: 验证**

```bash
yarn workspace @dsh-desktop/control-center typecheck
yarn build:web
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add control-center tools tab placeholder"
```

---

## Task 10: CI 路径更新 + AGENTS.md/README 更新 + 全量验证

**Files:**
- Modify: `.github/workflows/build.yml`（路径与构建命令）、`AGENTS.md`、`apps/shell/AGENTS.md`、`apps/shell/src-tauri/AGENTS.md`、`apps/control-center/AGENTS.md`（新建）、`packages/contracts/AGENTS.md`（新建）、`README.md`

**Interfaces:**
- Consumes: 全部先前任务。
- Produces: 与 monorepo 布局一致的 CI、AGENTS.md 与 README。

- [ ] **Step 1: 更新 `.github/workflows/build.yml`**

- PR 触发路径从 `src/**`、`src-tauri/**` 改为 `apps/shell/**`、`apps/control-center/**`、`packages/**`、`package.json`、`yarn.lock`。
- “Build Tauri app” 步骤命令从 `corepack yarn tauri build --bundles ...` 改为 `corepack yarn build --bundles ...`（根脚本 → shell workspace）。
- artifact 路径保持 `apps/shell/src-tauri/target/release/bundle/...`（路径前缀变化：`src-tauri/target/...` → `apps/shell/src-tauri/target/...`）。

```yaml
# build.yml 中需改动的三处
on:
  pull_request:
    paths:
      - "apps/shell/**"
      - "apps/control-center/**"
      - "packages/**"
      - "package.json"
      - "yarn.lock"

# 构建步骤
run: corepack yarn build --bundles ${{ matrix.bundles }}

# 产物路径
bundle: |
  apps/shell/src-tauri/target/release/bundle/dmg/*.dmg
  # ... 其余平台类似，均在 apps/shell/src-tauri/target/release/bundle/ 下
```

> `bump-version.mjs` 与 release job 的 rename 步骤读取根 `package.json` 与根 `CHANGELOG.md`，无需改动。

- [ ] **Step 2: 更新根 `AGENTS.md`**

将模块划分改为 apps/shell / apps/control-center / packages/contracts / .github，补充命令表（`yarn dev`、`yarn build`、`yarn dev:web`、`yarn build:web`、`yarn typecheck`），并新增“配置优先级 config.json → 环境变量 → PATH”与“control 窗口按需打开（菜单项 / CmdOrCtrl+Shift+C）”条目。

- [ ] **Step 3: 更新 `apps/shell/AGENTS.md` 与 `apps/shell/src-tauri/AGENTS.md`**

- `apps/shell/AGENTS.md`：指向新路径（`../apps/shell/src` 的 index.html 在 apps/shell 根），补充 TypeScript 说明。
- `apps/shell/src-tauri/AGENTS.md`：补充 DshConfig/config.json、get_config/set_config、control 窗口、菜单与快捷键、解析优先级。

- [ ] **Step 4: 新建 `apps/control-center/AGENTS.md` 与 `packages/contracts/AGENTS.md`**

`apps/control-center/AGENTS.md` 内容模板：

```markdown
# AGENTS.md — apps/control-center（SolidJS 控制中心）

DSH Desktop 的第二个窗口：SolidJS SPA（Vite 8 + vite-plugin-solid + TypeScript），
按需打开（应用菜单项“控制中心” / CmdOrCtrl+Shift+C），包含仪表盘 / 设置 / 工具三个 Tab。

## 文件

- `src/index.tsx` — 入口，挂载 App
- `src/App.tsx` — 三 Tab 布局（无路由库，状态切换）
- `src/pages/Dashboard.tsx` — 状态 / 日志 / 控制按钮
- `src/pages/Settings.tsx` — DSH_BIN / DSH_NODE / DSH_HOME 表单（get_config/set_config）
- `src/pages/Tools.tsx` — 工具入口占位
- `src/lib/ipc.ts` — 全部 Tauri 调用与事件监听的唯一入口
- `src/styles.css` — 全部样式

## 构建与开发

- dev server @ http://localhost:5174（strictPort）；`base: "/control-center/"`
- 构建输出 `../../dist/control-center`（构建顺序：shell 先，control-center 后）
- 窗口 URL：release 为 `/control-center/index.html`，dev 下 Rust 导航到 :5174/control-center/

## 通信契约

- IPC 命令：`get_status` / `install_dsh` / `restart` / `open_log_directory` / `get_config` / `set_config`
- 事件：`dsh-status`（RuntimeSnapshot）、`dsh-log`（文本行）
- 类型与常量来自 `@dsh-desktop/contracts`；修改契约须同步 Rust serde 类型与 `apps/shell/src`
```

`packages/contracts/AGENTS.md` 内容模板：

```markdown
# AGENTS.md — packages/contracts（共享 IPC 类型）

前后端 IPC 契约的 TypeScript 类型与常量定义，供 `apps/shell`（启动页）与
`apps/control-center`（控制中心）共用。

## 内容

- `RuntimePhase` — phase 联合类型（snake_case，与 Rust `RuntimePhase` serde 一致）
- `RuntimeSnapshot` — `get_status` 返回（phase/message/url/dsh_installed/node_found/install_dir/log_dir/logs）
- `DshConfig` — `get_config` / `set_config` 的配置结构（dsh_bin/dsh_node/dsh_home，null 表示未设置）
- `COMMANDS` / `EVENTS` — IPC 命令名与事件名字符串常量

## 同步规则

修改任何 IPC 契约时，必须同步四处：`packages/contracts/src/index.ts`、
Rust 端 `apps/shell/src-tauri/src/lib.rs`（及 config.rs）的 serde 类型、
`apps/shell/src/main.ts`、`apps/control-center/src/lib/ipc.ts`。
字段命名统一 snake_case（Rust serde `rename_all = "snake_case"`，TS 侧直接写 snake_case）。
```

- [ ] **Step 5: 更新 `README.md`**

目录结构章节更新为 monorepo 布局；补充多窗口说明（main 承载 dsh，control 为 SolidJS 控制中心）；命令章节同步。

- [ ] **Step 6: 全量验证**

```bash
yarn typecheck
yarn build:web
cd apps/shell/src-tauri && cargo test && cargo clippy -- -D warnings && cargo fmt --check && cd ../..
git status   # 确认无遗漏文件
```

> 手动冒烟（可选，需 GUI）：`yarn dev` 后验证 ① main 窗口启动页 → dsh 就绪 → 导航到 dsh web；② 菜单项 / CmdOrCtrl+Shift+C 打开 control 窗口且重复调用只聚焦；③ 设置保存生成 config.json，重启后生效；④ 控制中心日志随事件实时更新。

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "docs: update CI paths and module AGENTS.md for monorepo"
```
