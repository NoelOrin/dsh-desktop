# 内嵌插件自动挂载（Overlay 装配）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** 桌面应用把 packages/plugins/ 下所有插件编译产物内嵌进安装包；应用启动时把产物装入 dsh 的 profile 兜底 node_modules，并通过 `dsh web --patch <overlay>` 让插件随 dsh 启动自动挂载——全程不写 profile manifest、不调用 pnpm/下载。

**Architecture:** 利用 dsh 0.1.0-rc.6 的三个已核实的机制：① loader 的 host 入口与 dsh-client-modules 的 client 面扫描都按包名做 Node 模块解析，起点是 profile 目录；② dsh 自维护 $DSH_HOME/profiles/node_modules/ 兜底目录（pnpm 永不清理），任何包放进去都能被解析；③ launcher 的 web 子命令自带 --patch <file>（repeatable），overlay 的 - insert: 行直接插入插件行。因此：Rust 侧把内嵌包复制到兜底目录 + 生成 overlay 文件 + 启动命令加 --patch，插件即自动挂载，两个面（host/client）同时生效。

**Tech Stack:** TypeScript（插件源码）、tsdown@0.22（双面构建：host=esm/node，client=cjs/browser+UMD 包装）、Rust（Tauri 2 后端装配）、serde_json、dirs。

**Spec:** 2026-08-14 会话中已确认的设计（方案 B：overlay 装配；装配位置 profiles/node_modules 兜底目录；范围 packages/plugins/* 全部内嵌）。

## Global Constraints

- dsh 基线 0.1.0-rc.6；挂载机制依赖：profile 目录解析包名（dsh-app-boot / dsh-client-modules）+ launcher --patch overlay。
- 不写 dsh.profile.bundles、不跑 dsh plugin、不调用 pnpm/npm。
- 装配目标：$DSH_HOME/profiles/node_modules/@dsh-desktop/<name>/（home 与 dsh 一致：config.dsh_home → ~/.dsh）。
- YAML 里插件名必须单引号包裹（@ 是 YAML 1.1 保留指示符，实测裸写报错）。
- 产物自包含：内嵌 package.json 不含 dependencies/devDependencies/peerDependencies（host 无运行时依赖；client externals 由浏览器模块表解析）。
- 装配是 best-effort：单包失败仅记日志跳过；resources 缺失时整体跳过并不带 --patch，保证 dsh web 永远可启动。
- 复制以版本号为键：缺失或版本不同才重装（幂等、自愈）。
- 范围：packages/plugins/ 下所有子目录（build 脚本按目录扫描，未来新增插件自动纳入）。
- 用户可见文案与代码注释用中文；提交遵循 Conventional Commits；代码过 Biome（space/2 缩进、双引号、分号、尾逗号）。

## 文件总览

| 动作 | 文件 | 职责 |
| --- | --- | --- |
| Create | packages/plugins/bridge/tsdown.config.ts | bridge 双面构建配置（host + client） |
| Create | packages/plugins/hello/tsdown.config.ts | hello host 构建配置 |
| Modify | packages/plugins/bridge/package.json | main/exports 指向 lib、加 build 脚本、加 dshDesktop.id |
| Modify | packages/plugins/hello/package.json | 同上（无 client 面） |
| Create | scripts/build-plugins.mjs | 构建全部插件 → 组装 dist 包 → 拷入 src-tauri/resources/plugins |
| Create | scripts/build-plugins.test.mjs | dist 包清单变换的纯函数单测（node:test） |
| Modify | package.json | 加 build:plugins |
| Modify | apps/shell/src-tauri/tauri.conf.json | bundle.resources + beforeBuildCommand 串联 |
| Modify | .gitignore | 忽略 lib/dist/resources 产物 |
| Create | apps/shell/src-tauri/src/embedded.rs | 装配（复制+版本比对）+ overlay 生成 + 单测 |
| Modify | apps/shell/src-tauri/src/lib.rs | mod embedded; + start() 集成 --patch |
| Modify | apps/shell/src-tauri/Cargo.toml | 加 dirs |
| Modify | AGENTS.md / packages/plugins/AGENTS.md / docs/plugin-tauri-boundary.md | 文档化新装配机制 |

---

## Task 1: tsdown 双面构建管线

**Files:**
- Create: packages/plugins/bridge/tsdown.config.ts
- Create: packages/plugins/hello/tsdown.config.ts
- Modify: packages/plugins/bridge/package.json、packages/plugins/hello/package.json
- Modify: package.json（root devDependencies）
- Test: 构建产物断言（命令行检查）

**Interfaces:**
- Produces: packages/plugins/<name>/lib/index.js（host ESM）、packages/plugins/bridge/lib/client.js（client UMD 包装）；dshDesktop.id 出现在两个包源 package.json 的 dsh 字段旁。

- [ ] Step 1: 安装 tsdown

  corepack yarn add -W -D tsdown@^0.22
  corepack yarn tsdown --version   # 预期打印 0.22.x

- [ ] Step 2: 给两个插件包源 package.json 加 dshDesktop.id 与 build 脚本

packages/plugins/bridge/package.json（在 dsh 字段同级加 dshDesktop；main/exports 指向 lib；scripts 加 build）：

```jsonc
{
  "main": "./lib/index.js",
  "types": "./lib/index.d.ts",
  "exports": {
    ".": "./lib/index.js",
    "./client": "./lib/client.js",
    "./cordis.patch.yml": "./cordis.patch.yml",
    "./package.json": "./package.json"
  },
  "scripts": {
    "build": "tsdown",
    "typecheck": "tsc --noEmit"
  },
  "dshDesktop": { "id": "bridge" },
  // 其余字段（name/version/type/dsh...）保持不变
}
```

packages/plugins/hello/package.json 同理（无 client 面，dshDesktop.id: "hello"，main → ./lib/index.js）。

> 注意：exports 里 ./cordis.patch.yml 和 ./package.json 保留；dependencies 保留用于 typecheck（dist 副本在 Task 2 剔除）。

- [ ] Step 3: 写 bridge 构建配置

packages/plugins/bridge/tsdown.config.ts：

```ts
import { defineConfig } from "tsdown";

/** 浏览器平台模块表（复制自 deepseek-harness packages/client/web/src/platform.ts）。 */
const PLATFORM_MODULES = [
  "react",
  "react/jsx-runtime",
  "react-dom",
  "react-dom/client",
  "@deepseek-ai/cordis",
  "@deepseek-ai/dsh-client-ui-slots",
  "@deepseek-ai/dsh-client-web-react",
  "@deepseek-ai/dsh-client-ui-primitives",
  "@deepseek-ai/dsh-client-ui-attachment",
  "@deepseek-ai/dsh-client-schema-form",
];

