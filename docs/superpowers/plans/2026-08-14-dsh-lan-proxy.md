# dsh web 局域网反向代理 实现计划

> **For agentic workers:** 本计划为独立小任务，无需子代理编排；步骤用 checkbox（`- [ ]`）追踪。

**Goal:** 让局域网内其他设备（手机 / 平板 / 第二台电脑）能访问本机 `dsh web` GUI（当前 `127.0.0.1:53553`），且不牺牲 dsh 的 browser-trust fence 防护、不重启 dsh、不丢会话。交付物：零依赖 Node 反代脚本 + 使用说明。

**Architecture:** 反代脚本 `scripts/lan-proxy.mjs` 监听 `0.0.0.0:<port>`，把请求转发到本机 `127.0.0.1:53553`。关键在**头改写**：把 `Host` 与 `Origin` 都改写成 `127.0.0.1:53553`（loopback），使 dsh 的 fence 认为所有请求来自本机回环——这样连被 fence 硬编码钉死在 loopback 的特权方法（`settings.*`、`credentials.*`、`host.*`、`llm.discoverModels`、`agentPreset.*`）都能在远程设备上正常使用。`Sec-Fetch-Site` 原样透传，保住跨站防护。HTTP 与 WebSocket 升级（`/api/events.mux`、`/api/events.host`）都走同一套改写与可选的 Bearer token 门禁。

**Tech Stack:** Node（仅内置 `node:http` / `node:os`，零依赖）。

**Spec:** 见下方「设计依据」一节（本任务已完成可行性实证，无需独立 spec 文件）。

## 背景与设计依据

1. **fence 机制**（`@deepseek-ai/dsh-client-connection` 的 `isTrustedApiRequest`）：`/api` 与 WebSocket 每次请求都校验 `Host` 头——loopback（127.* / localhost）放行，非 loopback 必须命中 `--trusted-host` 白名单；`Origin` 存在时其 host 必须与 `Host` 一致；`Sec-Fetch-Site: cross-site` 一律拒绝。
2. **特权方法钉死 loopback**：`settings/credentials/host/llm.discoverModels/agentPreset` 等方法在 fence 后仍以空白名单二次校验，`--trusted-host` 也救不了 → 直接 `--host 0.0.0.0` 暴露时远程设备拿到的 UI 是残缺的。
3. **fence 不是认证层**（源码注释原文）：谁连上谁就是本机权限 → 反代必须自带认证门禁。
4. **实证结论**（对本机线上 harness 直接 curl 验证）：

| 模拟请求（反代转发后 dsh 所见） | 结果 |
| --- | --- |
| Host=127.0.0.1:53553，Origin=192.168.1.5:8080（只改 Host） | 403 |
| Host=127.0.0.1:53553，Origin=127.0.0.1:53553（Host+Origin 都改） | 200 |
| Host=127.0.0.1:53553，无 Origin（改 Host + 剥 Origin） | 200 |
| Host=192.168.1.5:8080（不改写，靠 trusted-host） | 403 |
| 任意 + `Sec-Fetch-Site: cross-site` | 403 |

→ **反代必须同时改写 Host 与 Origin**；只改 Host 必挂。

## Global Constraints

- 零依赖：只用 Node 内置模块，`node scripts/lan-proxy.mjs` 直接可跑，不碰 Yarn/PnP。
- dsh 侧**不做任何改动**：不重启、不换 `--host`、不加 `--trusted-host`。
- 用户可见文案与注释使用中文。
- 默认不启用 token 也要在启动日志显著警告（fence 非认证层）。
- 转发时必须剥掉 hop-by-hop 头（`connection/keep-alive/transfer-encoding/upgrade/te/trailer/proxy-*`），由 Node 重新管理连接语义。

---

## Task 1: 反代脚本 `scripts/lan-proxy.mjs`

**Files:**
- Create: `scripts/lan-proxy.mjs`
- Optional: 根 `package.json` 增加 `lan-proxy` 脚本入口

