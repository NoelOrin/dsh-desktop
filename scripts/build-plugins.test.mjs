#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
/**
 * build-plugins 纯函数冒烟测试（零依赖，仅 node:test / node:assert）
 *
 * 直接测 buildDistManifest：源 package.json → 内嵌 dist 清单变换
 * （剔除依赖/私有字段/scripts/packageManager/悬空 types，保留 lib 入口与 dsh 字段）。
 *
 * 运行：
 *   node scripts/build-plugins.test.mjs
 */
import test from "node:test";
import { buildDistManifest, deployToProfile } from "./build-plugins.mjs";

test("dist manifest 剔除依赖并指向 lib", () => {
  const dist = buildDistManifest({
    name: "@dsh-desktop/plugin-bridge",
    version: "0.1.0",
    type: "module",
    main: "./lib/index.js",
    exports: { ".": "./lib/index.js", "./client": "./lib/client.js" },
    description: "桥接 Tauri 壳能力",
    dependencies: { "@deepseek-ai/cordis": "^4.0.1" },
    devDependencies: { typescript: "^5" },
    peerDependencies: { react: "^18" },
    private: true,
    dsh: { bundle: { patch: "./cordis.patch.yml" } },
    dshDesktop: { id: "bridge" },
  });
  assert.equal(dist.dependencies, undefined);
  assert.equal(dist.devDependencies, undefined);
  assert.equal(dist.peerDependencies, undefined);
  assert.equal(dist.private, undefined);
  assert.equal(dist.main, "./lib/index.js");
  assert.equal(dist.version, "0.1.0");
  assert.equal(dist.type, "module");
  assert.deepEqual(dist.exports, { ".": "./lib/index.js", "./client": "./lib/client.js" });
  assert.equal(dist.description, "桥接 Tauri 壳能力");
  assert.equal(dist.dsh.bundle.patch, "./cordis.patch.yml");
  assert.equal(dist.dshDesktop.id, "bridge");
});

test("dist manifest 剔除悬空的 types 字段", () => {
  const dist = buildDistManifest({
    name: "@dsh-desktop/plugin-hello",
    version: "0.1.0",
    type: "module",
    main: "./lib/index.js",
    types: "./lib/index.d.ts",
    scripts: { build: "tsdown" },
    packageManager: "yarn@4.17.1",
    dsh: { bundle: { patch: "./cordis.patch.yml" } },
    dshDesktop: { id: "hello" },
  });
  assert.equal(dist.types, undefined);
  assert.equal(dist.scripts, undefined);
  assert.equal(dist.packageManager, undefined);
  assert.equal(dist.name, "@dsh-desktop/plugin-hello");
  assert.equal(dist.main, "./lib/index.js");
  assert.equal(dist.dshDesktop.id, "hello");
});

test("deployToProfile 原子复制到 dsh profile", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "dsh-deploy-"));
  const src = path.join(root, "src");
  const home = path.join(root, "home");
  fs.mkdirSync(src, { recursive: true });
  fs.writeFileSync(path.join(src, "client.js"), "// hot");

  const error = deployToProfile(src, "@dsh-desktop/plugin-bridge", home);
  assert.equal(error, null);
  const target = path.join(
    home,
    "profiles",
    "node_modules",
    "@dsh-desktop",
    "plugin-bridge",
    "client.js",
  );
  assert.equal(fs.readFileSync(target, "utf8"), "// hot");
  fs.rmSync(root, { recursive: true, force: true });
});
