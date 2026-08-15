#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import crypto from "node:crypto";
import http from "node:http";
import net from "node:net";
import path from "node:path";
/**
 * lan-proxy 冒烟测试（零依赖，仅 node:test / node:assert / node:http / node:net）
 *
 * 与真实 dsh harness（127.0.0.1:53553）完全无关：起一个临时 mock 上游 + 一个代理
 * 子进程，逐项验证 token 门禁、Host/Origin 改写、跨站拒绝、WS 升级、hop-by-hop
 * 头剥离。
 *
 * 运行：
 *   /usr/local/bin/node --test scripts/lan-proxy.test.mjs
 */
import test from "node:test";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROXY_SCRIPT = path.join(__dirname, "lan-proxy.mjs");

// 随机 token：每次运行都不同，避免与环境变量/固定值撞车
const TOKEN = crypto.randomBytes(16).toString("hex");

let mockServer;
let mockPort;
let proxyPort;
let proxyProc;
// mock 上游记录最近一次收到的请求头（用于断言 Host/Origin 改写）
let lastReceivedHeaders = null;

// ---- 工具函数 ----

// 找一个空闲端口（绑定 0 拿系统分配端口后立即释放）
function freePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });
}

// 轮询直到端口可连（避免时序竞态：代理监听就绪前不做断言）
function waitForPort(port, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const sock = net.connect({ host: "127.0.0.1", port });
      sock.on("connect", () => {
        sock.destroy();
        resolve();
      });
      sock.on("error", () => {
        sock.destroy();
        if (Date.now() > deadline) reject(new Error(`端口 ${port} 在 ${timeoutMs}ms 内未就绪`));
        else setTimeout(attempt, 50);
      });
    };
    attempt();
  });
}

// 走代理发一个 HTTP 请求，返回 { status, headers, body }
function httpGet(port, { method = "GET", path = "/", token, headers = {}, body } = {}) {
  return new Promise((resolve, reject) => {
    const h = { ...headers };
    if (token !== undefined) h.Authorization = `Bearer ${token}`;
    const req = http.request({ host: "127.0.0.1", port, method, path, headers: h }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () =>
        resolve({
          status: res.statusCode,
          headers: res.headers,
          body: Buffer.concat(chunks).toString("utf8"),
        }),
      );
    });
    req.on("error", reject);
    if (body !== undefined) req.write(body);
    req.end();
  });
}

// 用裸 TCP 走代理发 WS 升级请求，返回服务端回写的前两段（状态行 + 头）
function wsUpgrade(port, token) {
  return new Promise((resolve, reject) => {
    const sock = net.connect({ host: "127.0.0.1", port });
    let buf = "";
    const timer = setTimeout(() => {
      sock.destroy();
      reject(new Error("WS 升级超时"));
    }, 5000);
    sock.on("connect", () => {
      const key = crypto.randomBytes(16).toString("base64");
      sock.write(
        "GET /api/events.mux HTTP/1.1\r\n" +
          `Host: 127.0.0.1:${port}\r\n` +
          `Authorization: Bearer ${token}\r\n` +
          "Connection: Upgrade\r\n" +
          "Upgrade: websocket\r\n" +
          `Sec-WebSocket-Key: ${key}\r\n` +
          "Sec-WebSocket-Version: 13\r\n" +
          `Origin: http://127.0.0.1:${port}\r\n` +
          "\r\n",
      );
    });
    sock.on("data", (d) => {
      buf += d.toString("utf8");
      if (buf.includes("\r\n\r\n")) {
        clearTimeout(timer);
        sock.destroy();
        resolve(buf);
      }
    });
    sock.on("error", (e) => {
      clearTimeout(timer);
      reject(e);
    });
  });
}

