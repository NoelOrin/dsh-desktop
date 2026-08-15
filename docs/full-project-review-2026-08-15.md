# DSH Desktop 全项目 4 轮 Review（2026-08-15）

## 范围与方法

- 审查对象：当前工作区全项目源码与配置，包含未提交改动和新增的 `reasoning` 插件。
- 方法：4 轮独立视角审查，不做代码修改。
  1. Rust 后端、进程生命周期、Tauri 权限与安全边界。
  2. 前端、契约、bridge/theme/shortcuts/reasoning 插件客户端。
  3. 构建脚本、热更新、lan-proxy、CI 与文档一致性。
  4. 跨模块契约、权限、竞态与可复现验证。
- 验证命令：
  - `yarn typecheck`：通过。
  - `node --test scripts/build-plugins.test.mjs`：5/5 通过。
  - `node --test scripts/lan-proxy.test.mjs`：7/7 通过。
  - `cargo test --lib`：49/49 通过。

## 结论

当前代码基线可构建、可运行，已有单元测试覆盖了进程解析、插件装配、主题与 lan-proxy 的核心路径。主要风险集中在“远端 dsh 页面获得 Tauri 桥接后的攻击面”和若干会破坏用户数据的契约/状态问题，建议在合并 native 功能前优先处理 P0/P1。

## 第 1 轮：Rust 后端与安全边界

- `apps/shell/src-tauri/tauri.conf.json:15` P0：`csp` 为 `null`。dsh web 是远端 origin，桥接脚本又暴露了高权限命令；一旦页面存在 XSS，攻击面会直接放大。启用白名单 CSP，或至少对动态页面保持严格 `script-src`/`connect-src`。
- `apps/shell/src-tauri/src/lib.rs:63` + `apps/shell/src-tauri/src/inject.rs:4` P0：导航策略和桥接注入都只按“任意 loopback host”判定，不校验是否当前托管的 dsh URL。dsh 页面里一个指向其他 `127.0.0.1:*` 服务的链接会把 `window.__DSH_DESKTOP__` 注入给该服务。改为只允许当前 `inner.url` 对应的 origin，或给 dsh URL 加一次性 token/端口校验。
- `apps/shell/src-tauri/capabilities/bridge.json:24-49` P0：remote 白名单一次性开放了 `set_config`、`open_external`、`install_update`、`update_dsh`、全局快捷键、项目等命令。即使本意是给自家 bridge client 用，任何挂载后的 dsh 页面都能调用同一套命令。按实际 consumer 拆分 capability，并删除 bridge client 不直接使用的命令。
- `apps/shell/src-tauri/src/lib.rs:2102` P1：`open_external` 不校验目标类型。桥接面应限制为 `https?://` URL、已确认存在的文件/目录，或由壳侧生成的白名单路径，避免任意路径/URL 被打开。
- `apps/shell/src-tauri/src/lib.rs:3057` P1：读取 shell PATH 时执行登录交互式 shell（`-l -i`），会运行用户 shell 配置，也可能被长驻子进程拖住 stdout 读取。改为非交互式 `-l -c` 或直接解析 PATH 配置，并给整条管道设置硬超时。
- `apps/shell/src-tauri/src/lib.rs:1981` P1：`get_ui_theme` 只读 `DSH_HOME` 环境变量，启动和主题轮询则走 `config::load().effective()`。用户把 `dsh_home` 写在 `config.json` 时，启动页初始主题可能与实际数据目录不一致。统一用有效配置解析 home。
- `apps/shell/src-tauri/src/lib.rs:2377` P1：`unregister_shortcut` 先把快捷键从内存 map 删除，再调 OS unregister；OS 失败后内存/持久化状态已经不一致。先 unregister 成功，再更新 map 和 `config.json`。
- `apps/shell/src-tauri/src/lib.rs:2124` P2：`get_desktop_settings` 不回退到 `settings.yaml` 的 `desktop.startupMode`，但启动路径会回退。迁移期间 UI 显示和实际启动模式可能不一致。
- `apps/shell/src-tauri/src/lib.rs:2096` P2：`set_config` 不校验路径，`update_project` 也不校验新路径是否为目录。避免无效配置写入后只到重启/文件管理操作才暴露。
- `apps/shell/src-tauri/src/embedded.rs:122` P2：插件目录名直接拼到 `profiles/node_modules/<name>`，对构建产物可信假设过强。为内嵌插件增加名字段校验（禁止 `..`、路径分隔符、YAML 特殊字符）。

