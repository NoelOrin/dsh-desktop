import {
  Button,
  IconArchiveOutline20,
  IconBranchOutline16,
  IconCheckOutline16,
  IconEditOutline16,
  IconEllipsisOutline16,
  IconFolderOpenOutline16,
  IconPlusOutline16,
  IconTrashOutline16,
  Input,
  Menu,
  type MenuEntry,
  Modal,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { useEffect, useState } from "react";
import { injectPluginCss } from "../../client-kit/inject";
import css from "./client/projects.module.css";
import { getBridge, type ProjectEntry, type Translate } from "./client/runtime";
import { SettingsPage, SettingsSection } from "./client/settings-layout";

injectPluginCss("@dsh-desktop/plugin-projects", "@dsh-desktop/plugin-projects/ui");

/** 平台 Menu 需要 anchor rect；右键菜单与省略号按钮统一走该快照。 */
interface MenuState {
  project: ProjectEntry;
  rect: DOMRect;
}

function buildMenuItems(project: ProjectEntry, t: Translate): MenuEntry[] {
  return [
    {
      id: "pin",
      label: project.pinned ? t("menu.unpin") : t("menu.pin"),
    },
    { id: "finder", label: t("menu.finder"), icon: <IconFolderOpenOutline16 /> },
    { id: "worktree", label: t("menu.worktree"), icon: <IconBranchOutline16 /> },
    { id: "edit", label: t("menu.edit"), icon: <IconEditOutline16 /> },
    { type: "separator", id: "sep-chat" },
    { id: "read", label: t("menu.read"), icon: <IconCheckOutline16 /> },
    {
      id: "archive",
      label: project.archived_chats ? t("menu.unarchive") : t("menu.archive"),
      icon: <IconArchiveOutline20 />,
    },
    { type: "separator", id: "sep-danger" },
    { id: "remove", label: t("menu.remove"), danger: true, icon: <IconTrashOutline16 /> },
  ];
}

function ProjectsPanel({ t }: { t: Translate }): JSX.Element {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [menuState, setMenuState] = useState<MenuState | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [removing, setRemoving] = useState<ProjectEntry | null>(null);

  const refresh = async () => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      setLoading(false);
      return;
    }
    try {
      const items = await bridge.projects.list();
      setProjects(
        [...items].sort(
          (a, b) =>
            Number(b.pinned) - Number(a.pinned) ||
            b.updated_at - a.updated_at ||
            a.created_at - b.created_at,
        ),
      );
      setError(null);
    } catch (e) {
      setError(`${t("error.read")}: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    let disposed = false;
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      setLoading(false);
      return;
    }
    bridge.projects
      .list()
      .then((items) => {
        if (!disposed) {
          setProjects(
            [...items].sort(
              (a, b) =>
                Number(b.pinned) - Number(a.pinned) ||
                b.updated_at - a.updated_at ||
                a.created_at - b.created_at,
            ),
          );
          setError(null);
        }
      })
      .catch((e: unknown) => {
        if (!disposed) setError(`${t("error.read")}: ${String(e)}`);
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [t]);

  const openMenuFrom = (project: ProjectEntry, rect: DOMRect): void => {
    setMenuState({ project, rect });
  };

  const runAction = async (project: ProjectEntry, id: string): Promise<void> => {
    setMenuState(null);
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      setBusy(true);
      switch (id) {
        case "pin":
          await bridge.projects.setPinned(project.id, !project.pinned);
          break;
        case "finder":
          await bridge.projects.showInFinder(project.id);
          break;
        case "worktree": {
          const result = await bridge.projects.createWorktree(project.id);
          setNotice(`${t("worktree.created")}: ${result.target}`);
          break;
        }
        case "edit":
          setEditingId(project.id);
          setEditingName(project.name);
          return;
        case "read":
          await bridge.projects.markRead(project.id);
          setNotice(t("read.done"));
          break;
        case "archive":
          await bridge.projects.setArchived(project.id, !project.archived_chats);
          setNotice(project.archived_chats ? t("unarchived.done") : t("archived.done"));
          break;
        case "remove":
          setRemoving(project);
          return;
      }
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.action")}: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const addProject = async (): Promise<void> => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      const picked = await bridge.dialog.openFile({
        title: t("add.title"),
        directory: true,
        multiple: false,
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      setBusy(true);
      await bridge.projects.add(path);
      setNotice(t("added"));
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.action")}: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const saveEdit = async (project: ProjectEntry): Promise<void> => {
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      await bridge.projects.update({ ...project, name: editingName.trim() });
      setEditingId(null);
      setNotice(t("saved"));
      setError(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.action")}: ${String(e)}`);
    }
  };

  const confirmRemove = async (): Promise<void> => {
    if (!removing) return;
    const bridge = getBridge();
    if (!bridge) {
      setError(t("error.bridge"));
      return;
    }
    try {
      await bridge.projects.remove(removing.id);
      setNotice(t("removed"));
      setError(null);
      setRemoving(null);
      await refresh();
    } catch (e) {
      setError(`${t("error.action")}: ${String(e)}`);
    }
  };

  return (
    <div className={css.stack}>
      <div className={css.toolbar}>
        <Button
          type="button"
          variant="outline"
          icon={<IconPlusOutline16 />}
          disabled={busy}
          onClick={() => void addProject()}
        >
          {t("add.action")}
        </Button>
      </div>

      {projects.length > 0 ? (
        <ul className={css.list} aria-label={t("nav")}>
          {projects.map((project) => (
            <li
              key={project.id}
              className={css.row}
              onContextMenu={(event) => {
                event.preventDefault();
                openMenuFrom(project, new DOMRect(event.clientX, event.clientY, 0, 0));
              }}
            >
              {editingId === project.id ? (
                <div className={css.editRow}>
                  <Input
                    className={css.editInput}
                    value={editingName}
                    autoFocus
                    aria-label={t("edit.placeholder")}
                    onChange={(event) => setEditingName(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void saveEdit(project);
                      if (event.key === "Escape") setEditingId(null);
                    }}
                  />
                  <div className={css.editActions}>
                    <Button
                      type="button"
                      size="sm"
                      variant="primary"
                      onClick={() => void saveEdit(project)}
                    >
                      {t("edit.save")}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => setEditingId(null)}
                    >
                      {t("edit.cancel")}
                    </Button>
                  </div>
                </div>
              ) : (
                <>
                  <div className={css.rowText}>
                    <span className={css.name}>{project.name}</span>
                    <span className={css.path} title={project.path}>
                      {project.path}
                    </span>
                    {project.pinned ||
                    project.archived_chats ||
                    project.unread_chats > 0 ||
                    project.read_at !== null ? (
                      <span className={css.badges}>
                        {project.pinned ? (
                          <span className={css.badge}>{t("badge.pinned")}</span>
                        ) : null}
                        {project.unread_chats > 0 ? (
                          <span className={css.badge}>
                            {t("badge.unread")} {project.unread_chats}
                          </span>
                        ) : null}
                        {project.read_at !== null ? (
                          <span className={css.badge}>{t("badge.read")}</span>
                        ) : null}
                        {project.archived_chats ? (
                          <span className={css.badge}>{t("badge.archived")}</span>
                        ) : null}
                      </span>
                    ) : null}
                  </div>
                  <div className={css.rowActions}>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      aria-label={t("menu.label")}
                      icon={<IconEllipsisOutline16 />}
                      onClick={(event) =>
                        openMenuFrom(project, event.currentTarget.getBoundingClientRect())
                      }
                    />
                  </div>
                </>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <p className={css.empty}>{loading ? t("status.loading") : t("list.empty")}</p>
      )}

      {menuState ? (
        <Menu
          open
          portal
          getAnchorRect={() => menuState.rect}
          anchor={<span className={css.menuAnchor} aria-hidden="true" />}
          items={buildMenuItems(menuState.project, t)}
          onSelect={(id) => void runAction(menuState.project, id)}
          onClose={() => setMenuState(null)}
        />
      ) : null}

      <Modal
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={t("remove.title")}
        closeLabel={t("edit.cancel")}
        description={t("remove.desc")}
        footer={
          <>
            <Button type="button" variant="ghost" onClick={() => setRemoving(null)}>
              {t("edit.cancel")}
            </Button>
            <Button
              type="button"
              variant="primary"
              icon={<IconTrashOutline16 />}
              onClick={() => void confirmRemove()}
            >
              {t("remove.confirm")}
            </Button>
          </>
        }
      >
        <div className={css.modalBody}>
          {removing ? <span className={css.modalName}>{removing.name}</span> : null}
        </div>
      </Modal>

      {error ? <p className={css.messageError}>{error}</p> : null}
      {notice ? <p className={css.messageInfo}>{notice}</p> : null}
    </div>
  );
}

/** client 面所需服务的本地结构类型（slots / locale 由 dsh 生态注入）。 */
interface ClientContextLike {
  effect(effect: () => unknown, label?: string): void;
  locale: {
    bind(ns: string): Translate;
    register(ns: string, locale: string, dict: Record<string, string>): unknown;
  };
  slots: {
    inject(key: string, callback: () => unknown): unknown;
    register(options: unknown, component: unknown): unknown;
  };
}

/** 所需服务（cordis fiber inject）；client 面入口只注册设置节。 */
export const inject = ["slots", "locale"];

/** 设置节入口：把“项目”注册到 dsh WebUI 设置面板。 */
export function apply(ctx: ClientContextLike): void {
  const NS = "settings.projects";
  const t = ctx.locale.bind(NS);

  ctx.effect(
    () =>
      ctx.locale.register(NS, "zh", {
        nav: "项目",
        "nav.desc": "管理本地项目与对应的会话聊天",
        "add.action": "添加项目",
        "add.title": "选择项目目录",
        "list.empty": "还没有项目，先添加本地目录",
        "status.loading": "正在读取…",
        "menu.label": "项目菜单",
        "menu.pin": "置顶项目",
        "menu.unpin": "取消置顶",
        "menu.finder": "在 Finder 中显示",
        "menu.worktree": "创建永久工作树",
        "menu.edit": "编辑项目",
        "menu.read": "全部标为已读",
        "menu.archive": "归档聊天",
        "menu.unarchive": "恢复聊天",
        "menu.remove": "移除本地项目",
        "edit.save": "保存",
        "edit.cancel": "取消",
        "edit.placeholder": "项目名称",
        "remove.title": "移除本地项目",
        "remove.desc": "仅从列表移除，不会删除磁盘上的项目目录。",
        "remove.confirm": "移除",
        "badge.pinned": "置顶",
        "badge.archived": "聊天已归档",
        "badge.read": "已读",
        "badge.unread": "未读",
        added: "已添加项目",
        removed: "已移除项目",
        saved: "已保存",
        "read.done": "已全部标为已读",
        "archived.done": "聊天已归档",
        "unarchived.done": "聊天已恢复",
        "worktree.created": "已创建永久工作树",
        "error.bridge": "桌面壳桥接不可用",
        "error.read": "读取项目失败",
        "error.action": "操作失败",
      }),
    "projects: 项目设置中文字典",
  );
  ctx.effect(
    () =>
      ctx.locale.register(NS, "en", {
        nav: "Projects",
        "nav.desc": "Manage local projects and their chat sessions",
        "add.action": "Add project",
        "add.title": "Choose project directory",
        "list.empty": "No projects yet. Add a local directory.",
        "status.loading": "Loading…",
        "menu.label": "Project menu",
        "menu.pin": "Pin project",
        "menu.unpin": "Unpin project",
        "menu.finder": "Show in Finder",
        "menu.worktree": "Create permanent worktree",
        "menu.edit": "Edit project",
        "menu.read": "Mark all as read",
        "menu.archive": "Archive chats",
        "menu.unarchive": "Restore chats",
        "menu.remove": "Remove local project",
        "edit.save": "Save",
        "edit.cancel": "Cancel",
        "edit.placeholder": "Project name",
        "remove.title": "Remove local project",
        "remove.desc": "Removes the entry only; files on disk are kept.",
        "remove.confirm": "Remove",
        "badge.pinned": "Pinned",
        "badge.archived": "Chats archived",
        "badge.read": "Read",
        "badge.unread": "Unread",
        added: "Project added",
        removed: "Project removed",
        saved: "Saved",
        "read.done": "Marked all as read",
        "archived.done": "Chats archived",
        "unarchived.done": "Chats restored",
        "worktree.created": "Permanent worktree created",
        "error.bridge": "Desktop bridge unavailable",
        "error.read": "Failed to read projects",
        "error.action": "Action failed",
      }),
    "projects: English dictionary",
  );

  ctx.slots.inject("settings.section", () =>
    ctx.slots.register(
      {
        name: "settings.section",
        id: "projects",
        order: 70,
        label: () => t("nav"),
        locale: NS,
        children: {},
      },
      () => (
        <SettingsPage>
          <SettingsSection
            headingId="projects-heading"
            title={t("nav")}
            description={t("nav.desc")}
          >
            <ProjectsPanel t={t} />
          </SettingsSection>
        </SettingsPage>
      ),
    ),
  );
}