/** 文档化的临时豁免（deepseek-harness 同款）。 */
const RUNTIME_STORE_EXEMPTION = "@deepseek-ai/dsh-client-runtime/client";

const CLIENT_EXTERNALS = [...PLATFORM_MODULES, RUNTIME_STORE_EXEMPTION];

export default defineConfig([
  {
    // host 面：Node ESM
    name: "@dsh-desktop/plugin-bridge",
    entry: ["src/index.ts"],
    outDir: "lib",
    format: ["esm"],
    platform: "node",
    target: "es2024",
    dts: false,
    clean: true,
  },
  {
    // client 面：浏览器 CJS + __ModuleLoader__ 包装（tsdown.client.ts 协议）
    name: "@dsh-desktop/plugin-bridge/client",
    entry: { client: "src/client.tsx" },
    outDir: "lib",
    format: "cjs",
    platform: "browser",
    dts: false,
    sourcemap: true,
    clean: false,
    external: CLIENT_EXTERNALS,
    define: {
      "process.env.NODE_ENV": JSON.stringify("production"),
      "import.meta.env.MODE": JSON.stringify("production"),
      "import.meta.env": JSON.stringify({ MODE: "production" }),
    },
    noExternal: (id: string) => (CLIENT_EXTERNALS.includes(id) ? undefined : true),
    plugins: [
      {
        // purity gate：非平台模块的 @deepseek-ai 值导入直接构建失败
        name: "dsh-client-bundle-purity",
        resolveId(source: string) {
          if (!source.startsWith("@deepseek-ai/")) return null;
          if (CLIENT_EXTERNALS.includes(source)) return null;
          throw new Error(
            `client bundle purity: "${source}" is not a platform module - cross-plugin value imports are forbidden; collaborate through cordis services`,
          );
        },
      },
    ],
    outputOptions: {
      entryFileNames: "client.js",
      banner:
        'window.__ModuleLoader__.load({ id: "@dsh-desktop/plugin-bridge", factory: (require) => {',
      footer: "return module.exports; } });",
      intro: "var module = { exports: {} }; var exports = module.exports;",
    },
  },
]);
```

- [ ] Step 4: 写 hello 构建配置

packages/plugins/hello/tsdown.config.ts：只有 host 面（同 Step 3 第一个 config，name 换 @dsh-desktop/plugin-hello）。

- [ ] Step 5: 构建并断言产物

  corepack yarn workspace @dsh-desktop/plugin-bridge build
  corepack yarn workspace @dsh-desktop/plugin-hello build
  ls packages/plugins/bridge/lib/   # index.js + client.js + client.js.map
  head -c 120 packages/plugins/bridge/lib/client.js   # 以 window.__ModuleLoader__.load({ 开头
  tail -c 60 packages/plugins/bridge/lib/client.js    # 以 return module.exports; } }); 结尾
  grep -c "^import" packages/plugins/bridge/lib/index.js  # 预期 0（host 无运行时 import）

- [ ] Step 6: typecheck 仍通过

  corepack yarn typecheck

- [ ] Step 7: Commit

  git add packages/plugins/bridge packages/plugins/hello package.json yarn.lock
  git commit -m "build: 插件 tsdown 双面构建（host ESM + client UMD 包装）"

---

## Task 2: 构建脚本与 Tauri resources 集成

**Files:**
- Create: scripts/build-plugins.mjs、scripts/build-plugins.test.mjs
- Modify: package.json、apps/shell/src-tauri/tauri.conf.json、.gitignore
- Test: node scripts/build-plugins.test.mjs

**Interfaces:**
- Consumes: Task 1 的 packages/plugins/<name>/lib/ 产物、源 package.json 的 dsh/dshDesktop 字段。
- Produces: apps/shell/src-tauri/resources/plugins/<name>/（含 package.json / cordis.patch.yml / lib/*）；纯函数 buildDistManifest(src: object): object（test 直接测）。

- [ ] Step 1: 写失败测试（纯函数变换）

scripts/build-plugins.test.mjs（node:test，参照 scripts/lan-proxy.test.mjs 风格）：

```js
import test from "node:test";
import assert from "node:assert/strict";
import { buildDistManifest } from "./build-plugins.mjs";

