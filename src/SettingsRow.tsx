import type { ReactNode } from "react";

/** A block of the settings page (Figma "Sous-titre" and "Groupe"): an optional title, then rows sharing one bordered box. */
export function SettingsSection({
  title,
  className,
  children,
}: {
  title?: string;
  /** Added to the group, to style the rows of one page. */
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-section">
      {title && <h2 className="section-title">{title}</h2>}
      <div className={className ? `settings-group ${className}` : "settings-group"}>{children}</div>
    </section>
  );
}

type Props = {
  title: string;
  detail?: ReactNode;
  error?: string | null;
  children?: ReactNode;
};

/** One setting (Figma "Ligne de liste"): its text on the left, its control on the right. */
function SettingsRow({ title, detail, error, children }: Props) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <p className="settings-row-title">{title}</p>
        {detail && <p className="settings-row-detail">{detail}</p>}
        {error && <p className="settings-row-error">{error}</p>}
      </div>
      {children && <div className="settings-row-control">{children}</div>}
    </div>
  );
}

export default SettingsRow;
