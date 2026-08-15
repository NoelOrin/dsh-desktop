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

/** 壳注入脚本需要的服务面：webServer 走 inject，workspaceRegistry 按需 ctx.get。 */
interface ProjectsContext {
  inject(
    dependencies: readonly string[],
    callback: (ctx: { webServer: WebServerLike }) => unknown,
  ): unknown;
  get(name: string, strict?: boolean): unknown;
}

function registryOf(ctx: ProjectsContext): WorkspaceRegistryLike | null {
  return (ctx.get("workspaceRegistry") as WorkspaceRegistryLike | undefined) ?? null;
}

interface SessionLike {
  readonly id: string;
  readonly header: { readonly cwd?: string };
}

interface SessionStoreLike {
  list(): SessionLike[];
}

interface SessionTitleLike {
  get(session: SessionLike): { readonly title: string } | undefined;
}

function sessionStoreOf(ctx: ProjectsContext): SessionStoreLike | null {
  return (ctx.get("sessions") as SessionStoreLike | undefined) ?? null;
}

function sessionTitleOf(ctx: ProjectsContext): SessionTitleLike | null {
  return (ctx.get("sessionTitle") as SessionTitleLike | undefined) ?? null;
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
  cwd: string | null;
  workspace_name: string | null;
  pinned: boolean;
}

function listSessions(ctx: ProjectsContext): SessionPayload[] {
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
      cwd: cwd ?? null,
      workspace_name: owner?.title ?? null,
      pinned: owner ? owner.sessionIds[0] === session.id : false,
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
    case "session-pin": {
      const owner = registry.list().find((item) => item.sessionIds.includes(sessionId));
      if (!owner) return { ok: false, error: "会话不存在" };
      const first = owner.sessionIds[0];
      if (first && first !== sessionId) {
        await owner.insertSessionBefore(sessionId, first);
      }
      return { ok: true, pinned: true };
    }
    case "session-unpin": {
      const owner = registry.list().find((item) => item.sessionIds.includes(sessionId));
      if (!owner) return { ok: false, error: "会话不存在" };
      await owner.insertSessionBefore(sessionId);
      return { ok: true, pinned: false };
    }
    case "session-archive":
      await registry.archiveSession(sessionId);
      return { ok: true };
    case "session-finder": {
      const session = sessionStoreOf(ctx)
        ?.list()
        .find((item) => item.id === sessionId);
      const path =
        session?.header.cwd ??
        registry.list().find((item) => item.sessionIds.includes(sessionId))?.path;
      if (!path) return { ok: false, error: "会话没有项目目录" };
      return { ok: true, path };
    }
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
    sctx.webServer.register({
      kind: "exact",
      path: "/dsh-desktop/workspaces",
      handler: (_req, res) => {
        sendJson(res, 200, { ok: true, workspaces: listWorkspaces(ctx) });
      },
    });
    sctx.webServer.register({
      kind: "exact",
      path: "/dsh-desktop/sessions",
      handler: (_req, res) => {
        sendJson(res, 200, { ok: true, sessions: listSessions(ctx) });
      },
    });
    sctx.webServer.register({
      kind: "exact",
      path: "/dsh-desktop/workspaces/action",
      handler: async (req, res) => {
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
    });
  });
}
