/** projects 插件使用的桥接与项目本地类型切片。 */
import { getBridge as readBridge } from "../../../client-kit/inject";

export interface ProjectEntry {
  id: string;
  name: string;
  path: string;
  pinned: boolean;
  archived_chats: boolean;
  unread_chats: number;
  read_at: number | null;
  created_at: number;
  updated_at: number;
}

export interface ProjectWorktreeResult {
  entry: ProjectEntry;
  target: string;
  branch: string;
}

export interface BridgeLike {
  dialog: {
    openFile(options?: {
      title?: string;
      multiple?: boolean;
      directory?: boolean;
      defaultPath?: string;
    }): Promise<string | string[] | null>;
  };
  projects: {
    list(): Promise<ProjectEntry[]>;
    add(path: string): Promise<ProjectEntry>;
    update(project: ProjectEntry): Promise<ProjectEntry>;
    remove(id: string): Promise<void>;
    setPinned(id: string, pinned: boolean): Promise<ProjectEntry>;
    markRead(id: string): Promise<ProjectEntry>;
    setArchived(id: string, archived: boolean): Promise<ProjectEntry>;
    createWorktree(id: string): Promise<ProjectWorktreeResult>;
    showInFinder(id: string): Promise<void>;
  };
}

/** 翻译函数形状（ctx.locale.bind 的返回）。 */
export type Translate = (key: string) => string;

/** 读取壳注入的桥接对象；未注入（如纯浏览器）时返回 null。 */
export const getBridge = () => readBridge<BridgeLike>();
