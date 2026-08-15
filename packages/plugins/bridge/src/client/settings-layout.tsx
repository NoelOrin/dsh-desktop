/** 所有插件设置节共用的页面布局与区块结构（与上游 ui-theme 对齐）。 */
import type { ReactNode } from "react";
import css from "./settings-layout.module.css";

export function SettingsPage({ children }: { children: ReactNode }): JSX.Element {
  return <div className={css.page}>{children}</div>;
}

export function SettingsSection({
  headingId,
  title,
  description,
  actions,
  children,
}: {
  headingId: string;
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
}): JSX.Element {
  return (
    <section className={css.section} aria-labelledby={headingId}>
      <div className={css.header}>
        <div className={css.headerText}>
          <h2 id={headingId} className={css.heading}>
            {title}
          </h2>
          {description ? <p className={css.hint}>{description}</p> : null}
        </div>
        {actions ? <div className={css.headerActions}>{actions}</div> : null}
      </div>
      <div className={css.body}>{children}</div>
    </section>
  );
}
