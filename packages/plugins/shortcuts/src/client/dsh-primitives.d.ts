/** dsh 平台模块的最小类型面（运行时由 dsh web 提供，不随插件打包）。 */
declare module "@deepseek-ai/dsh-client-ui-primitives" {
  import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, ReactPortal } from "react";

  export function Button(
    props: ButtonHTMLAttributes<HTMLButtonElement> & {
      variant?: "primary" | "outline" | "ghost";
      size?: "md" | "sm";
      icon?: ReactNode;
    },
  ): JSX.Element;

  export function Input(props: InputHTMLAttributes<HTMLInputElement>): JSX.Element;

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

  export function IconPlusOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconNewChatOutline16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconPanelLeftOutline16(props?: {
    size?: number;
    className?: string;
  }): JSX.Element;

  export function IconStopFill16(props?: { size?: number; className?: string }): JSX.Element;

  export function IconTrashOutline16(props?: { size?: number; className?: string }): JSX.Element;
}
