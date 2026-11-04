import { CircleX, Info, TriangleAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/arc/button/button";

export type AlertTone = "info" | "warning" | "danger";
export type AlertItem = {
  key: string;
  tone: AlertTone;
  title: string;
  text?: string;
  action?: { label: string; run: () => void };
};

const ALERT_ICONS: Record<AlertTone, LucideIcon> = {
  info: Info,
  warning: TriangleAlert,
  danger: CircleX,
};

/** Figma component "Alerte": tone icon, title, detail, and an optional action button. */
function Alert({ tone, title, text, action }: Omit<AlertItem, "key">) {
  const Icon = ALERT_ICONS[tone];
  return (
    <div className="alert" data-tone={tone} role={tone === "info" ? "status" : "alert"}>
      <Icon className="alert-icon" size={18} strokeWidth={1.75} aria-hidden="true" />
      <div className="alert-text">
        <p className="alert-title">{title}</p>
        {text && <p className="alert-detail">{text}</p>}
      </div>
      {action && (
        <Button variant="secondary" size="sm" className="action-button" onClick={action.run}>
          {action.label}
        </Button>
      )}
    </div>
  );
}

export default Alert;
