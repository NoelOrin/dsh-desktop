# Changelog

## [0.3.1] - 2026-08-16

### Bug Fixes

- fix(ci): bump 后显式 dispatch CI 构建 (#4)

### Other

- docs(changelog): 关联 PR #4 修复说明 (#4)

### Pending Updates

- [PR #4](https://github.com/NoelOrin/dsh-desktop/pull/4)：修复 bump 后未自动触发 CI 构建的问题

## [0.3.0] - 2026-08-16

### Features

- feat(shell): 完善插件能力并拆分 CI/release 发布 (#3)
- feat(shell): register remote plugin commands (#3)
- feat(bridge): add settings navigation icon markers (#3)
- feat(shell): add desktop mode settings and advanced shell (#3)
- feat(bridge): add plugin management panel and operations (#3)
- feat(shell): add managed plugin operation runner (#3)
- feat(bridge): add profile environment UI (#3)
- feat(shell): launch dsh web with active profile (#3)
- feat(shell): add profile discovery and state machine (#3)
- feat(security): harden dsh web bridge injection with host token (#3)
- feat(contracts): add profile and plugin operation contract (#3)

### Bug Fixes

- fix(bridge): add plugin management icon declaration (#3)
- fix(shell): add missing plugin capability models (#3)
- fix(shell): harden plugin operation cancellation and validation (#3)
- fix(shell): wait for plugin operation cleanup after cancel (#3)
- fix(shell): keep plugin operation slot until cleanup finishes (#3)
- fix(shell): harden plugin operation cancellation and logging (#3)
- fix(bridge): confirm profile switch and fallback profile home (#3)
- fix(shell): align profile launch cleanup and healthy commit (#3)

### Other

- docs(changelog): 关联 PR #3 更新说明 (#3)
- docs(bridge): clarify advanced mode uses upstream layout (#3)
- style(bridge): tidy desktop settings spacing (#3)
- style(bridge): polish plugin panel and appearance UI (#3)
- docs(plugins): add remote preset and capability refactor plan (#3)
- chore(shell): add generated plugin command permissions (#3)
- style(shortcuts): complete settings layout and preset icons (#3)
- chore(deps): update lockfile for shortcuts settings dependencies (#3)
- style(shell): format and harden mode settings injection (#3)
- refactor(plugins): persist shortcuts and harden projects endpoints (#3)

### Pending Updates

- [PR #3](https://github.com/NoelOrin/dsh-desktop/pull/3)：拆分 CI 构建与 GitHub Release 发布为两个独立 workflow
- 更新内容：release workflow 仅在 Release 发布时触发，并复用 build.yml 上传的三平台 CI artifacts

## [0.2.1] - 2026-08-15

### Bug Fixes

- fix: 修复 Windows 编译错误 (#2)

## [0.2.0] - 2026-08-15

### Features

- feat: 桌面壳原生能力与 dsh 插件体系完整落地 (#1)

## [0.1.0] - 2026-08-14

### Features

- feat: add Tauri desktop shell for DeepSeek Harness
