#!/usr/bin/env node
/**
 * dsh-web 局域网反向代理（零依赖，仅 Node 内置模块）
 *
 * 解决的问题：
 *   dsh web 默认只绑 127.0.0.1，局域网其他设备无法访问。直接改绑 0.0.0.0
 *   又绕不开 dsh 的 browser-trust fence：/api 与 WebSocket 全部按 Host/Origin
 *   头校验，非 loopback 一律 403，且特权方法（settings/credentials/host.* 等）
 *   被硬编码钉死在 loopback，靠 --trusted-host 也救不了。
 *
 * 本脚本把「本机回环身份」伪造成 dsh 看到的来源：
 *   - 改写 Host   → 目标地址（默认 127.0.0.1:53553）
 *   - 改写 Origin → 目标地址（浏览器 POST/WS 都会带 Origin，不改必 403）
 *   - 原样透传 Sec-Fetch-Site（保留跨站防护，cross-site 依旧 403）
 *   - 处理 WebSocket 升级（/api/events.mux、/api/events.host）与流式响应
 *
 *
 * 用法：
 *   node scripts/lan-proxy.mjs [--bind 0.0.0.0] [--port 8080]
 * 环境变量（同名参数优先）：DSH_PROXY_BIND / DSH_PROXY_PORT /
 */
import http from "node:http";
import os from "node:os";

const HELP = `dsh-web 局域网反向代理

用法:
  node scripts/lan-proxy.mjs [选项]

选项:
  --bind <host>       监听地址，默认 0.0.0.0（局域网可访问）
  --port <port>       监听端口，默认 8080
  --target <host:port> 上游 dsh web 地址，默认 127.0.0.1:53553
  --help, -h          显示帮助

环境变量（优先级低于同名参数）:

示例:
  node scripts/lan-proxy.mjs --port 9090
`;

// ---- 参数解析 ----
const argv = process.argv.slice(2);
function opt(name, envName, dflt) {
  const i = argv.indexOf(`--${name}`);
  if (i !== -1 && argv[i + 1] !== undefined && !argv[i + 1].startsWith("--")) return argv[i + 1];
  const e = process.env[envName];
  return e !== undefined && e !== "" ? e : dflt;
}
if (argv.includes("--help") || argv.includes("-h")) {
  console.log(HELP);
  process.exit(0);
}

const BIND = opt("bind", "DSH_PROXY_BIND", "0.0.0.0");
const PORT = Number(opt("port", "DSH_PROXY_PORT", "8080"));
const TARGET_RAW = opt("target", "DSH_PROXY_TARGET", "127.0.0.1:53553");

if (!Number.isInteger(PORT) || PORT < 1 || PORT > 65535) {
  console.error(`[lan-proxy] 端口无效: ${PORT}`);
  process.exit(1);
}
let TARGET;
try {
  TARGET = new URL(`http://${TARGET_RAW}`);
} catch {
  console.error(`[lan-proxy] 目标地址无效: ${TARGET_RAW}（应为 host 或 host:port）`);
  process.exit(1);
}
const TARGET_HOST = TARGET.host; // 规范化后的 host:port
const TARGET_ORIGIN = `http://${TARGET_HOST}`;
const TARGET_PORT = TARGET.port || "80";

// ---- 头改写 ----
// RFC 2616 §13.5.1 标准 hop-by-hop 头；proxy-* 按前缀覆盖，未来新增的代理头也能兜住
const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);
function isHopByHop(key) {
  const lk = String(key).toLowerCase();
  return HOP_BY_HOP.has(lk) || lk.startsWith("proxy-");
}
/**
 * 清理并改写转发头：剥掉 hop-by-hop，把 Host/Origin 改写成 loopback 目标，
 * 其余（含 Sec-Fetch-Site、Cookie、X-* 等）原样保留。
 */
function rewriteHeaders(headers, { keepUpgrade }) {
  const out = {};
  for (const [key, value] of Object.entries(headers)) {
    const lk = key.toLowerCase();
    if (isHopByHop(lk)) continue;
    if (lk === "host") {
      out.host = TARGET_HOST;
      continue;
    }
    if (lk === "origin") {
      out.origin = TARGET_ORIGIN;
      continue;
    }
    out[lk] = value;
  }
  if (keepUpgrade) {
    out.connection = headers.connection;
    out.upgrade = headers.upgrade;
  }
  return out;
}

// 响应回写前过滤：剥掉 hop-by-hop / framing 头（connection / upgrade /
// transfer-encoding / content-length / keep-alive / trailer / te / proxy-*），
// 让 Node 重新管理传输帧。数组值（如 set-cookie）原样保留，不拍平、不 stringify。
const RESPONSE_EXCLUDE = new Set([
  "connection",
  "upgrade",
  "transfer-encoding",
  "content-length",
  "keep-alive",
  "trailer",
  "te",
]);
function filterResponseHeaders(headers) {
  const out = {};
  for (const [key, value] of Object.entries(headers)) {
    const lk = key.toLowerCase();
    if (RESPONSE_EXCLUDE.has(lk) || lk.startsWith("proxy-")) continue;
    out[lk] = value;
  }
  return out;
}