## 第 2 轮：前端、契约与插件客户端

- `packages/plugins/bridge/src/client/runtime.ts:41` P0：桥接侧 `DshConfig` 漏掉 `shortcuts`。`DesktopPanel.tsx:273` 保存配置时只传 `dsh_bin/dsh_node/dsh_home`，Rust 对缺失 `shortcuts` 使用默认空数组，会把已注册的全局快捷键持久化记录清空。在 runtime 类型和保存对象中补回 `shortcuts`，或让 `set_config` 合并现有快捷键。
- `packages/plugins/bridge/src/client/CustomThemeEditor.tsx:182-190` P1：编辑已有主题时改名会同步改 `id`，但没有 `ensureUniqueThemeId`，也没有删除旧 id。可能覆盖另一个同名主题，或留下旧主题。保存时应保留稳定 id，或明确执行“旧 id 删除 + 新 id 插入”。
- `packages/plugins/bridge/src/shared/theme.ts:419-479` P1：导入主题只校验 JSON 外层字段，颜色、contrast、`overrides` 均未规范化。`deriveThemeTokens` 会把任意 override 写入 `body.style.setProperty`，并可能产出无效颜色。导入时用 `normalizeHexColor`/数字 clamp 规范种子，并只允许 `--dsw-alias-*` 白名单 override。
- `packages/plugins/shortcuts/src/client.tsx:415-418` P1：更新已启用快捷键时先 unregister 旧键，再 register 新键；新键被占用时旧键已经失效。先注册新键成功，再注销旧键，或失败时回滚。
- `packages/plugins/shortcuts/src/client.tsx:577-587` P2：`unregisterAll` 成功但本地 preset 写失败时，下次装载会按仍启用的 preset 重新注册，和“全部移除”语义冲突。先持久化停用 presets，再注销；失败时提示并可重试。
- `packages/plugins/reasoning/src/index.ts:41-56` P1：host 面直接改写共享服务对象 `llm.resolveModelInfo`，没有 disposer；HMR 或另一个插件再包装时可能嵌套包装并永久叠加。优先通过 cordis 服务注册/事件包装，或至少记录原始函数并在 effect 清理时恢复。
- `packages/plugins/projects/src/index.ts:302-343` P1：`webServer.register` 返回的 disposer 被忽略。插件 HMR/卸载时旧端点可能残留。应在 `ctx.effect` 中收集并返回 disposer。
- `packages/plugins/projects/src/index.ts:290-299` P2：`readBody` 无大小上限，JSON body 可被本地请求撑爆内存。设置 body 上限并拒绝超大请求。
- `packages/plugins/bridge/src/client/desktop-shell.ts:65-92` P2：窗口控制按钮用 `innerHTML` 生成静态 SVG，当前无注入风险，但后续若拼接动态内容需改为 DOM API。
- `packages/plugins/reasoning/src/models/client/ModelListEditor.tsx:226-249` P2：容量编辑 buffer 以行号做 key，采纳候选后重建行序可能把未提交文本映射到错误行。建议改用模型 id 或候选稳定键。
- `packages/plugins/bridge/src/client/theme-store.ts:101-115` P2：防抖写回逐个 `await scope.set`，失败被吞掉且无 UI 反馈。主题设置静默丢失时用户无法感知；至少记录失败或暴露错误状态。

## 第 3 轮：脚本、CI 与文档

