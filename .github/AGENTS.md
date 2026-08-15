# AGENTS.md - .github（CI/CD 与发布）

三平台构建 + 自动发布流水线，以及基于 Conventional Commits 的自动版本号管理。

## 文件

- `workflows/build.yml` - 工作流：`prepare` → `build`（平台矩阵）→ `release`
- `scripts/bump-version.mjs` - 依据提交信息自动 bump semver、更新 CHANGELOG、打 tag

## 工作流触发与流程

- 触发条件：
  - push 到 `release` 分支：`prepare` 依据提交信息 bump 版本、更新 `CHANGELOG.md`、本地打 `v<version>` tag，并用一次 atomic push 同时推送 `release` 分支与 tag
  - push `v*` tag：执行三平台构建与发布，避免分支 push 与 tag push 分裂成两条并发 release 流水线
  - PR 中改动 `apps/shell/**`、`packages/**`、`scripts/**`、`.github/**`、`package.json`、`yarn.lock`、`biome.json`、`lefthook.yml`、`.yarnrc.yml`：仅构建验证
  - `workflow_dispatch`：手动触发；在 `release` 分支上会构建并发布
- `prepare`：仅在 push 到 `release` 分支时运行 bump；tag 事件直接输出当前版本
- `build`：PR / tag / `workflow_dispatch` 触发
- `release`（仅 tag 或 `release` 分支手动触发）：下载各平台 artifact、按平台重命名、取 CHANGELOG 最新一节为发布说明，发布到现有 `v<version>` tag

## bump-version.mjs 行为

- 仅在 push 到 `release` 分支时真正 bump；提交信息以 `chore: bump version` 开头时跳过（防止递归触发）
- 版本级别：含 `BREAKING CHANGE` / `!` → major；`feat:` → minor；其余 → patch
- 脚本会以 github-actions[bot] 身份执行 `git commit` / `git tag` / `git push --atomic`，并更新 `package.json` 与 `CHANGELOG.md`
- 先创建本地 tag，再原子推送 `release` 分支与 tag；构建与发布由 tag push 事件驱动
- 本地手动运行会改写版本文件并推送 —— 调试前先想清楚影响面

## 约定与注意

- 提交信息必须遵循 [Conventional Commits](https://www.conventionalcommits.org/)，否则 CHANGELOG 与版本号无法正确生成
- 新增/修改平台矩阵时，需同步三平台的 bundle 配置、artifact 命名与 `release` job 中的重命名规则
- 勿手改 `CHANGELOG.md` 顶部（由脚本生成）；需要人工发布说明时，把它写进提交信息
