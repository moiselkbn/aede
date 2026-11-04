import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { LucideIcon } from "lucide-react";

export type MenuItem = {
  label: string;
  icon: LucideIcon;
  onSelect: () => void;
  /** An action that destroys something: it turns red under the pointer. */
  danger?: boolean;
};

type Props = {
  /** Where the button that opened the menu is: the menu opens under it, right edges aligned. */
  anchor: DOMRect;
  items: MenuItem[];
  onClose: () => void;
};

/** Figma component "Menu": a small list of actions. Closes on a click outside, Escape, or a choice. */
function Menu({ anchor, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState<{ top: number; right: number }>({
    top: anchor.bottom + 4,
    right: window.innerWidth - anchor.right,
  });

  // Opens above the button when there is no room below it.
  useLayoutEffect(() => {
    const height = ref.current?.offsetHeight ?? 0;
    if (anchor.bottom + 4 + height > window.innerHeight) {
      setPosition((p) => ({ ...p, top: Math.max(4, anchor.top - 4 - height) }));
    }
  }, [anchor]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return (
    <div className="menu" role="menu" ref={ref} style={position}>
      {items.map(({ label, icon: Icon, onSelect, danger }) => (
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          data-tone={danger ? "danger" : undefined}
          key={label}
          onClick={() => {
            onClose();
            onSelect();
          }}
        >
          <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
          {label}
        </button>
      ))}
    </div>
  );
}

export default Menu;
