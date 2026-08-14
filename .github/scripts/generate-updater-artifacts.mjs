#!/usr/bin/env node
// 生成 Tauri updater 的 latest.json 与各平台安装包的 .sig 签名。
// 用法: node .github/scripts/generate-updater-artifacts.mjs <VERSION> [BUNDLE_DIR]
// 读取 TAURI_SIGNING_PRIVATE_KEY（私钥 PEM，配置为仓库 secret）与
// TAURI_SIGNING_PRIVATE_KEY_PASSWORD（可选，私钥口令）。
// - 缺密钥时：跳过签名并从未签名平台中剔除（不写进 latest.json），打印警告，不阻塞发布；
//   此时 latest.json 不含任何平台——发布继续但客户端暂时收不到更新，等配置密钥后重新发布。
// - 已配密钥但 signer 失败：exit(1) 使发布失败（避免静默发布无法校验的更新）。
// - 可选 RELEASE_NOTES 环境变量：写入 latest.json 的 notes 字段（默认 "see CHANGELOG"）。

import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const VERSION = process.argv[2];
if (!VERSION) {
  console.error("缺少版本号参数: node .github/scripts/generate-updater-artifacts.mjs <VERSION> [BUNDLE_DIR]");
  process.exit(1);
}

// 可选 bundle 目录：CI 中产物经 upload/download-artifact 保留 workspace 相对路径，
// 落在 apps/shell/src-tauri/target/release/bundle/ 下，必须显式传入该目录（默认当前目录）。
const BUNDLE_DIR = process.argv[3] ?? process.cwd();

// GitHub Actions 自动注入的仓库标识（owner/repo），用于拼 release asset URL
const REPO = process.env.GITHUB_REPOSITORY;
if (!REPO) {
  console.error("缺少 GITHUB_REPOSITORY 环境变量（GitHub Actions 注入）");
  process.exit(1);
}

const BUNDLE_EXTS = [".dmg", ".exe", ".AppImage", ".deb"];
const PLATFORM_MAP = {
  ".dmg": "darwin-aarch64",
  ".exe": "windows-x86_64",
  ".AppImage": "linux-x86_64",
};

function collectBundles(dir) {
  const found = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    let stat;
    try {
      stat = statSync(full);
    } catch {
      continue;
    }
    if (stat.isDirectory()) {
      if (entry === "node_modules" || entry === ".git") continue;
      found.push(...collectBundles(full));
    } else if (BUNDLE_EXTS.some((ext) => full.endsWith(ext))) {
      found.push(full);
    }
  }
  return found;
}

function resolveSignerCommand() {
  // 优先用仓库已装的 CLI，否则退回 npx（release job 未执行 yarn install）
  const local = join(process.cwd(), "node_modules", ".bin", "tauri");
  try {
    statSync(local);
    return { cmd: local, args: [] };
  } catch {
    return { cmd: "npx", args: ["--yes", "@tauri-apps/cli@2"] };
  }
}

const bundles = collectBundles(BUNDLE_DIR);
if (bundles.length === 0) {
  console.error(`未找到任何构建产物（*.dmg / *.exe / *.AppImage / *.deb）于 ${BUNDLE_DIR}`);
  process.exit(1);
}

const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY;
const privateKeyPassword = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "";
const platforms = {};
let signed = 0;
const skipped = [];

for (const file of bundles) {
  const ext = BUNDLE_EXTS.find((e) => file.endsWith(e));
  const filename = file.split(/[\\/]/).pop();
  const assetUrl = `https://github.com/${REPO}/releases/download/v${VERSION}/${filename}`;

  let signature = "";
  let unsigned = false;
  if (privateKey) {
    const env = {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: privateKey,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: privateKeyPassword,
    };
    const { cmd, args } = resolveSignerCommand();
    const result = spawnSync(cmd, [...args, "signer", "sign", file], {
      env,
      stdio: ["ignore", "inherit", "inherit"],
    });
    if (result.status === 0) {
      const sigFile = `${file}.sig`;
      try {
        signature = readFileSync(sigFile, "utf8").trim();
        signed += 1;
        console.log(`已签名: ${filename}`);
      } catch {
        console.error(`签名完成但未找到 ${sigFile}`);
        unsigned = true;
        skipped.push(filename);
      }
    } else {
      console.error(`签名失败: ${filename}`);
      // 已配置密钥却签名失败：阻断发布，避免客户端收到无法校验的更新
      process.exit(1);
    }
  } else {
    console.warn(`[跳过签名] 未配置 TAURI_SIGNING_PRIVATE_KEY，跳过 ${filename}`);
    unsigned = true;
    skipped.push(filename);
  }

  // .deb 不参与 updater（Linux 用 AppImage），仅签名上传
  if (PLATFORM_MAP[ext]) {
    if (unsigned) {
      // 未签名平台不写进 latest.json——客户端 updater 不做未签名校验，
      // 发布空平台清单保住"缺密钥不阻塞发布"语义，待密钥就绪后重发。
      console.warn(`[跳过发布] 平台 ${PLATFORM_MAP[ext]} 无签名，不写入 latest.json`);
    } else {
      platforms[PLATFORM_MAP[ext]] = { url: assetUrl, signature };
    }
  }
}

const notes = process.env.RELEASE_NOTES ?? "see CHANGELOG";
const latest = {
  version: VERSION,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync(join(process.cwd(), "latest.json"), JSON.stringify(latest, null, 2) + "\n");
console.log(
  `已写出 latest.json（version=${VERSION}，平台=${Object.keys(platforms).length}，签名=${signed}）`,
);

if (skipped.length > 0) {
  console.warn(`未签名文件: ${skipped.join(", ")}`);
  console.warn("请配置 TAURI_SIGNING_PRIVATE_KEY 仓库 secret 后重新发布，否则客户端无法验证更新。");
}
