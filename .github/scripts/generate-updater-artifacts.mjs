#!/usr/bin/env node
// 生成 Tauri updater 的 latest.json 与各平台安装包的 .sig 签名。
// 用法: node .github/scripts/generate-updater-artifacts.mjs <VERSION>
// 读取 TAURI_SIGNING_PRIVATE_KEY（私钥 PEM，配置为仓库 secret）与
// TAURI_SIGNING_PRIVATE_KEY_PASSWORD（可选，私钥口令）；缺密钥时打印警告并跳过签名，不阻塞发布。

import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const VERSION = process.argv[2];
if (!VERSION) {
  console.error("缺少版本号参数: node .github/scripts/generate-updater-artifacts.mjs <VERSION>");
  process.exit(1);
}

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
      if (entry === "node_modules" || entry === ".git" || entry === "target") continue;
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

const bundles = collectBundles(process.cwd());
if (bundles.length === 0) {
  console.error("未找到任何构建产物（*.dmg / *.exe / *.AppImage / *.deb）");
  process.exit(1);
}

const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY;
const privateKeyPassword = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "";
const platforms = {};
let signed = 0;
const skipped = [];

for (const file of bundles) {
  const ext = BUNDLE_EXTS.find((e) => file.endsWith(e));
  const filename = file.split(/[/\\]/).pop();
  const assetUrl = `https://github.com/${REPO}/releases/download/v${VERSION}/${filename}`;

  let signature = "";
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
      }
    } else {
      console.error(`签名失败: ${filename}`);
      skipped.push(filename);
    }
  } else {
    console.warn(`[跳过签名] 未配置 TAURI_SIGNING_PRIVATE_KEY，跳过 ${filename}`);
    skipped.push(filename);
  }

  // .deb 不参与 updater（Linux 用 AppImage），仅签名上传
  if (PLATFORM_MAP[ext]) {
    platforms[PLATFORM_MAP[ext]] = { url: assetUrl, signature };
  }
}

const latest = {
  version: VERSION,
  notes: "see CHANGELOG",
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync(join(process.cwd(), "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
console.log(
  `已写出 latest.json（version=${VERSION}，平台=${Object.keys(platforms).length}，签名=${signed}）`,
);

if (skipped.length > 0) {
  console.warn(`未签名文件: ${skipped.join(", ")}`);
  console.warn("请配置 TAURI_SIGNING_PRIVATE_KEY 仓库 secret 后重新发布，否则客户端无法验证更新。");
}
