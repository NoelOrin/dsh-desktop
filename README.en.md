# DSH Desktop

> A cross-platform desktop shell for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)
> (`dsh`) built with Tauri 2: it automatically launches `dsh`, installs it with one click
> if missing, and opens the Harness Web UI directly in the system WebView once ready.

<p align="center">
  <a href="README.md">简体中文</a> · <b>English</b>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platform" />
  <img src="https://img.shields.io/badge/Tauri-2.11-4D6BFE" alt="Tauri 2.11" />
  <img src="https://img.shields.io/badge/Rust-1.97-orange" alt="Rust 1.97" />
  <img src="https://img.shields.io/badge/Yarn-4-blue" alt="Yarn 4" />
  <img src="https://img.shields.io/badge/release-v0.2.1-brightgreen" alt="Release" />
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License" />
</p>

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/NoelOrin/dsh-desktop/build.yml?branch=release&label=build" alt="Build" />
  <img src="https://img.shields.io/github/v/release/NoelOrin/dsh-desktop?sort=semver&label=release" alt="GitHub Release" />
</p>

## Features

- Auto-detects `dsh`: launches it directly if installed, or provides a one-click install and enters automatically
- Lets dsh pick a free loopback port (`dsh --profile <active> --host 127.0.0.1 --port 0`; the default profile is `web`)
- Single-window architecture: the main window hosts the splash page and the dsh Web UI; desktop shell capabilities (remote plugins / status / config / profile / interface mode / tools / autostart) are provided via the "Plugins", "Desktop", and "Appearance" pages of the WebUI settings panel — no separate control window
- Embedded dsh plugin auto-mount: `packages/plugins` (bridge / projects / shortcuts / reasoning) are bundled into Tauri resources with the app, copied into the dsh profile at startup, and mounted with a generated `--patch` overlay; external remote plugin presets are loaded by group from `packages/external-plugins` or other configured paths and installed with `dsh plugin add`
- Enhanced native capabilities: system tray (close to tray), native notifications, single-instance lock, crash auto-restart, system theme following, graceful exit with process-tree cleanup, file drop, window-state memory, `dsh-desktop://` deep links, auto-update (background check + "Check for updates" on the Desktop page), `update_dsh`, autostart and startup modes (`normal` / `tray` / `minimized`, dsh plugin settings panel, off by default), profile switching and rollback, remote plugin presets and managed plugin operations, custom global shortcuts, tray "Quit" confirmation; dsh web bridges the shell via `window.__DSH_DESKTOP__` (open external links / status and logs / config / profile / remote plugins / autostart / global shortcuts / updates)
- Profile environment: the app starts with the active profile and can switch profiles from the Desktop page, falling back to the last known good profile
- Interface mode: the Desktop page can switch between `compatibility` and `advanced` dsh desktop modes (Linux is fixed at `compatibility`; a restart is required)
- Theme and wallpaper: Settings → Appearance (7 built-in theme families with light/dark halves, custom themes, wallpaper blur/pixelation, glass opacity, typography), applied to the splash page and window background
- Third-party reasoning effort: the Models settings page lets custom / third-party models enable `low` / `medium` / `high` / `xhigh` / `max`, and the composer model menu switches the level directly
- Borderless window with a custom title bar: draggable, double-click to maximize, minimize/maximize/close controls, and theme-following title bar
- LAN access: a built-in zero-dependency reverse proxy lets LAN devices reach the local dsh web UI (unauthenticated; trusted networks only)

## Architecture

```mermaid
flowchart LR
  A[main window<br/>splash → dsh Web UI] --> B[Rust process manager]
  B -->|spawn / monitor / stop| C[dsh --profile <active> child process]
  C -->|HTTP 127.0.0.1:random port| A
  B -->|npm install -g when missing| D[install dsh globally]
  D --> C
  E[dsh plugin (bridge)<br/>WebUI settings "Plugins / Desktop / Appearance" pages] -->|window.__DSH_DESKTOP__ bridge| B
  B -->|auto-mount at startup| F[embedded dsh plugins<br/>resources/plugins → dsh profile]
  F -->|embedded-plugins.patch.yml overlay| C
  B -->|load by group at startup| G[external remote plugin presets<br/>external-plugins / config]
  G -->|dsh plugin add| C
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
corepack yarn install
```

> If Corepack is not enabled yet, run `corepack enable` first.

### Development

```sh
yarn dev
```