- `scripts/lan-proxy.mjs:63-66` P1：默认绑定 `0.0.0.0:8080` 且 token 关闭。`yarn lan-proxy` 即可把本机 dsh 的完整权限暴露给局域网；应默认要求 token，或默认只绑 loopback 并在显式参数下才监听外网。
- `.github/scripts/bump-version.mjs:122-124` / `151-153` P1：先 push 到 `release` 再打 tag，push 本身会触发另一条 release 流水线，可能和当前 job 并发抢版本/tag。应先打 tag，或合并为一个原子 push，并在 workflow 层用 tag 事件去重。
- `.github/workflows/build.yml:8-12` P2：PR 触发路径只覆盖 `apps/shell/**`、`packages/**`、`package.json`、`yarn.lock`。修改 `scripts/`、`.github/`、根配置不会跑 CI，构建脚本回归可能漏检。
- `scripts/build-plugins.mjs:61-63` P2：`deployToProfile` 先删目标再 rename，rename 失败会丢失旧版本。与 Rust `embedded.rs` 相同的原子替换问题，建议使用临时目录 + 目录替换，失败时保留旧目标。
- `scripts/dev.mjs:28` P2：Vite 进程没有显式 `cwd`，依赖调用方 cwd 恰好是 shell workspace。补 `cwd: path.join(ROOT, "apps/shell")` 更稳。
- `README.en.md:107-109` P2：英文 README 仍把 `projects` 描述为“项目设置插件”，和当前 host-only 实现不一致；英文文档也未同步外观页能力。
- `CHANGELOG.md:3-9` P3：两个 `0.1.0` 小节重复，发布脚本可能截取错误内容。

## 第 4 轮：跨模块契约、竞态与交叉验证

- `packages/contracts/src/index.ts:104` P2：`WindowAction` 只包含 `minimize/maximize/close`，但 bridge 契约和 Rust 都支持 `toggle-visible`。native 契约缺项会造成类型/能力表漂移。
- `packages/plugins/bridge/src/index.ts:49` P2：`DshDesktopBridge.windowAction` 的返回值和实际调用未区分本地/remote capability；当前 remote 没有 `core:window` 全量权限，主要依赖 `window_action` 命令，契约层应明确命令切片而非 core API 形状。
- `apps/shell/src-tauri/src/lib.rs:1000-1020` P2：二次实例 payload 没有去重/上限，恶意/高频深链可无限堆积 `pending_deeplinks`。加队列上限和过期清理。
- `apps/shell/src-tauri/src/lib.rs:2434-2445` P2：`latest_release_tag` 与下载请求没有 timeout；桌面“检查更新”在无网络时可能长时间挂起。为 reqwest 设置明确 timeout。
- `apps/shell/src-tauri/src/lib.rs:1644-1650` P2：启动后立即 spawn 两个 stdout/stderr reader 和 health checker，但 manager 线程仅在 250ms 轮询中处理 exit；进程启动后立刻崩溃时，`ReadyTimeout` 和 `ReadinessError` 可能都发出。当前 generation 校验能兜住，但可增加日志去重。
- `packages/plugins/projects/src/index.ts:226-283` P2：action 端点对 `id` 只做字符串匹配，没有校验来源页面；loopback 内其他本地网页仍可能构造 POST。建议在端点上加最小来源校验或本地随机 token。
- `docs/plugin-tauri-boundary.md:138` P3：文档仍描述在 `tauri.conf.json` 配置 `dangerousRemoteDomainIpcAccess`，实际实现使用 capabilities remote 字段；两处描述应统一，避免后续维护者误改。

## 优先级清单

### P0（建议先修）

1. 设置 CSP；当前远端页面无 CSP。
2. 收紧导航/注入到“当前 dsh URL”，而不是任意 loopback。
3. 拆分或收窄 remote bridge 命令白名单。
4. 修复配置保存清空 `shortcuts` 的数据丢失问题。

### P1

1. 更新包增加校验和/签名验证。
2. 校验 `open_external` 目标。
3. 避免 shell PATH 读取执行登录交互脚本。
4. 统一 `get_ui_theme` 的 `DSH_HOME` 来源。
5. 修正快捷键 unregister/register 与全部移除的状态一致性。
6. 导入主题字段规范化与 override 白名单。
7. `llm.resolveModelInfo` 包装改为可清理。
8. projects 路由注册纳入 lifecycle。
9. lan-proxy 默认不允许无 token 监听外网。
10. 修复自动 bump 与 release workflow 并发。

### P2

其余文档漂移、body 上限、dev.mjs cwd、CI 触发路径、原子替换、窗口契约缺失等。

## 未验证项

- 未启动完整 Tauri 应用做人工 UI/进程联动测试。
- 未在 macOS/Windows/Linux 三平台真机验证桥接注入与 lan-proxy。
- 未审查 `node_modules` 与生成目录；只审查源码和配置。
