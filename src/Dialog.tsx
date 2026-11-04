import { useEffect } from "react";
import type { ReactNode } from "react";
import { X } from "lucide-react";

type Props = {
  title: string;
  description?: string;
  /** Shown before the title (the warning triangle of the permission dialogs). */
  icon?: ReactNode;
  onClose: () => void;
  /** The body, then the buttons on the right. */
  children: ReactNode;
  actions: ReactNode;
  width?: number;
};

/** Figma component "Fenêtre de dialogue": header with a close button, content, actions. Escape closes it. */
function Dialog({ title, description, icon, onClose, children, actions, width = 440 }: Props) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    // Dragging the backdrop moves the window, as it did before the dialog opened.
    <div className="dialog-backdrop" data-tauri-drag-region>
      <div
        className="dialog"
        style={{ width }}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
      >
        <div className="dialog-header">
          <div className="dialog-titles">
            <div className="dialog-heading">
              {icon}
              <h2 className="dialog-title" id="dialog-title">
                {title}
              </h2>
            </div>
            {description && <p className="dialog-description">{description}</p>}
          </div>
          <button type="button" className="dialog-close" aria-label="Fermer" onClick={onClose}>
            <X size={20} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
        {children && <div className="dialog-content">{children}</div>}
        <div className="dialog-actions">{actions}</div>
      </div>
    </div>
  );
}

export default Dialog;
