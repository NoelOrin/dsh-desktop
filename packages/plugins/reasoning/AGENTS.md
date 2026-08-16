# AGENTS.md - packages/plugins/reasoning（模型设置与第三方思考强度插件）

`@dsh-desktop/plugin-reasoning`：内嵌替换官方 `ui-settings-models` 客户端，在
dsh WebUI 的“模型”设置页内管理自定义 / 第三方 pi-ai 供应商，并为模型行提供
`low / medium / high / xhigh / max` 五档思考强度勾选。

## 职责

- 通过 overlay 停用官方 `ui-settings-models`，再以 `@dsh-desktop/plugin-reasoning`
  注册同一个 `settings.section`（id `models`，order 10）和官方 onboarding 槽位
- 源码复刻自参考仓库 `Deepseek-Harness-Desktop` 的 `ui-settings-models` client 包，
  保存在 `src/models/`；保留 API 密钥、自定义提供方、模型目录、模型询问等官方能力
- 在 pi-ai `ModelListEditor` 的模型行中加入思考强度勾选，按模型生成
  `reasoningEfforts: { low: "low", medium: "medium", ... }`
  （显示名与 canonical id 一致：low / medium / high / xhigh / max）
- 通过 `settings.mutate` 的 `["providers", route, "models"]` path 写入，
  携带当前 namespace revision，避免覆盖并发修改；全部取消时写
  `reasoningEfforts: { off: null }`，明确关闭推理而不是继承 catalog 能力
- 输入栏推理等级由上游 `dsh-client-ui-model-selection` 提供；本插件只保证
  `reasoningEfforts` 持久化后上游能读到对应能力，并在 host 面包装
  `ctx.llm.resolveModelInfo`，把显示名统一为 low / medium / high / xhigh / max

## 实现约定

- `cordis.patch.yml` 必须同时停用官方 `ui-settings-models` 并插入本插件，避免两个
  “模型”设置节重复注册
- `package.json` 的 `dshDesktop.overlay` 必须为 `true`，否则桌面壳只生成普通
  `insert`，官方模型页仍会加载并导致 `settings.models` 中文字典重复注册
- 不注册新的 settings namespace，只读写上游 `llm-pi-ai`
- host 面保存原始 `resolveModelInfo`，在 `sctx.effect` disposer 中仅在仍指向本包装时恢复，避免 HMR 重复叠加
- 上游升级时优先同步 `src/models/` 到新版本再重放本仓改动（集中在
  `ModelListEditor.tsx` / `locales.ts` / `ProviderEditor.tsx`）
- 修改后运行 `yarn typecheck`、`yarn lint`、`yarn build:plugins`
