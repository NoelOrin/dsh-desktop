/** 所有插件设置节共用的页面/区块布局：固定标题、说明、操作和控件间距。 */
import type { ReactNode } from "react";
import css from "./settings-layout.module.css";

export function SettingsPage({ children }: { children: ReactNode }): JSX.Element {
  return <div className={css.page}>{children}</div>;
}

export function SettingsSection({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
}): JSX.Element {
  return (
    <section className={css.section} aria-label={title}>
      <header className={css.header}>
        <div className={css.headingGroup}>
          <h3 className={css.title}>{title}</h3>
          {description ? <p className={css.description}>{description}</p> : null}
        </div>
        {actions ? <div className={css.actions}>{actions}</div> : null}
      </header>
      <div className={css.body}>{children}</div>
    </section>
  );
}
