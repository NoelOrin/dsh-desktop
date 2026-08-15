# AGENTS.md - docs

文档资源目录，包含插件边界、dsh 资料、设计/实施记录与截图：

- `plugin-tauri-boundary.md` - dsh 插件域与 Tauri 壳域的职责边界、IPC 契约、受控桥接与内嵌插件装配
- `dsh/config-catalog.zh.md` - dsh 插件配置目录（中文）归档
- `dsh/README.md` - dsh 资料目录索引
- `screenshots/splash-desktop.png` - 桌面端启动页截图
- `screenshots/splash-mobile.png` - 窄屏启动页截图
- `superpowers/plans/*.md` - 原生能力扩展与主题/背景图等历史实施计划
- `superpowers/specs/*.md` - Monorepo 改造、桌面壳设置与主题/背景图等历史设计记录

## 维护约定

- 修改启动页 UI 后如需更新截图，重新生成并替换 `screenshots/` 下文件；README 未直接引用这些截图，仅为存档用途
- 归档 dsh 官方文档时放入 `dsh/`，并在 `dsh/README.md` 记录来源与更新日期
- 新增插件边界、IPC 契约或内嵌装配规则时，先更新 `plugin-tauri-boundary.md`，再同步根与子模块 AGENTS
- 历史 `superpowers/` 计划/规格只记录当时设计，不视为当前实现；以代码与最新 AGENTS 为准