test("dist manifest 剔除依赖并指向 lib", () => {
  const dist = buildDistManifest({
    name: "@dsh-desktop/plugin-bridge",
    version: "0.1.0",
    type: "module",
    main: "./lib/index.js",
    exports: { ".": "./lib/index.js", "./client": "./lib/client.js" },
    dependencies: { "@deepseek-ai/cordis": "^4.0.1" },
    devDependencies: { typescript: "^5" },
    peerDependencies: { react: "^18" },
    private: true,
    dsh: { bundle: { patch: "./cordis.patch.yml" } },
    dshDesktop: { id: "bridge" },
  });
  assert.equal(dist.dependencies, undefined);
  assert.equal(dist.devDependencies, undefined);
  assert.equal(dist.peerDependencies, undefined);
  assert.equal(dist.private, undefined);
  assert.equal(dist.main, "./lib/index.js");
  assert.equal(dist.dsh.bundle.patch, "./cordis.patch.yml");
  assert.equal(dist.dshDesktop.id, "bridge");
});
```

- [ ] Step 2: 运行确认失败

  node scripts/build-plugins.test.mjs   # 预期 FAIL：buildDistManifest 未导出

- [ ] Step 3: 写 scripts/build-plugins.mjs

核心：对 packages/plugins/* 每个含 dsh 字段的子目录——① spawnSync corepack yarn workspace <pkg> build；② 读源 package.json，用 buildDistManifest 变换（保留 name/version/type/main/exports/dsh/dshDesktop/description，剔除 private/dependencies/devDependencies/peerDependencies/scripts/packageManager）；③ 把 package.json、cordis.patch.yml、lib/（含 .map）拷入 apps/shell/src-tauri/resources/plugins/<name>/（先清空旧目录）；④ 打印清单。任何一步失败 → 非零退出。buildDistManifest 作为具名导出供测试。

- [ ] Step 4: 跑测试 + 真跑一次

  node scripts/build-plugins.test.mjs    # PASS
  corepack yarn build:plugins            # 打印 bridge/hello 已装配
  find apps/shell/src-tauri/resources/plugins -maxdepth 3 | sort

跑两次验证幂等。

- [ ] Step 5: tauri.conf.json 与 .gitignore

apps/shell/src-tauri/tauri.conf.json：
- bundle.resources 加 ["resources/plugins/*"]（相对 src-tauri、保留目录结构 → $RESOURCE/plugins/<name>/，Tauri 2.11.4 已核实）。
- build.beforeBuildCommand 改为 corepack yarn web:build && corepack yarn build:plugins。
- build.beforeDevCommand 不动（web:all 是长驻进程，不能串联；dev 缺资源由 Rust 侧容错）。

.gitignore 追加：

```
packages/plugins/*/lib
packages/plugins/*/dist
apps/shell/src-tauri/resources
```

- [ ] Step 6: Commit

  git add scripts/build-plugins.mjs scripts/build-plugins.test.mjs package.json apps/shell/src-tauri/tauri.conf.json .gitignore
  git commit -m "build: 插件编译产物打包进 Tauri resources（build:plugins）"

---

## Task 3: Rust 装配模块（TDD）

**Files:**
- Create: apps/shell/src-tauri/src/embedded.rs
- Modify: apps/shell/src-tauri/src/lib.rs（mod embedded; + start() 集成）、apps/shell/src-tauri/Cargo.toml（dirs = "5"）
- Test: embedded.rs 内 #[cfg(test)]

**Interfaces:**
- Consumes: Task 2 的 resource_dir()/plugins/<name>/（package.json 含 dshDesktop.id）；DshManager 已有的 app（AppHandle）、append_log、config_path。
- Produces:
  - pub fn assemble(resource_dir: &Path, home: &Path, log: &mut dyn FnMut(&str)) -> Vec<MountedPlugin>
  - pub fn write_overlay(dir: &Path, mounted: &[MountedPlugin]) -> Option<PathBuf>
  - pub struct MountedPlugin { pub id: String, pub name: String }

- [ ] Step 1: 写失败测试

在 embedded.rs（先建文件含模块骨架与 #[cfg(test)]）写：

```rust
#[test]
fn write_overlay_quotes_names() {
    let dir = temp_dir().join(format!("dsh-overlay-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mounted = vec![
        MountedPlugin { id: "bridge".into(), name: "@dsh-desktop/plugin-bridge".into() },
        MountedPlugin { id: "hello".into(), name: "@dsh-desktop/plugin-hello".into() },
    ];
    let path = write_overlay(&dir, &mounted).expect("overlay 应生成");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("- insert:\n    - id: bridge\n      name: '@dsh-desktop/plugin-bridge'\n"));
    assert!(content.contains("- insert:\n    - id: hello\n      name: '@dsh-desktop/plugin-hello'\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_overlay_none_when_empty() {
    assert!(write_overlay(&temp_dir(), &[]).is_none());
}

#[test]
fn assemble_copies_when_missing_and_version_differs() {
    // 构造：resources/plugins/bridge/{package.json, cordis.patch.yml, lib/index.js}
    // 目标 home/profiles/node_modules/@dsh-desktop/plugin-bridge
    // 1) 缺失 → 复制，返回 mounted 含 id/name
    // 2) 同版本 → 不再复制（改源文件内容后目标不变）
    // 3) 版本不同 → 重新复制
    // 4) 无 dshDesktop.id 的目录 → 跳过
    // 5) resources/plugins 不存在 → 空 Vec，不 panic
}
```

- [ ] Step 2: 运行确认失败

  cd apps/shell/src-tauri && cargo test embedded 2>&1 | tail -20

（先只写测试骨架，预期编译失败/测试失败。）

- [ ] Step 3: 实现 embedded.rs

要点：
- MountedPlugin { id, name }（version 不进 struct，仅用于比对）。
- assemble：遍历 resource_dir/plugins/* 子目录；serde_json 读 package.json（字段 name/version/dshDesktop.id）；目标 home/profiles/node_modules/<name>；目标 package.json 版本不同或缺失 → 先清旧 tmp、拷到 <name>.tmp、删旧目标、rename（原子）；成功才 push 进 mounted。单包失败 log 后 continue；plugins 根目录不存在直接返回空。
- write_overlay：空列表返回 None；否则写 <dir>/embedded-plugins.patch.yml，行格式：- insert:\n    - id: <id>\n      name: '<name>'\n（name 单引号）。
- 递归复制用 std::fs 手写（不新增 fs_extra）。

- [ ] Step 4: lib.rs 集成

在 start() 里（构造 command 之前）：

```rust
// 装配内嵌插件（best-effort）并生成 --patch overlay
let resource_dir = self.app.path().resource_dir()?;
let home_path = home.map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".dsh")));
let mut log = |line: &str| self.append_log(line);
let mounted = home_path
    .map(|h| embedded::assemble(&resource_dir, &h, &mut log))
    .unwrap_or_default();
