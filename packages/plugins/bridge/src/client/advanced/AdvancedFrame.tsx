import type { ReactNode } from "react";
import {
  computeDesktopColumns,
  type AdvancedLayoutState,
} from "./layout-state";
import css from "./advanced.module.css";

export interface AdvancedFrameProps {
  layout: AdvancedLayoutState;
  sidebar: ReactNode;
  conversation: ReactNode;
  details: ReactNode;
  overlay: ReactNode;
}

/** advanced 模式的三栏根框架，保留官方 sidebar/conversation/details 内容。 */
export function AdvancedFrame({
  layout,
  sidebar,
  conversation,
  details,
  overlay,
}: AdvancedFrameProps): JSX.Element {
  const columns = computeDesktopColumns(layout, { width: window.innerWidth });
  return (
    <div
      className={css.frame}
      style={{
        gridTemplateColumns: `${columns.sidebar}px minmax(0, 1fr) ${columns.details}px`,
      }}
    >
      <div className={css.column}>{sidebar}</div>
      <div className={css.column}>{conversation}</div>
      <div className={css.column}>{details}</div>
      {overlay ? <div className={css.overlay}>{overlay}</div> : null}
    </div>
  );
}
