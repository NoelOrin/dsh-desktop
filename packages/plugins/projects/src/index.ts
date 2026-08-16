// 项目插件 host 面：为壳侧注入的侧边栏右键菜单提供 dsh 工作区数据与动作端点。
// 菜单 UI 由壳注入脚本渲染（apps/shell/src-tauri/src/lib.rs HARNESS_CHROME_SCRIPT），
// 本文件只暴露 loopback 端点，不做 UI 业务。
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { promisify } from "node:util";

export const name = "projects";

const execFileAsync = promisify(execFile);

/** dsh WorkspaceRegistry 服务与工作区实体的最小结构面（运行时由 dsh 提供）。 */
interface WorkspaceLike {
  readonly id: string;
  readonly title: string;
  readonly path: string;
  readonly sessionIds: readonly string[];
  setTitle(title: string): Promise<void>;
  insertSessionBefore(sessionId: string, beforeSessionId?: string): Promise<void>;
}

interface WorkspaceRegistryLike {
  list(): WorkspaceLike[];
  create(path: string, title?: string): Promise<WorkspaceLike>;
  delete(id: string): Promise<boolean>;
  insertBefore(id: string, beforeId?: string): Promise<readonly string[]>;
  archiveSession(sessionId: string): Promise<void>;
  readonly archivedSessionIds: readonly string[];
}

interface IncomingMessageLike {
  method?: string;
  headers?: Record<string, string | string[] | undefined>;
  on(event: string, listener: (chunk?: unknown) => void): unknown;
}

interface ServerResponseLike {
  writeHead(code: number, headers?: Record<string, string>): void;
  end(body?: string): void;
}

interface WebServerLike {
  register(route: {
    kind: "exact";
    path: string;
    handler: (req: IncomingMessageLike, res: ServerResponseLike) => void | Promise<void>;
  }): () => void;
}

interface ProjectsInjectContext {
  webServer: WebServerLike;
  effect(execute: () => unknown, label?: string): unknown;
}

/** 壳注入脚本需要的服务面：webServer 走 inject，workspaceRegistry 按需 ctx.get。 */
interface ProjectsContext {
  inject(
    dependencies: readonly string[],
    callback: (ctx: ProjectsInjectContext) => unknown,
  ): unknown;
  get(name: string, strict?: boolean): unknown;
}

function registryOf(ctx: ProjectsContext): WorkspaceRegistryLike | null {
  return (ctx.get("workspaceRegistry") as WorkspaceRegistryLike | undefined) ?? null;
}

interface SessionLike {
  readonly id: string;
  readonly header: SessionHeaderLike;
}

interface SessionStoreLike {
  list(): SessionLike[];
}

interface SessionTitleLike {
  get(session: SessionLike): { readonly title: string } | undefined;
}

interface SessionHeaderLike {
  readonly id: string;
  readonly cwd?: string;
}

interface ProjectionSnapshotLike {
  readonly values: Record<string, unknown>;
}

interface SessionProjectionsLike {
  snapshot(session: unknown): ProjectionSnapshotLike | undefined;
}

interface SessionProjectionCacheLike {
  cachedSnapshot(meta: SessionHeaderLike): ProjectionSnapshotLike | undefined;
}

interface SessionPersistenceLike {
  list(): Promise<SessionHeaderLike[]>;
}

function sessionStoreOf(ctx: ProjectsContext): SessionStoreLike | null {
  return (ctx.get("sessions") as SessionStoreLike | undefined) ?? null;
}

function sessionTitleOf(ctx: ProjectsContext): SessionTitleLike | null {
  return (ctx.get("sessionTitle") as SessionTitleLike | undefined) ?? null;
}

function sessionPersistenceOf(ctx: ProjectsContext): SessionPersistenceLike | null {
  return (ctx.get("sessionPersistence") as SessionPersistenceLike | undefined) ?? null;
}

