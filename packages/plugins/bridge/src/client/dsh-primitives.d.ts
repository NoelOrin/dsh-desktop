/** dsh 平台模块的最小类型面（运行时由 dsh web 提供，不随插件打包）。 */
declare module "@deepseek-ai/dsh-client-ui-primitives" {
  import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";

  export function Button(
    props: ButtonHTMLAttributes<HTMLButtonElement> & {
      variant?: "primary" | "outline" | "ghost";
      size?: "md" | "sm";
      icon?: ReactNode;
    },
  ): JSX.Element;

  export function Input(props: InputHTMLAttributes<HTMLInputElement>): JSX.Element;

  export interface MenuItem {
    id: string;
    label: ReactNode;
    disabled?: boolean | undefined;
    icon?: ReactNode;
    danger?: boolean | undefined;
    submenu?: readonly MenuItem[] | undefined;
  }

  export type MenuEntry =
    | MenuItem
    | { type: "separator"; id: string }
    | { type: "label"; id: string; text: string };

  export function Menu(props: {
    open: boolean;
    anchor: ReactNode;
    items: readonly MenuEntry[];
    selectedId?: string | undefined;
    selectedIds?: readonly string[] | undefined;
    onSelect: (id: string) => void;
    onClose: () => void;
    align?: "start" | "end";
    side?: "bottom" | "top" | "right";
    portal?: boolean;
    closeOnPointerLeave?: boolean;
    dense?: boolean;
    compact?: boolean;
    getAnchorRect?: () => DOMRect | null;
    footer?: readonly MenuEntry[];
    className?: string;
  }): JSX.Element;

  export type StateDotState = "done" | "warning" | "ongoing" | "error";

  export function StateDot(props: {
    state: StateDotState;
    size?: number | undefined;
    className?: string | undefined;
  }): JSX.Element;

  export function IconPlusOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconCheckOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconFolderOpenOutline16(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;

  export function IconEditOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconCopyOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconDownloadOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconGlobeOutline14(props?: { size?: number; className?: string }): JSX.Element;
  export function IconChevronDownOutline14(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;
  export function IconPlayOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconStopFill16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconRefreshOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconTrashOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconCordisPluginOutline14(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;

  export function IconLightOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconLinkOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconDarkOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconFollowsystemOutline16(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;
}
