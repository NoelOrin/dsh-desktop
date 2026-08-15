/** dsh 平台模块的最小类型面（运行时由 dsh web 提供，不随插件打包）。 */
declare module "@deepseek-ai/dsh-client-ui-primitives" {
  import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, ReactPortal } from "react";

  export type ButtonVariant = "primary" | "ghost" | "outline" | "toolbar";

  export function Button(
    props: ButtonHTMLAttributes<HTMLButtonElement> & {
      variant?: ButtonVariant;
      size?: "md" | "sm";
      icon?: ReactNode;
    },
  ): JSX.Element;

  export function Input(
    props: InputHTMLAttributes<HTMLInputElement> & { icon?: ReactNode },
  ): JSX.Element;

  export function Toast({
    text,
    icon,
    anchor,
    onDone,
  }: {
    text: string;
    icon?: ReactNode;
    anchor?: HTMLElement | null;
    onDone: () => void;
  }): ReactPortal;

  export interface MenuItem {
    id: string;
    label: ReactNode;
    disabled?: boolean;
    icon?: ReactNode;
    danger?: boolean;
    submenu?: readonly MenuItem[];
  }

  export interface MenuSeparator {
    type: "separator";
    id: string;
  }

  export interface MenuLabel {
    type: "label";
    id: string;
    text: string;
  }

  export type MenuEntry = MenuItem | MenuSeparator | MenuLabel;

  export function Menu({
    open,
    anchor,
    items,
    onSelect,
    onClose,
    align,
    side,
    portal,
    getAnchorRect,
    className,
  }: {
    open: boolean;
    anchor: ReactNode;
    items: readonly MenuEntry[];
    onSelect: (id: string) => void;
    onClose: () => void;
    align?: "start" | "end";
    side?: "bottom" | "top" | "right";
    portal?: boolean;
    getAnchorRect?: () => DOMRect | null;
    className?: string;
  }): JSX.Element;

  export function Modal({
    open,
    onClose,
    title,
    closeLabel,
    description,
    children,
    footer,
  }: {
    open: boolean;
    onClose: () => void;
    title: string;
    closeLabel?: string;
    description?: string;
    children?: ReactNode;
    footer?: ReactNode;
  }): ReactPortal | null;

  export function IconPlusOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconEllipsisOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconEditOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconTrashOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconBranchOutline16(props?: { size?: number; className?: string }): JSX.Element;
  export function IconFolderOpenOutline16(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;
  export function IconArchiveOutline20(props?: { size?: number; className?: string }): JSX.Element;
  export function IconCheckOutline16(props?: { size?: number; className?: string }): JSX.Element;
}
