# external-plugins

外部远程插件预设目录。应用初始化时检测该目录（或显式配置的 `remote_plugins_path` / `DSH_DESKTOP_REMOTE_PLUGINS_PATH`），并把其中的预设合并到远程插件管理中。

## 约定

- 每个顶层 `*.json` 文件代表一个 **group**，文件名（不含 `.json`）就是分组名。
- `remote-plugins.json` 特殊处理为 `default` 分组。
- 文件内容支持三种形式：
  - 数组：`[{ "url": "https://example.com/plugin.tgz", "enabled": true }]`
  - 对象：`{ "presets": [{ "url": "...", "enabled": true }] }`
  - 单个预设：`{ "url": "...", "enabled": true }`
- 可选字段：`id`、`group`、`enabled`。`group` 省略时使用文件名；`enabled` 省略时默认 `true`。
- 可选字段 `allow_build`：pnpm 构建脚本白名单包名数组；仅显式声明时透传给 `dsh plugin add --allow-build <package>`，用于 GitHub/git 插件等需要 `prepare` 或原生构建的场景。
- 目录中的预设是固定外部配置，bridge 插件管理页会以只读方式展示，不写入 `config.json`。
- 未显式配置时，应用会依次检测 `$DSH_HOME/remote-plugins(.json)`、应用数据目录与本仓库 `packages/external-plugins`。
- 发布模式在应用数据目录之后检测随包 `resources/external-plugins`，开发模式回退到本仓库目录。
- `yarn build:plugins` 会把本目录顶层 `*.json` 装配到 `apps/shell/src-tauri/resources/external-plugins/`，随 Tauri 发布包分发；发布模式优先从随包资源加载，开发模式仍读本目录。
- 启动时会对 active profile 自动执行 `dsh plugin --profile <active> add <url>`；失败只记日志，不阻塞 dsh 启动。

## 示例

`packages/external-plugins/tools.json`：

```json
[
  {
    "url": "https://github.com/example/dsh-tool-plugin",
    "enabled": true
  },
  {
    "url": "https://registry.npmjs.org/@example/dsh-plugin/-/dsh-plugin-1.0.0.tgz",
    "enabled": false
  }
]
```
