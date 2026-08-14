# DSH Desktop

> 用 Tauri 2 为 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)
> （`dsh`）打造的跨平台桌面套壳：自动拉起 `dsh web`，未安装时一键安装，
> 就绪后直接在系统 WebView 中进入 Harness Web UI。

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platform" />
  <img src="https://img.shields.io/badge/Tauri-2.11-4D6BFE" alt="Tauri 2.11" />
  <img src="https://img.shields.io/badge/Rust-1.97-orange" alt="Rust 1.97" />
  <img src="https://img.shields.io/badge/Yarn-4-blue" alt="Yarn 4" />
  <img src="https://img.shields.io/badge/release-v0.1.0-brightgreen" alt="Release" />
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License" />
</p>

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/NoelOrin/dsh-desktop/build.yml?branch=release&label=build" alt="Build" />
  <img src="https://img.shields.io/github/v/release/NoelOrin/dsh-desktop?sort=semver&label=release" alt="GitHub Release" />
</p>

## 特性

- 自动检测 `dsh`：已安装直接启动，未安装提供一键安装并自动进入
- 自动选择空闲 loopback 端口，以 `dsh web --host 127.0.0.1 --port <port>` 启动

## 架构

```mermaid
flowchart LR
  A[DSH Desktop 窗口] --> B[Rust 进程管理器]
  B -->|spawn / 监控 / 停止| C[dsh web 子进程]
  C -->|HTTP 127.0.0.1:随机端口| A
  B -->|未安装时 npm install| D[应用数据目录 runtime]
  D --> C
```

## 快速开始

### 前置依赖

| 依赖 | 最低版本 | 用途 |
| --- | --- | --- |
| Node.js | 22 | 前端构建与 dsh 运行环境 |
| Rust | 1.77 | Tauri 后端编译 |
| Yarn | 4 | 依赖管理 |
| `dsh` | 任意 | DeepSeek Harness CLI（可自动安装） |

### 安装

```sh
git clone <your-repo-url> dsh-desktop
cd dsh-desktop
yarn
```

### 开发运行

```sh
yarn tauri dev
```

### 打包

```sh
yarn build
```

构建产物位于 `src-tauri/target/release/bundle/`：

| 平台 | 产物 |
| --- | --- |
| macOS | `.dmg` |
| Windows | `.exe` |
| Linux | `.AppImage` / `.deb` |

## 一键安装 dsh

启动页检测到系统没有 `dsh` 时，点击“一键安装 DSH”，应用会在数据目录执行：

```sh
npm install --prefix <应用数据目录>/runtime @deepseek-ai/dsh
```

安装日志实时展示在启动页，完成后自动拉起 `dsh web` 并进入界面。

## 配置

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DSH_BIN` | PATH 中的 `dsh` | 指定 dsh 入口 |
| `DSH_NODE` | PATH 中的 `node` | 指定 Node 解释器 |
| `DSH_HOME` | 继承当前环境 | 传给 dsh 的 Harness 数据目录 |

## 目录结构

```text
dsh-desktop/
├── src/                     # 启动页前端（Vite）
├── src-tauri/               # Tauri 2 + Rust 后端
│   ├── capabilities/        # IPC 权限
│   ├── icons/               # 应用图标
│   └── src/lib.rs           # dsh 检测、安装、进程管理、窗口导航
├── .github/
│   ├── scripts/             # 版本管理脚本
│   └── workflows/           # 三平台构建与自动发布
├── CHANGELOG.md
└── package.json
```

## 注意事项

- 构建产物未签名，macOS 首次打开需右键“打开”，Windows 会提示 SmartScreen
- Linux 构建依赖 WebKitGTK 等系统包，GitHub Actions 已内置安装步骤
## 贡献

1. Fork 本仓库并创建功能分支
2. 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)
3. 推送分支并创建 Pull Request
4. 通过三平台构建后合并

## License

[MIT](LICENSE)