let overlay = if let Some(data_dir) = self.app.path().app_data_dir() {
    embedded::write_overlay(&data_dir, &mounted)
} else { None };

let mut args = vec!["web".to_string()];
if let Some(overlay_path) = overlay {
    args.push("--patch".into());
    args.push(overlay_path.to_string_lossy().into_owned());
}
args.extend(["--host".into(), "127.0.0.1".into(), "--port".into(), port.to_string()]);
cmd.args(&args);
```

文件头 mod embedded;；Cargo.toml 加 dirs = "5"。注意按实际 Tauri PathResolver API 签名调整（resource_dir()/app_data_dir() 返回 Result）。

- [ ] Step 5: 跑测试 + cargo check/clippy/fmt

  cd apps/shell/src-tauri && cargo test && cargo clippy && cargo fmt

- [ ] Step 6: Commit

  git add apps/shell/src-tauri/src/embedded.rs apps/shell/src-tauri/src/lib.rs apps/shell/src-tauri/Cargo.toml apps/shell/src-tauri/Cargo.lock
  git commit -m "feat: 桌面启动装配内嵌插件并生成 --patch overlay 自动挂载"

---

## Task 4: 文档与端到端验证

**Files:**
- Modify: AGENTS.md（模块划分表加 build:plugins；关键流程补装配说明）、packages/plugins/AGENTS.md（构建步骤 + 装配机制）、docs/plugin-tauri-boundary.md（§6 安装行改为内嵌装配；新增装配机制小节）

**Interfaces:**
- Consumes: Task 1-3 的全部产物与命令。

- [ ] Step 1: 更新文档

三处文档按"编译 → 内嵌 resources → 启动装配 → overlay 挂载"的新机制改写对应小节，并注明：装配位置 $DSH_HOME/profiles/node_modules、不写 profile manifest、卸载后无引用残留、单独 dsh web 不挂载内嵌插件（方案 B 语义）。

- [ ] Step 2: 端到端验证清单（手工执行并记录结果）

  # 1. 产物
  corepack yarn build:plugins && find apps/shell/src-tauri/resources/plugins -type f | sort
  # 2. Rust 测试
  cd apps/shell/src-tauri && cargo test
  # 3. 完整启动（会触发 tauri build 前的 web:build + build:plugins）
  corepack yarn dev

启动后验证：
- 日志出现 [desktop] 装配插件 @dsh-desktop/plugin-bridge@0.1.0 等行；无 "client bundle not found" 报错。
- 浏览器打开 http://127.0.0.1:<port>/plugins/bridge/client.js?rev=...（从页面 window.__DSH_BOOT__ 拿 url）返回 UMD 包装内容。
- Web UI 设置面板出现"桌面"节 + "开机自启"开关（bridge client 面生效）。
- 单独 dsh web（不带 --patch）启动 → 设置面板没有"桌面"节（方案 B 语义正确）。
- 删掉 apps/shell/src-tauri/resources/plugins 后重启桌面 → 不报错、正常进 dsh、无插件（容错生效）。

- [ ] Step 3: Commit

  git add AGENTS.md packages/plugins/AGENTS.md docs/plugin-tauri-boundary.md
  git commit -m "docs: 内嵌插件自动挂载机制（overlay 装配）"