// ---- HTTP 转发 ----
const server = http.createServer((req, res) => {
  const proxyReq = http.request(
    {
      hostname: TARGET.hostname,
      port: TARGET_PORT,
      method: req.method,
      path: req.url,
      headers: rewriteHeaders(req.headers, { keepUpgrade: false }),
    },
    (proxyRes) => {
      res.writeHead(proxyRes.statusCode || 502, filterResponseHeaders(proxyRes.headers));
      proxyRes.pipe(res); // 流式透传（SSE/大响应/分块编码）
    },
  );
  proxyReq.on("error", (err) => {
    if (!res.headersSent) res.writeHead(502, { "Content-Type": "text/plain; charset=utf-8" });
    res.end(`网关错误: ${err.message}`);
  });
  // 客户端中途断开（关页/闪断）时销毁上游请求：及时释放连接，也避免继续写已关闭的 socket
  res.on("close", () => proxyReq.destroy());
  req.pipe(proxyReq);
});

// ---- WebSocket 升级转发 ----
// 转发给客户端时由本机管理的 framing 头（状态行 + Connection/Upgrade 另写），
// 其余上游头原样回写（含 Sec-WebSocket-Accept / Extensions / Protocol）
const RELAY_EXCLUDE = new Set(["connection", "upgrade", "transfer-encoding", "content-length"]);

server.on("upgrade", (req, socket, head) => {
  const proxyReq = http.request({
    hostname: TARGET.hostname,
    port: TARGET_PORT,
    method: "GET",
    path: req.url,
    headers: rewriteHeaders(req.headers, { keepUpgrade: true }),
    agent: false, // 每个 WS 独立连接
  });
  proxyReq.on("upgrade", (proxyRes, proxySocket, proxyHead) => {
    const headerLines = Object.entries(proxyRes.headers)
      .filter(([k]) => !RELAY_EXCLUDE.has(k.toLowerCase()) && !isHopByHop(k))
      .map(([k, v]) => `${k}: ${Array.isArray(v) ? v.join(", ") : v}`);
    const head = [
      "HTTP/1.1 101 Switching Protocols",
      `Upgrade: ${proxyRes.headers.upgrade || "websocket"}`,
      "Connection: Upgrade",
      ...headerLines,
      "",
      "",
    ].join("\r\n");
    socket.write(head);
    if (proxyHead && proxyHead.length > 0) socket.write(proxyHead);
    proxySocket.pipe(socket);
    socket.pipe(proxySocket);
    // 任一侧断开（关页/闪断/强杀）都销毁对侧：既避免未处理 EPIPE 击穿进程，也及时释放上游连接
    socket.on("error", () => proxySocket.destroy());
    socket.on("close", () => proxySocket.destroy());
    proxySocket.on("error", () => socket.destroy());
    proxySocket.on("close", () => socket.destroy());
  });
  // 上游拒绝升级（403/404 等）时 Node 触发 response 而非 upgrade：
  // 必须把真实状态码/头回给局域网客户端，否则客户端会一直空等挂起。
  proxyReq.on("response", (proxyRes) => {
    const chunks = [];
    proxyRes.on("data", (c) => chunks.push(c));
    proxyRes.on("error", () => socket.destroy());
    proxyRes.on("end", () => {
      const body = Buffer.concat(chunks);
      const headerLines = Object.entries(proxyRes.headers)
        .filter(([k]) => !RELAY_EXCLUDE.has(k.toLowerCase()) && !isHopByHop(k))
        .map(([k, v]) => `${k}: ${Array.isArray(v) ? v.join(", ") : v}`);
      const head = [
        `HTTP/1.1 ${proxyRes.statusCode} ${proxyRes.statusMessage || ""}`,
        ...headerLines,
        "Connection: close",
        `Content-Length: ${body.length}`,
        "",
        "",
      ].join("\r\n");
      socket.end(Buffer.concat([Buffer.from(head, "utf8"), body]));
    });
  });
  proxyReq.on("error", () => socket.destroy());
  if (head && head.length > 0) proxyReq.write(head); // 客户端预发的帧（一般为空）
  proxyReq.end();
});

server.on("clientError", (_err, socket) => {
  if (socket.writable) socket.end("HTTP/1.1 400 Bad Request\r\n\r\n");
});

// ---- 启动 ----
function lanAddresses() {
  const out = [];
  for (const infos of Object.values(os.networkInterfaces())) {
    for (const info of infos ?? []) {
      if (info.family === "IPv4" && !info.internal) out.push(info.address);
    }
  }
  return out;
}

server.on("error", (err) => {
  console.error(`[lan-proxy] 启动失败: ${err.message}`);
  process.exit(1);
});

server.listen(PORT, BIND, () => {
  console.log(`[lan-proxy] 已启动`);
  console.log(`  监听:      ${BIND}:${PORT}`);
  console.log(`  上游 dsh:  http://${TARGET_HOST}`);
  for (const ip of lanAddresses()) {
    console.log(`  局域网访问: http://${ip}:${PORT}`);
  }
});