function sessionProjectionsOf(ctx: ProjectsContext): SessionProjectionsLike | null {
  return (ctx.get("sessionProjections") as SessionProjectionsLike | undefined) ?? null;
}

function sessionProjectionCacheOf(ctx: ProjectsContext): SessionProjectionCacheLike | null {
  return (ctx.get("sessionProjectionCache") as SessionProjectionCacheLike | undefined) ?? null;
}

function projectionTitle(snapshot: ProjectionSnapshotLike | undefined): string | undefined {
  const value = snapshot?.values.title;
  return typeof value === "string" && value !== "" ? value : undefined;
}

function liveProjectionTitle(
  projections: SessionProjectionsLike | null,
  session: SessionLike,
): string | undefined {
  try {
    return projectionTitle(projections?.snapshot(session));
  } catch {
    return undefined;
  }
}

function cachedProjectionTitle(
  cache: SessionProjectionCacheLike | null,
  header: SessionHeaderLike,
): string | undefined {
  try {
    return projectionTitle(cache?.cachedSnapshot(header));
  } catch {
    return undefined;
  }
}

interface WorkspacePayload {
  id: string;
  name: string;
  path: string;
  session_ids: string[];
  archived: boolean;
  pinned: boolean;
}

function listWorkspaces(ctx: ProjectsContext): WorkspacePayload[] {
  const registry = registryOf(ctx);
  if (!registry) return [];
  const archived = new Set(registry.archivedSessionIds);
  const workspaces = registry.list();
  const pinnedId = workspaces[0]?.id;
  return workspaces.map((workspace) => ({
    id: workspace.id,
    name: workspace.title,
    path: workspace.path,
    session_ids: [...workspace.sessionIds],
    archived:
      workspace.sessionIds.length > 0 &&
      workspace.sessionIds.every((sessionId) => archived.has(sessionId)),
    pinned: workspace.id === pinnedId,
  }));
}

interface SessionPayload {
  id: string;
  title: string;
  workspace_name: string | null;
}

function displayTitle(id: string, cwd: string | null | undefined, title?: string): string {
  if (title) return title;
  return cwd ? basename(cwd) : id;
}

function listLiveSessions(ctx: ProjectsContext): SessionPayload[] {
  const registry = registryOf(ctx);
  const sessions = sessionStoreOf(ctx);
  if (!registry || !sessions) return [];
  const titles = sessionTitleOf(ctx);
  const workspaces = registry.list();
  return sessions.list().map((session) => {
    const cwd = session.header.cwd;
    const owner = cwd ? workspaces.find((workspace) => workspace.path === cwd) : undefined;
    const title = titles?.get(session)?.title ?? (cwd ? basename(cwd) : session.id);
    return {
      id: session.id,
      title,
      workspace_name: owner?.title ?? null,
    };
  });
}

async function listSessions(ctx: ProjectsContext): Promise<SessionPayload[]> {
  const registry = registryOf(ctx);
  if (!registry) return [];
  const liveSessions = sessionStoreOf(ctx)?.list() ?? [];
  const liveById = new Map(liveSessions.map((session) => [session.id, session]));
  const persistence = sessionPersistenceOf(ctx);
  let headers = liveSessions.map((session) => session.header);
  try {
    if (persistence) {
      const persisted = await persistence.list();
      headers = [...headers, ...persisted.filter((header) => !liveById.has(header.id))];
    }
  } catch {
    return listLiveSessions(ctx);
  }

  const workspaces = registry.list();
  const liveTitles = sessionTitleOf(ctx);
  const projections = sessionProjectionsOf(ctx);
  const projectionCache = sessionProjectionCacheOf(ctx);

  return headers.map((header) => {
    const cwd = header.cwd;
    const owner = cwd ? workspaces.find((workspace) => workspace.path === cwd) : undefined;
    const live = liveById.get(header.id);
    const title = live
      ? (liveTitles?.get(live)?.title ?? liveProjectionTitle(projections, live))
      : cachedProjectionTitle(projectionCache, header);
    return {
      id: header.id,
      title: displayTitle(header.id, cwd, title),
      workspace_name: owner?.title ?? null,
    };
  });
}

