#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const BUILD_SCRIPT = path.join(ROOT, "scripts", "build-plugins.mjs");
const WATCH_SCRIPT = path.join(ROOT, "scripts", "watch-plugins.mjs");
const shell = process.platform === "win32";

const initial = spawnSync(process.execPath, [BUILD_SCRIPT], {
  cwd: ROOT,
  stdio: "inherit",
  shell,
});
if (initial.error) {
  console.error(`[dev] 插件初始构建失败: ${initial.error.message}`);
  process.exit(1);
}
if (initial.status !== 0) process.exit(initial.status ?? 1);

const watch = spawn(process.execPath, [WATCH_SCRIPT, "--no-initial"], {
  cwd: ROOT,
  stdio: "inherit",
  shell,
});
const vite = spawn("corepack", ["yarn", "web:all"], {
  cwd: ROOT,
  stdio: "inherit",
  shell,
});

let stopping = false;
function stop() {
  if (stopping) return;
  stopping = true;
  watch.kill();
  vite.kill();
  setTimeout(() => process.exit(0), 1000).unref();
}

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => stop());
}

watch.on("error", (error) => {
  console.error(`[dev] 插件 watch 启动失败: ${error.message}`);
  vite.kill();
  process.exit(1);
});
vite.on("error", (error) => {
  console.error(`[dev] Vite 启动失败: ${error.message}`);
  watch.kill();
  process.exit(1);
});

vite.on("exit", (code, signal) => {
  stop();
  process.exit(code ?? (signal ? 1 : 0));
});
watch.on("exit", (code, signal) => {
  if (stopping) return;
  vite.kill();
  process.exit(code ?? (signal ? 1 : 0));
});