- `yarn dev`: full Tauri development mode (build plugins first, then start Vite and the plugin watcher)
- `yarn dev:web`: frontend-only hot reload (shell @ :5173)
- `yarn dev:shell`: Tauri `beforeDevCommand` prerequisite command (build plugins + Vite)
- `yarn dev:plugins`: watch plugin sources and hot-deploy to the dsh profile
- `yarn build:web`: build the frontend into `dist/`
- `yarn build:plugins`: compile dsh plugins into Tauri resources (stale outputs are pruned automatically)
- `yarn tauri`: pass through the Tauri CLI
- `yarn typecheck` / `yarn lint` / `yarn format:check`: typecheck and code-quality checks
- `yarn lan-proxy`: LAN reverse proxy (see below)

### Build

```sh
yarn build
```

`yarn build` builds the frontend and embedded dsh plugins first, then runs the Tauri bundle. Artifacts land in `apps/shell/src-tauri/target/release/bundle/`:

| Platform | Artifact |
| --- | --- |
| macOS | `.dmg` |
| Windows | `.exe` |
| Linux | `.AppImage` / `.deb` |

The macOS DMG uses a custom installer background and icon layout; sources are under `apps/shell/src-tauri/bundle/macos/`.

## One-Click dsh Install

When the splash page detects that `dsh` is not installed, clicking "Install DSH" runs a global install:

```sh
npm install -g @deepseek-ai/dsh
```

Installation logs stream live on the splash page; once finished, `dsh` is launched with the default `web` profile and the UI opens automatically.

## Embedded dsh Plugins

`packages/plugins` is the dsh plugin container in this repo. It currently includes:

- `bridge` (`@dsh-desktop/plugin-bridge`) — a two-sided plugin bridging Tauri shell capabilities: renders the "Plugins" page (external + local grouped remote presets / sync / installed plugins / remove / update), the "Desktop" page (status / config / profile / interface mode / tools / autostart) and the "Appearance" page (theme preference / theme library / background image / glass opacity / custom theme / typography) in the dsh WebUI settings panel, calling shell capabilities via `window.__DSH_DESKTOP__` (open external links / window controls / status and logs / config / profile / remote plugins / autostart / global shortcuts / updates)
- `projects` (`@dsh-desktop/plugin-projects`) — a host plugin providing dsh workspace/session data and action endpoints for the sidebar context menu
- `shortcuts` (`@dsh-desktop/plugin-shortcuts`) — a global shortcut settings plugin managing shell shortcuts from the dsh WebUI settings panel, plus double-Esc stop for the current conversation
- `reasoning` (`@dsh-desktop/plugin-reasoning`) — a Models settings plugin that writes `reasoningEfforts` for custom / third-party models inside the Models page so the composer model menu can switch levels
- `client-kit` (shared sources, not a plugin) — shared `window.__DSH_DESKTOP__` access, client CSS injection, and common tsdown build plugins; it is inlined into each plugin's client bundle and is not copied into `resources/plugins`

At build time `scripts/build-plugins.mjs` compiles each plugin with tsdown and copies a self-contained dist manifest into `apps/shell/src-tauri/resources/plugins/` for distribution with the installer; after a plugin is removed from source, the next build prunes its stale resources and `--home` profile output. At startup `embedded.rs` copies them into the dsh profile's node_modules and generates a `--patch` overlay so `dsh --profile <active>` mounts them automatically — best-effort, never blocking startup.

### Remote Plugin Presets

Remote plugin presets come from two sources: fixed external presets (for example `packages/external-plugins`, where each top-level `*.json` file is a group and external entries are read-only) and local grouped presets in `config.json.remote_plugins`. On startup, enabled presets are installed into the active profile with `dsh plugin add`; the bridge "Plugins" page supports group sync, installed-plugin lists, removal, and updates.

Presets may declare an `allow_build` array, forwarded as pnpm `--allow-build`, for GitHub/git plugins that require build scripts.
At build time `yarn build:plugins` copies `packages/external-plugins/*.json` into Tauri resources so default remote presets ship with the application.

> The responsibility boundary between the plugin domain and the Tauri shell domain, plus the controlled bridge and port-whitelist mechanism, is documented in [docs/plugin-tauri-boundary.md](docs/plugin-tauri-boundary.md).

## LAN Access (Optional)