**Interfaces:**
- Consumes: 上游 `dsh web`（默认 `127.0.0.1:53553`）。
- Produces: `0.0.0.0:<port>` 上的 HTTP + WebSocket 反代端点；启动日志打印局域网访问地址。

- [x] **Step 1: 编写脚本**（已完成）
  - 参数：`--bind`（默认 0.0.0.0）、`--port`（默认 8080）、`--target`（默认 127.0.0.1:53553）、`--token`（可选）；同名字段也支持 `DSH_PROXY_*` 环境变量。
  - HTTP 转发：`rewriteHeaders` 改写 Host/Origin、剥离 hop-by-hop，其余原样透传；响应 `pipe` 流式透传（SSE/大响应/分块编码）。
  - WebSocket：监听 `upgrade` 事件，改写后以 `agent:false` 建独立连接，把上游 101 响应头原样回写客户端后双向 `pipe`。
  - token 门禁：HTTP 请求与 WS 握手都要求 `Authorization: Bearer <token>`，不匹配回 401。
  - 启动日志：`os.networkInterfaces()` 探测 IPv4 局域网地址并打印访问 URL；未设 token 时打印醒目警告。

- [x] **Step 2: 验证**（本机已完成，复测命令见下）（本机/评审已实测）

## Task 2: 使用与验证

- [x] **Step 1: 启动反代**

```bash
node scripts/lan-proxy.mjs --token "<随机长串>"            # 默认 0.0.0.0:8080 → 127.0.0.1:53553
node scripts/lan-proxy.mjs --port 9090                    # 换端口；无 token 会有警告
```

- [x] **Step 2: 本机自测**（期望值见注释）

```bash
# 无 token → 401
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8080/
# 对 token → 200（页面）
curl -s -o /dev/null -w "%{http_code}\n" -H "Authorization: Bearer <token>" http://127.0.0.1:8080/
# 模拟浏览器：POST /api + Origin（验证 Host/Origin 改写穿透 fence）→ 200
curl -s -o /dev/null -w "%{http_code}\n" -X POST -H "Authorization: Bearer <token>" -H "Origin: http://127.0.0.1:8080" -H "Content-Type: application/json" -d '{"type":"client-request","rpcId":"verify-1","method":"settings.describe","payload":{}}' http://127.0.0.1:8080/api/settings.describe
# 跨站标记 → 403
curl -s -o /dev/null -w "%{http_code}\n" -X POST -H "Authorization: Bearer <token>" -H "Sec-Fetch-Site: cross-site" -H "Content-Type: application/json" -d '{"type":"client-request","rpcId":"verify-1","method":"settings.describe","payload":{}}' http://127.0.0.1:8080/api/settings.describe
```

- [x] **Step 3: 局域网端到端**
  1. 手机/其他设备连同一 Wi-Fi。
  2. 访问启动日志打印的 `http://<局域网IP>:8080`（如 `http://192.168.8.239:8080`）。
  3. 确认：页面加载、会话/流式正常、**设置/凭证/模型发现等特权功能可用**（这是 Host/Origin 改写生效的直接证据）。
  4. 若提示输入凭证：反代不支持浏览器弹窗，token 需要手动注入——可用浏览器扩展/开发者工具在请求头加 `Authorization: Bearer <token>`（或先以无 token 模式在可信网络验证全流程，再决定门禁形态）。

## 安全说明

- **fence 不是认证层**：拿到访问权 = 本机完整 harness 权限（跑代码、读写文件、取凭证）。
- 局域网模式至少设 token；公网场景**必须** token / Cloudflare Access 且走 HTTPS。
- macOS 首次启动会弹「允许接受传入连接」——允许该 node 进程。

## 公网扩展（后续，不在本次范围）

架构不变：`公网设备 → Cloudflare Tunnel / ngrok / VPS+frp → 本机反代 :8080 → dsh`。反代已把 Host/Origin 改写掉，上层隧道无需任何头配置、对隧道透明。待办：命名隧道 + 自有域名、Cloudflare Access 或 token 认证、launchd 开机自启、可选固化进 dsh-desktop（`apps/shell/src-tauri` 的启动逻辑加「局域网/公网共享」开关）。
