import type { MouseEvent } from "react";
import type { LucideIcon } from "lucide-react";

type Props = {
  label: string;
  icon: LucideIcon;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
};

/** Figma "Bouton icône" (xs): 24 px, a 14 px icon. */
function IconButton({ label, icon: Icon, onClick, disabled }: Props) {
  return (
    <button
      type="button"
      className="icon-button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
    >
      <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

export default IconButton;