`dsh` binds to `127.0.0.1` by default, so LAN devices cannot reach it. This repo ships a zero-dependency reverse proxy that impersonates the local loopback identity dsh expects (rewrites Host / Origin, passes through `Sec-Fetch-Site`, supports WebSocket and streaming responses):

The desktop shell exposes the proxy in the bridge plugin: Settings → Desktop → LAN access, where you can configure the bind address, port and upstream dsh address, then start or stop it. The token gate has been removed; use it only on trusted networks.

A standalone CLI entry point is also available:

```sh
yarn lan-proxy
```

Common flags (env vars `DSH_PROXY_*` take lower precedence than flags):

| Flag | Default | Description |
| --- | --- | --- |
| `--bind` | `0.0.0.0` | Listen address |
| `--port` | `8080` | Listen port |
| `--target` | `127.0.0.1:53553` | Upstream dsh web address |

## Configuration

Priority: `config.json` (app data dir; includes `remote_plugins_path` and `remote_plugins`) → environment variables → PATH detection.
PATH detection merges the GUI process PATH, macOS system PATH (`/etc/paths` + `/etc/paths.d`), Linux `/etc/environment`, Windows user/system registry PATH, and non-Windows user shell PATH; the merged PATH is also passed to dsh/npm child processes.
The Desktop page "Config" section reads/writes `config.json` via `get_config` / `set_config`; fields not set in `config.json` fall back to environment variables.

| Environment variable | Default | Description |
| --- | --- | --- |
| `DSH_BIN` | `dsh` on PATH | dsh entry point |
| `DSH_NODE` | `node` on PATH | Node interpreter |
| `DSH_HOME` | inherited from environment | Harness data directory passed to dsh |
| `DSH_DESKTOP_REMOTE_PLUGINS_PATH` | auto-detected `$DSH_HOME/remote-plugins(.json)`, app data, bundled `resources/external-plugins`, and repo `packages/external-plugins` | External remote plugin preset file or directory |

## Directory Structure

```text
dsh-desktop/
├── apps/
│   └── shell/                    # main window
│       ├── index.html            # splash page entry
│       ├── src/                  # splash frontend (Vite + TypeScript)
│       └── src-tauri/            # Tauri 2 + Rust backend
│           ├── bundle/macos/     # macOS DMG background sources and rendered assets
│           ├── capabilities/     # IPC permissions (default.json / bridge.json / shortcuts.json)
│           ├── permissions/      # autogenerated command permissions
│           ├── resources/plugins # embedded dsh plugins bundled with the app (build output)
│           ├── icons/            # app icons
│           ├── tauri.conf.json   # main Tauri configuration
│           ├── tauri.macos.conf.json
│           └── src/              # Rust backend (lib.rs / config.rs / profiles.rs / plugin_ops.rs / mode.rs / projects.rs / embedded.rs)
├── packages/
│   ├── contracts/                # shared IPC types & constants (@dsh-desktop/contracts)
│   ├── external-plugins/         # external remote plugin presets (one group per top-level JSON file)
│   └── plugins/                  # dsh plugin container: bridge / projects / shortcuts / reasoning / client-kit
├── scripts/
│   ├── dev.mjs                   # dev prerequisite: build plugins + Vite
│   ├── watch-plugins.mjs         # plugin source watcher and hot deployment
│   ├── build-plugins.mjs         # compile plugins into Tauri resources
│   ├── lan-proxy.mjs             # LAN reverse proxy
│   └── *.test.mjs                # script tests
├── .github/
│   ├── scripts/                  # versioning / updater artifact scripts
│   └── workflows/                # three-platform CI build & independent release
├── docs/                         # plugin boundary docs, screenshots, etc.
├── dist/                         # frontend build output (generated, do not edit)
├── CHANGELOG.md
└── package.json
```

## Notes

- Artifacts are unsigned: on macOS right-click → "Open" the first time; on Windows SmartScreen will prompt
- Linux builds depend on WebKitGTK and other system packages; GitHub Actions installs them
- The frontend cannot fully run in a regular browser (it relies on `window.__TAURI_INTERNALS__` injected by Tauri); only styles/layout can be debugged there
- `dist/`, `apps/shell/src-tauri/target/`, and `apps/shell/src-tauri/resources/plugins/` are generated output and should not be edited

## Contributing

1. Fork the repo and create a feature branch
2. Follow [Conventional Commits](https://www.conventionalcommits.org/) for commit messages
3. Push the branch and open a Pull Request
4. Merge after the three-platform build passes

## License

[MIT](LICENSE)