type ActionResult = { ok: true; [key: string]: unknown } | { ok: false; error: string };

function sanitizeName(input: string): string {
  const cleaned = input.replace(/[^\p{L}\p{N}._-]+/gu, "-").replace(/^[.-]+|[.-]+$/g, "");
  return cleaned || "worktree";
}

/** 在项目目录旁创建永久 git 工作树，返回（目标目录，分支名）。 */
async function createWorktree(
  repoPath: string,
  title: string,
): Promise<{ target: string; branch: string }> {
  await execFileAsync("git", ["-C", repoPath, "rev-parse", "--is-inside-work-tree"]);
  const base = sanitizeName(`${sanitizeName(title)}-worktree`);
  const parent = dirname(repoPath);
  const branchExists = async (branch: string): Promise<boolean> => {
    try {
      await execFileAsync("git", [
        "-C",
        repoPath,
        "show-ref",
        "--verify",
        "--quiet",
        `refs/heads/${branch}`,
      ]);
      return true;
    } catch {
      return false;
    }
  };
  let target = join(parent, base);
  let branch = `wt/${base}`;
  let index = 2;
  while (existsSync(target)) {
    target = join(parent, `${base}-${index}`);
    index += 1;
  }
  while (await branchExists(branch)) {
    branch = `wt/${base}-${index}`;
    index += 1;
  }
  await execFileAsync("git", ["-C", repoPath, "worktree", "add", "-b", branch, target]);
  return { target, branch };
}

async function runSessionAction(
  ctx: ProjectsContext,
  action: string,
  body: Record<string, unknown>,
): Promise<ActionResult> {
  const registry = registryOf(ctx);
  if (!registry) return { ok: false, error: "workspaceRegistry 服务不可用" };
  const sessionId = String(body.session_id ?? "");
  switch (action) {
    case "session-archive":
      await registry.archiveSession(sessionId);
      return { ok: true };
    default:
      return { ok: false, error: `未知动作: ${action}` };
  }
}

