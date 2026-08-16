# AGENTS.md - .github（CI/CD 与发布）

三平台 CI 构建 + 独立 release 发布，以及基于 Conventional Commits 的自动版本号管理。

## 文件

- `workflows/build.yml` - CI workflow：`prepare` → `build`（平台矩阵）
- `workflows/release.yml` - release workflow：仅在 GitHub Release 发布（`published`）时触发，复用 CI 构建产物
- `scripts/bump-version.mjs` - 依据提交信息自动 bump semver、更新 CHANGELOG、创建并推送 tag

## 工作流触发与流程

- `build.yml`：
  - push 到 `release` 分支：`prepare` 依据提交信息 bump 版本、更新 `CHANGELOG.md`、创建 `v<version>` tag 并推送；`prepare` 报告 `bumped=false` 时（例如自动 bump 提交）才执行三平台构建
  - PR 中改动 `apps/shell/**`、`packages/**`、`scripts/**`、`.github/**`、`package.json`、`yarn.lock`、`biome.json`、`lefthook.yml`、`.yarnrc.yml`：执行三平台构建验证
  - `workflow_dispatch`：手动触发构建
- `release.yml`：
  - 仅 `release` 事件 `published` 触发；tag push 不再自动发布
  - checkout release tag，按 tag commit 查找 `build.yml` 中成功的 push 到 `release` CI run，下载 `DSH-Desktop-*` artifacts，按平台重命名后上传到现有 GitHub Release
  - 若 CI 尚未完成，最多等待 30 分钟；CI 失败或 artifacts 已过期时 workflow 失败

## bump-version.mjs 行为

- 仅在 push 到 `release` 分支时真正 bump；提交信息以 `chore: bump version` 开头时跳过（防止递归触发）
- 版本级别：含 `BREAKING CHANGE` / `!` → major；`feat:` → minor；其余 → patch
- 脚本会以 github-actions[bot] 身份执行 `git commit` / `git tag` / `git push --atomic`，并更新 `package.json` 与 `CHANGELOG.md`
- 生成 CHANGELOG 条目时，若在 GitHub Actions 且提供 `GH_TOKEN`，脚本会查询 commit 关联的 PR，并在条目后追加 `(#N)`；已有 PR 编号或查不到时保持原样
- 先创建本地 tag，再原子推送 `release` 分支与 tag；tag 只供后续 GitHub Release 使用，不再驱动 workflow
- 本地手动运行会改写版本文件并推送 —— 调试前先想清楚影响面

## 约定与注意

- 提交信息必须遵循 [Conventional Commits](https://www.conventionalcommits.org/)，否则 CHANGELOG 与版本号无法正确生成
- CHANGELOG 条目应写清 PR 编号/链接与本次更新内容；`Unreleased` 区也按此维护
- 新增/修改平台矩阵时，需同步三平台的 bundle 配置、artifact 命名与 `release.yml` 中的重命名规则
- release 必须等对应 tag commit 的 CI 成功且 artifacts 未过期后再发布；CI artifacts 保留 90 天
- 已发布版本区块由脚本生成，勿手改；未发布更新应维护在 `## [Unreleased]`，用 `-` 开头并写清 PR 编号/链接
