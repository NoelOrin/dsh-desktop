#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const PLUGINS_DIR = path.join(ROOT, "packages", "plugins");
const BUILD_SCRIPT = path.join(ROOT, "scripts", "build-plugins.mjs");
const POLL_INTERVAL_MS = 400;
const FAIL_RETRY_MS = 1500;
const EXCLUDED_DIRS = new Set(["lib", "node_modules", ".git"]);

function resolveHome() {
  const envHome = process.env.DSH_HOME;
  return envHome ? path.resolve(envHome) : path.join(os.homedir(), ".dsh");
}

function listPlugins() {
  const dirs = fs
    .readdirSync(PLUGINS_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  const plugins = [];
  for (const dirName of dirs) {
    const dir = path.join(PLUGINS_DIR, dirName);
    const pkgPath = path.join(dir, "package.json");
    if (!fs.existsSync(pkgPath)) continue;
    let pkg;
    try {
      pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    } catch {
      continue;
    }
    if (!pkg.dsh || typeof pkg.name !== "string") continue;
    plugins.push({ name: pkg.name, dir });
  }
  return plugins;
}

function walkFiles(dir, base = dir, result = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const absolute = path.join(dir, entry.name);
    if (entry.name.startsWith(".")) continue;
    if (entry.isDirectory()) {
      if (EXCLUDED_DIRS.has(entry.name)) continue;
      walkFiles(absolute, base, result);
    } else if (entry.isFile()) {
      const relative = path.relative(base, absolute);
      if (/\.(tmp|backup)$/.test(relative)) continue;
      try {
        const stat = fs.statSync(absolute);
        result.push([relative, stat.mtimeMs, stat.size]);
      } catch {
        // 文件可能在扫描瞬间被编辑器删除，跳过即可
      }
    }
  }
  return result;
}

function snapshot() {
  const state = new Map();
  // 监听整个插件容器：client-kit/ 与 tsdown 配置变化也应触发重建。
  state.set("plugins", walkFiles(PLUGINS_DIR));
  return state;
}

function hasChanged(prev, next) {
  if (prev.size !== next.size) return true;
  for (const [name, files] of next) {
    const old = prev.get(name);
    if (!old || old.length !== files.length) return true;
    for (let index = 0; index < files.length; index += 1) {
      const before = old[index];
      const after = files[index];
      if (before[0] !== after[0] || before[1] !== after[1] || before[2] !== after[2]) {
        return true;
      }
    }
  }
  return false;
}

function runBuild(home) {
  const result = spawnSync(process.execPath, [BUILD_SCRIPT, "--home", home], {
    cwd: ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.error) {
    console.error(`[watch-plugins] 启动插件构建失败: ${result.error.message}`);
    return false;
  }
  if (result.status !== 0) {
    console.error(`[watch-plugins] 插件构建失败（exit ${result.status}）`);
    return false;
  }
  return true;
}

function main() {
  const noInitial = process.argv.includes("--no-initial");
  const home = resolveHome();
  const plugins = listPlugins();
  if (plugins.length === 0) {
    console.error("[watch-plugins] 未找到任何 dsh 插件");
    process.exit(1);
  }

  console.log(`[watch-plugins] 监视 ${plugins.length} 个插件，热部署目录: ${home}`);
  if (!noInitial) {
    if (!runBuild(home)) process.exit(1);
  }

  let last = snapshot();
  let pending = false;
  let building = false;
  let retryAt = 0;

  setInterval(() => {
    if (building) return;
    const next = snapshot();
    if (hasChanged(last, next)) pending = true;
    if (!pending || Date.now() < retryAt) return;

    building = true;
    const ok = runBuild(home);
    building = false;
    if (ok) {
      pending = false;
      last = snapshot();
      console.log("[watch-plugins] 插件已重建并热部署，等待 dsh-client-hmr 热替换");
    } else {
      retryAt = Date.now() + FAIL_RETRY_MS;
    }
  }, POLL_INTERVAL_MS);

  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => process.exit(0));
  }
}

main();