async function runAction(
  ctx: ProjectsContext,
  action: string,
  id: string,
  body: Record<string, unknown>,
): Promise<ActionResult> {
  if (action.startsWith("session-")) {
    return runSessionAction(ctx, action, body);
  }
  const registry = registryOf(ctx);
  if (!registry) return { ok: false, error: "workspaceRegistry 服务不可用" };
  const workspace = registry.list().find((item) => item.id === id);
  if (!workspace) return { ok: false, error: "工作区不存在" };
  try {
    switch (action) {
      case "pin": {
        const first = registry.list()[0];
        if (first && first.id !== id) {
          await registry.insertBefore(id, first.id);
        }
        return { ok: true, pinned: true };
      }
      case "unpin":
        await registry.insertBefore(id);
        return { ok: true, pinned: false };
      case "finder":
        return { ok: true, path: workspace.path };
      case "edit": {
        const name = String(body.name ?? "").trim();
        if (!name) return { ok: false, error: "项目名称不能为空" };
        await workspace.setTitle(name);
        return { ok: true };
      }
      case "archive":
        for (const sessionId of workspace.sessionIds) {
          await registry.archiveSession(sessionId);
        }
        return { ok: true };
      case "remove": {
        const deleted = await registry.delete(id);
        if (!deleted) return { ok: false, error: "工作区不存在" };
        return { ok: true };
      }
      case "worktree": {
        const { target, branch } = await createWorktree(workspace.path, workspace.title);
        await registry.create(target, basename(target));
        return { ok: true, target, branch };
      }
      default:
        return { ok: false, error: `未知动作: ${action}` };
    }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function sendJson(res: ServerResponseLike, code: number, payload: unknown): void {
  res.writeHead(code, { "content-type": "application/json" });
  res.end(JSON.stringify(payload));
}

function headerValue(req: IncomingMessageLike, name: string): string {
  const value = req.headers?.[name.toLowerCase()];
  return Array.isArray(value) ? (value[0] ?? "") : (value ?? "");
}

/** 仅接受来自当前 dsh web loopback origin 的请求，防止其他本地页面构造动作 POST。 */
function isTrustedRequest(req: IncomingMessageLike): boolean {
  const expectedToken = process.env.DSH_DESKTOP_HOST_TOKEN ?? "";
  if (!expectedToken || headerValue(req, "x-dsh-desktop-token") !== expectedToken) {
    return false;
  }
  const host = headerValue(req, "host").toLowerCase();
  const origin = headerValue(req, "origin") || headerValue(req, "referer");
  if (host && origin) {
    try {
      const parsed = new URL(origin);
      return parsed.protocol === "http:" && parsed.host.toLowerCase() === host;
    } catch {
      return false;
    }
  }
  return headerValue(req, "sec-fetch-site").toLowerCase() === "same-origin";
}

const MAX_ACTION_BODY_BYTES = 64 * 1024;

function readBody(req: IncomingMessageLike): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    let rejected = false;
    const fail = (error: Error): void => {
      if (rejected) return;
      rejected = true;
      reject(error);
    };
    req.on("data", (chunk) => {
      if (rejected) return;
      if (chunk !== undefined) {
        const bytes =
          typeof chunk === "string"
            ? Buffer.byteLength(chunk)
            : ((chunk as { length?: number }).length ?? 0);
        size += bytes;
        if (size > MAX_ACTION_BODY_BYTES) {
          fail(new Error("request body too large"));
          return;
        }
        chunks.push(chunk as Buffer);
      }
    });
    req.on("end", () => {
      if (!rejected) resolve(Buffer.concat(chunks).toString("utf8"));
    });
    req.on("error", (error) => fail(error instanceof Error ? error : new Error(String(error))));
  });
}

export function apply(ctx: ProjectsContext): void {
  (ctx as unknown as ProjectsContext).inject(["webServer"] as never, (sctx) => {
    sctx.effect(() => {
      const disposers = [
        sctx.webServer.register({
          kind: "exact",
          path: "/dsh-desktop/workspaces",
          handler: (req, res) => {
            if (!isTrustedRequest(req)) {
              sendJson(res, 403, { ok: false, error: "untrusted request" });
              return;
            }
            sendJson(res, 200, { ok: true, workspaces: listWorkspaces(ctx) });
          },
        }),
        sctx.webServer.register({
          kind: "exact",
          path: "/dsh-desktop/sessions",
          handler: async (req, res) => {
            if (!isTrustedRequest(req)) {
              sendJson(res, 403, { ok: false, error: "untrusted request" });
              return;
            }
            sendJson(res, 200, { ok: true, sessions: await listSessions(ctx) });
          },
        }),
        sctx.webServer.register({
          kind: "exact",
          path: "/dsh-desktop/workspaces/action",
          handler: async (req, res) => {
            if (!isTrustedRequest(req)) {
              sendJson(res, 403, { ok: false, error: "untrusted action origin" });
              return;
            }
            if ((req.method ?? "GET").toUpperCase() !== "POST") {
              sendJson(res, 405, { ok: false, error: "method not allowed" });
              return;
            }
            try {
              const raw = await readBody(req);
              const body = (JSON.parse(raw || "{}") ?? {}) as Record<string, unknown>;
              const result = await runAction(
                ctx,
                String(body.action ?? ""),
                String(body.id ?? ""),
                body,
              );
              sendJson(res, result.ok ? 200 : 400, result);
            } catch (error) {
              sendJson(res, 400, {
                ok: false,
                error: error instanceof Error ? error.message : String(error),
              });
            }
          },
        }),
      ];
      return () => {
        for (const dispose of disposers) dispose();
      };
    }, "projects: 桌面壳 loopback 端点");
  });
}
