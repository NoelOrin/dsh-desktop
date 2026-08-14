# DSH Desktop

> A cross-platform desktop shell for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)
> (`dsh`) built with Tauri 2: it automatically launches `dsh web`, installs it with one click
> if missing, and opens the Harness Web UI directly in the system WebView once ready.

<p align="center">
  <a href="README.md">简体中文</a> · <b>English</b>
</p>

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

## Features

- Auto-detects `dsh`: launches it directly if installed, or provides a one-click install and enters automatically
- Picks a free loopback port and starts `dsh web --host 127.0.0.1 --port <port>`
- Single-window architecture: the main window hosts the splash page and the dsh Web UI; desktop shell capabilities (status / config / tools / autostart) are provided via a dsh plugin on the "Desktop" page of the WebUI settings panel — no separate control window
- Embedded dsh plugin auto-mount: `packages/plugins` (bridge / hello) are bundled into Tauri resources with the app, then copied into the dsh profile at startup with a generated `--patch` overlay
- Enhanced native capabilities: system tray (close to tray), native notifications, single-instance lock, crash auto-restart, system theme following, graceful exit with process-tree cleanup, file drop, window-state memory, `dsh-desktop://` deep links, auto-update (background check + "Check for updates" on the Desktop page), autostart (dsh plugin settings panel, off by default), custom global shortcuts, tray "Quit" confirmation; dsh web bridges the shell via `window.__DSH_DESKTOP__` (notifications / clipboard / dialogs / open external links / autostart / global shortcuts / deep links)
- LAN access: a built-in zero-dependency reverse proxy lets LAN devices reach the local `dsh web` (with optional Bearer token gate)

## Architecture

```mermaid
flowchart LR
  A[main window<br/>splash → dsh Web UI] --> B[Rust process manager]
  B -->|spawn / monitor / stop| C[dsh web child process]
  C -->|HTTP 127.0.0.1:random port| A
  B -->|npm install -g when missing| D[install dsh globally]
  D --> C
  E[dsh plugin (bridge)<br/>WebUI settings "Desktop" page] -->|window.__DSH_DESKTOP__ bridge| B
  B -->|auto-mount at startup| F[embedded dsh plugins<br/>resources/plugins → dsh profile]
  F -->|cordis patch overlay| C
```

## Quick Start

### Prerequisites

| Dependency | Minimum | Purpose |
| --- | --- | --- |
| Node.js | 22 | Frontend build and dsh runtime |
| Rust | 1.77 | Tauri backend compilation |
| Yarn | 4 | Package management |
| `dsh` | any | DeepSeek Harness CLI (auto-installable) |

### Install

```sh
git clone https://github.com/NoelOrin/dsh-desktop.git
cd dsh-desktop
yarn
```

### Development

```sh
yarn dev
```

Frontend-only hot reload: `yarn dev:web` (shell @ :5173).
Typecheck all workspaces: `yarn typecheck`.
Compile dsh plugins into Tauri resources: `yarn build:plugins`.
LAN reverse proxy: `yarn lan-proxy` (see "LAN Access" below).

### Build

```sh
yarn build
```

Artifacts land in `apps/shell/src-tauri/target/release/bundle/`:

| Platform | Artifact |
| --- | --- |
| macOS | `.dmg` |
| Windows | `.exe` |
| Linux | `.AppImage` / `.deb` |

## One-Click dsh Install

When the splash page detects that `dsh` is not installed, clicking "Install DSH" runs a global install:

```sh
npm install -g @deepseek-ai/dsh
```

Installation logs stream live on the splash page; once finished, `dsh web` is launched and the UI opens automatically.

## Embedded dsh Plugins

`packages/plugins` is the dsh plugin container in this repo. It currently includes:

- `bridge` (`@dsh-desktop/plugin-bridge`) — a two-sided plugin bridging Tauri shell capabilities: renders the "Desktop" page and the "autostart" toggle in the dsh WebUI settings panel, calling shell capabilities via `window.__DSH_DESKTOP__` (notifications / clipboard / dialogs / open external links / autostart / global shortcuts / deep links)
- `hello` (`@dsh-desktop/plugin-hello`) — a sample custom plugin (skeleton) demonstrating the development structure

At build time `scripts/build-plugins.mjs` compiles each plugin with tsdown and copies a self-contained dist manifest into `apps/shell/src-tauri/resources/plugins/` for distribution with the installer; at startup `embedded.rs` copies them into the dsh profile's node_modules and generates a `--patch` overlay so `dsh web` mounts them automatically — best-effort, never blocking startup.

> The responsibility boundary between the plugin domain and the Tauri shell domain, plus the controlled bridge and port-whitelist mechanism, is documented in [docs/plugin-tauri-boundary.md](docs/plugin-tauri-boundary.md).

## LAN Access (Optional)

`dsh web` binds to `127.0.0.1` by default, so LAN devices cannot reach it. This repo ships a zero-dependency reverse proxy that impersonates the local loopback identity dsh expects (rewrites Host / Origin, passes through `Sec-Fetch-Site`, supports WebSocket and streaming responses):

```sh
yarn lan-proxy --token "a-long-enough-random-string"
```

Common flags (env vars `DSH_PROXY_*` take lower precedence than flags):

| Flag | Default | Description |
| --- | --- | --- |
| `--bind` | `0.0.0.0` | Listen address |
| `--port` | `8080` | Listen port |
| `--target` | `127.0.0.1:53553` | Upstream dsh web address |
| `--token` | off | Enable Bearer token gate (strongly recommended) |

## Configuration

Priority: `config.json` (app data dir) → environment variables → PATH detection.
The Desktop page "Config" section reads/writes `config.json` via `get_config` / `set_config`; fields not set in `config.json` fall back to environment variables.

| Environment variable | Default | Description |
| --- | --- | --- |
| `DSH_BIN` | `dsh` on PATH | dsh entry point |
| `DSH_NODE` | `node` on PATH | Node interpreter |
| `DSH_HOME` | inherited from environment | Harness data directory passed to dsh |

## Directory Structure

```text
dsh-desktop/
├── apps/
│   └── shell/                    # main window
│       ├── index.html            # splash page entry
│       ├── src/                  # splash frontend (Vite + TypeScript)
│       └── src-tauri/            # Tauri 2 + Rust backend
│           ├── capabilities/     # IPC permissions (default.json / bridge.json)
│           ├── permissions/      # autogenerated command permissions
│           ├── resources/plugins # embedded dsh plugins bundled with the app (build output)
│           ├── icons/            # app icons
│           └── src/              # lib.rs / config.rs / embedded.rs / main.rs
├── packages/
│   ├── contracts/                # shared IPC types & constants (@dsh-desktop/contracts)
│   └── plugins/                  # dsh plugin container: bridge / hello
├── scripts/
│   ├── lan-proxy.mjs             # LAN reverse proxy
│   └── build-plugins.mjs         # compile plugins into Tauri resources
├── .github/
│   ├── scripts/                  # versioning / updater artifact scripts
│   └── workflows/                # three-platform build & auto release
├── docs/                         # plugin boundary docs, screenshots, etc.
├── dist/                         # frontend build output (generated, do not edit)
├── CHANGELOG.md
└── package.json
```

## Notes

- Artifacts are unsigned: on macOS right-click → "Open" the first time; on Windows SmartScreen will prompt
- Linux builds depend on WebKitGTK and other system packages; GitHub Actions installs them
- The frontend cannot fully run in a regular browser (it relies on `window.__TAURI_INTERNALS__` injected by Tauri); only styles/layout can be debugged there

## Contributing

1. Fork the repo and create a feature branch
2. Follow [Conventional Commits](https://www.conventionalcommits.org/) for commit messages
3. Push the branch and open a Pull Request
4. Merge after the three-platform build passes

## License

[MIT](LICENSE)
