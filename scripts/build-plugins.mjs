#!/usr/bin/env node
import { spawnSync } from "node:child_process";
/**
 * build:plugins —— 编译 packages/plugins/* 全部插件，并把 dist 包装配进 Tauri resources。
 *
 * 对每个含 dsh 字段的插件子目录：
 *   ① spawnSync 跑 tsdown（corepack yarn workspace <pkg> build）
 *   ② 读源 package.json，用 buildDistManifest 变换成自包含 dist 清单
 *      （只保留 name/version/type/main/exports/dsh/dshDesktop/description，
 *       剔除 dependencies/devDependencies/peerDependencies/private/scripts/
 *       packageManager/types —— 内嵌产物不携带悬空声明）
 *   ③ 把 package.json、cordis.patch.yml、lib/（含 .map）拷入
 *      apps/shell/src-tauri/resources/plugins/<name>/（先清空旧目录）
 *   ④ 打印装配清单
 * 任一步失败 → 非零退出。
 *
 * 运行：
 *   node scripts/build-plugins.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const PLUGINS_DIR = path.join(ROOT, "packages", "plugins");
const RESOURCES_DIR = path.join(ROOT, "apps", "shell", "src-tauri", "resources", "plugins");

/** dist 清单白名单字段：其余（依赖/scripts/private/packageManager/types...）一律剔除。 */
const KEEP_FIELDS = new Set([
  "name",
  "version",
  "type",
  "main",
  "exports",
  "dsh",
  "dshDesktop",
  "description",
]);

/** 纯函数：源 package.json → 内嵌 dist 清单（剔除依赖与悬空 types，保留 lib 入口与 dsh 字段）。 */
export function buildDistManifest(src) {
  const dist = {};
  for (const [key, value] of Object.entries(src)) {
    if (KEEP_FIELDS.has(key)) dist[key] = value;
  }
  return dist;
}

function main() {
  if (!fs.existsSync(PLUGINS_DIR)) {
    console.error(`[build-plugins] 找不到插件目录: ${PLUGINS_DIR}`);
    process.exit(1);
  }

  const dirs = fs
    .readdirSync(PLUGINS_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  let failures = 0;
  for (const dirName of dirs) {
    const pluginDir = path.join(PLUGINS_DIR, dirName);
    const srcPkgPath = path.join(pluginDir, "package.json");
    if (!fs.existsSync(srcPkgPath)) continue;

    let srcPkg;
    try {
      srcPkg = JSON.parse(fs.readFileSync(srcPkgPath, "utf8"));
    } catch (error) {
      console.error(`[build-plugins] 读取 ${srcPkgPath} 失败: ${error.message}`);
      failures += 1;
      continue;
    }
    if (!srcPkg.dsh) continue; // 只装配含 dsh 字段的插件包

    // ① tsdown 构建（Task 1 产物：lib/）
    // win32 下 Node 无法直接 spawn .cmd/.bat（corepack.cmd），需经 shell 解析后才能启动
    const build = spawnSync("corepack", ["yarn", "workspace", srcPkg.name, "build"], {
      cwd: ROOT,
      stdio: "inherit",
      shell: process.platform === "win32",
    });
    if (build.error) {
      console.error(`[build-plugins] ${srcPkg.name} 启动构建失败: ${build.error.message}`);
      failures += 1;
      continue;
    }
    if (build.status !== 0) {
      console.error(`[build-plugins] ${srcPkg.name} 构建失败（exit ${build.status}）`);
      failures += 1;
      continue;
    }

    // ② dist 清单变换
    const dist = buildDistManifest(srcPkg);

    // ③ 装配到 resources/plugins/<name>/（先清空旧目录，避免残留陈旧产物）
    const target = path.join(RESOURCES_DIR, dirName);
    fs.rmSync(target, { recursive: true, force: true });
    fs.mkdirSync(target, { recursive: true });

    fs.writeFileSync(path.join(target, "package.json"), `${JSON.stringify(dist, null, 2)}\n`);

    const patchPath = path.join(pluginDir, "cordis.patch.yml");
    if (fs.existsSync(patchPath)) {
      fs.copyFileSync(patchPath, path.join(target, "cordis.patch.yml"));
    } else {
      console.warn(`[build-plugins] 警告: ${srcPkg.name} 缺少 cordis.patch.yml`);
    }

    const libDir = path.join(pluginDir, "lib");
    if (!fs.existsSync(libDir)) {
      console.error(`[build-plugins] ${srcPkg.name} 构建产物 lib/ 缺失（${libDir}），装配失败`);
      failures += 1;
      continue;
    }
    fs.cpSync(libDir, path.join(target, "lib"), { recursive: true });

    // ④ 打印清单
    const files = [];
    for (const entry of fs.readdirSync(target, { recursive: true })) {
      files.push(String(entry));
    }
    console.log(
      `[build-plugins] 已装配 ${srcPkg.name}@${dist.version} → resources/plugins/${dirName}/`,
    );
    for (const file of files) {
      console.log(`  - ${file}`);
    }
  }

  if (failures > 0) {
    console.error(`[build-plugins] ${failures} 个插件装配失败`);
    process.exit(1);
  }
}

// 直接执行时才跑装配；被测试 import 时仅暴露 buildDistManifest
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