// ---- 生命周期：起 mock + 代理，测完必清理 ----
test.before(async () => {
  // 1) mock 上游：模拟 dsh 行为（fence 拒绝 cross-site、回环校验、WS 101）
  mockServer = http.createServer((req, res) => {
    lastReceivedHeaders = req.headers;
    // 模拟 dsh browser-trust fence：Sec-Fetch-Site: cross-site 一律 403
    if (req.headers["sec-fetch-site"] === "cross-site") {
      res.writeHead(403, { "Content-Type": "text/plain; charset=utf-8" });
      res.end("cross-site blocked");
      return;
    }
    // hop-by-hop 剥离检查专用：模拟上游发出 Connection/Keep-Alive/Trailer + 双 Set-Cookie + Content-Length。
    // 用裸 socket 原样回写（node:http 不允许 Trailer 与固定 Content-Length 同用，会抛 ERR_HTTP_TRAILER_INVALID）。
    if (req.url === "/hopcheck") {
      const body = "hop-ok";
      const raw =
        "HTTP/1.1 200 OK\r\n" +
        "Content-Type: text/plain; charset=utf-8\r\n" +
        "Connection: keep-alive\r\n" +
        "Keep-Alive: timeout=5\r\n" +
        "Trailer: X-Checksum\r\n" +
        "Set-Cookie: a=1; Path=/" +
        "\r\n" +
        "Set-Cookie: b=2; Path=/" +
        "\r\n" +
        "Content-Length: " +
        Buffer.byteLength(body) +
        "\r\n" +
        "\r\n" +
        body;
      res.socket.write(raw);
      res.socket.end();
      return;
    }
    if (req.method === "POST" && req.url.startsWith("/api")) {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, url: req.url }));
      return;
    }
    res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
    res.end("mock-ok");
  });
  mockServer.on("upgrade", (req, socket) => {
    // 兜住代理侧可能因客户端提前断开而触发的写入错误
    socket.on("error", () => {});
    const key = req.headers["sec-websocket-key"] || "";
    const accept = crypto
      .createHash("sha1")
      .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
      .digest("base64");
    socket.write(
      "HTTP/1.1 101 Switching Protocols\r\n" +
        "Upgrade: websocket\r\n" +
        "Connection: Upgrade\r\n" +
        `Sec-WebSocket-Accept: ${accept}\r\n` +
        "\r\n",
    );
    // 保持一小会儿再关，模拟真实 WS 服务（客户端读到 101 即已通过）
    setTimeout(() => socket.end(), 100);
  });
  await new Promise((resolve, reject) => {
    mockServer.once("error", reject);
    mockServer.listen(0, "127.0.0.1", resolve);
  });
  mockPort = mockServer.address().port;

  // 2) 代理子进程：--target 指向 mock，随机 token，独立端口
  proxyPort = await freePort();
  proxyProc = spawn(
    process.execPath,
    [
      PROXY_SCRIPT,
      "--bind",
      "127.0.0.1",
      "--port",
      String(proxyPort),
      "--target",
      `127.0.0.1:${mockPort}`,
      "--token",
      TOKEN,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  proxyProc.stdout.on("data", () => {});
  proxyProc.stderr.on("data", (d) => process.stderr.write(`[proxy] ${d}`));
  proxyProc.on("exit", (code, signal) => {
    process.stderr.write(`[proxy] exit code=${code} signal=${signal}\n`);
  });

  // 3) 等代理端口就绪，再开始断言
  await waitForPort(proxyPort);
});

test.after(async () => {
  if (proxyProc && proxyProc.exitCode === null) proxyProc.kill("SIGKILL");
  if (mockServer) await new Promise((resolve) => mockServer.close(resolve));
});

// ---- 用例 ----

test("无 token GET → 401", async () => {
  const r = await httpGet(proxyPort, { path: "/" });
  assert.equal(r.status, 401);
});

test("错误 token GET → 401", async () => {
  const r = await httpGet(proxyPort, { path: "/", token: "wrong-token" });
  assert.equal(r.status, 401);
});

test("正确 token GET → 200", async () => {
  const r = await httpGet(proxyPort, { path: "/", token: TOKEN });
  assert.equal(r.status, 200);
  assert.equal(r.body, "mock-ok");
});

test("正确 token POST /api + Origin → 上游看到回环 Host/Origin 且响应 200", async () => {
  const r = await httpGet(proxyPort, {
    method: "POST",
    path: "/api/echo",
    token: TOKEN,
    headers: { Origin: "http://192.168.1.50:8080", "Content-Type": "application/json" },
    body: JSON.stringify({ type: "client-request" }),
  });
  assert.equal(r.status, 200);
  assert.ok(lastReceivedHeaders, "mock 应记录到请求头");
  assert.equal(lastReceivedHeaders.host, `127.0.0.1:${mockPort}`, "Host 应被改写为回环目标");
  assert.equal(
    lastReceivedHeaders.origin,
    `http://127.0.0.1:${mockPort}`,
    "Origin 应被改写为回环目标",
  );
});

test("Sec-Fetch-Site: cross-site → 403", async () => {
  const r = await httpGet(proxyPort, {
    method: "POST",
    path: "/api/events.mux",
    token: TOKEN,
    headers: { "Sec-Fetch-Site": "cross-site", "Content-Type": "application/json" },
    body: "{}",
  });
  assert.equal(r.status, 403);
});

test("WS 升级经代理 → 客户端收到 101", async () => {
  const resp = await wsUpgrade(proxyPort, TOKEN);
  assert.match(resp, /^HTTP\/1\.1 101 /);
  assert.match(resp, /sec-websocket-accept:/i);
});

test("响应剥掉 hop-by-hop 头（connection/keep-alive/trailer）且保留 set-cookie 数组", async () => {
  // 客户端带 Connection: close，让 Node 只加它自己管理的 framing（close + chunked），
  // 从而能确定性地断言上游的 keep-alive/trailer 头没有漏到客户端。
  const r = await httpGet(proxyPort, {
    path: "/hopcheck",
    token: TOKEN,
    headers: { Connection: "close" },
  });
  assert.equal(r.status, 200);
  assert.equal(r.body, "hop-ok");
  const lk = Object.fromEntries(Object.entries(r.headers).map(([k, v]) => [k.toLowerCase(), v]));
  // 上游发的 Connection: keep-alive 不得透传（Node 自己的 Connection: close 属于托管 framing）
  assert.notEqual(lk.connection, "keep-alive", `上游 connection 头不应漏出: ${JSON.stringify(lk)}`);
  assert.ok(!("keep-alive" in lk), `不应有 keep-alive 头: ${JSON.stringify(lk)}`);
  assert.ok(!("trailer" in lk), `不应有 trailer 头: ${JSON.stringify(lk)}`);
  // 多值数组（set-cookie）必须原样保留，不得被拍平/丢弃
  assert.ok(
    Array.isArray(lk["set-cookie"]) && lk["set-cookie"].length === 2,
    `set-cookie 应保留为数组: ${JSON.stringify(lk["set-cookie"])}`,
  );
});
