import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { Check, CircleX, Download, TriangleAlert, X } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { motionTokens } from "@/components/arc/motion-tokens";

export type ToastData = {
  key: string;
  tone: "progress" | "success" | "warning" | "danger";
  title: string;
  text?: string;
  /** 0 to 1: draws the progress bar under the text. */
  progress?: number;
  action?: { label: string; run: () => void };
  /** Without it the toast has no close button (the download one: it goes away when it ends). */
  onClose?: () => void;
};

const ICONS: Record<ToastData["tone"], ReactNode> = {
  progress: <Download size={16} strokeWidth={1.75} aria-hidden="true" />,
  success: <Check size={16} strokeWidth={1.75} aria-hidden="true" />,
  warning: <TriangleAlert size={16} strokeWidth={1.75} aria-hidden="true" />,
  danger: <CircleX size={16} strokeWidth={1.75} aria-hidden="true" />,
};

/** Toast Stack of Arc UI, as the Figma describes it: the newest toast is in front, the older ones tuck
 *  behind it (14 px visible, 0.05 less scale per rank, 3 at most) and the stack fans out on hover. */
const PEEK = 14;
const SCALE_STEP = 0.05;
const VISIBLE = 3;
const GAP = 8;
const FALLBACK_HEIGHT = 96;

function ToastCard({
  toast,
  onHeight,
}: {
  toast: ToastData;
  onHeight: (key: string, height: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const report = () => onHeight(toast.key, el.offsetHeight);
    report();
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [toast.key, onHeight]);

  return (
    <div className="toast" data-tone={toast.tone} ref={ref}>
      <span className="toast-icon">{ICONS[toast.tone]}</span>
      <div className="toast-body">
        <p className="toast-title">{toast.title}</p>
        {toast.text && <p className="toast-text">{toast.text}</p>}
        {toast.progress !== undefined && (
          <span className="toast-bar">
            <span style={{ transform: `scaleX(${Math.min(1, Math.max(0, toast.progress))})` }} />
          </span>
        )}
        {toast.action && (
          <div className="toast-action">
            <Button variant="secondary" size="sm" className="setup-action" onClick={toast.action.run}>
              {toast.action.label}
            </Button>
          </div>
        )}
      </div>
      {toast.onClose && (
        <button type="button" className="toast-close" aria-label="Fermer" onClick={toast.onClose}>
          <X size={14} strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

/** The notices at the bottom of the right panel. With `sunk`, the download notice slides down until
 *  only its top shows (down to the numbers), for the screens whose cards take the whole panel. It comes
 *  back up under the mouse. */
function ToastStack({ toasts, sunk = false }: { toasts: ToastData[]; sunk?: boolean }) {
  const [heights, setHeights] = useState<Record<string, number>>({});
  const [fanned, setFanned] = useState(false);
  const reduced = useReducedMotion();

  const onHeight = (key: string, height: number) =>
    setHeights((prev) => (prev[key] === height ? prev : { ...prev, [key]: height }));
  const heightOf = (key: string) => heights[key] ?? FALLBACK_HEIGHT;

  // Front first: rank 0 is the newest toast.
  const ranked = [...toasts].reverse();
  const front = ranked[0];
  const frontHeight = front ? heightOf(front.key) : 0;

  // Distance from the bottom edge of the stack to the bottom of the toast of each rank.
  // Folded, a toast behind is cut to the height of the front one, so only its top edge shows.
  let spread = 0;
  const lift = ranked.map((toast, rank) => {
    const scale = 1 - SCALE_STEP * rank;
    const collapsed = rank === 0 ? 0 : frontHeight * (1 - scale) + PEEK * rank;
    const expanded = spread;
    spread += heightOf(toast.key) + GAP;
    return { scale, up: fanned ? expanded : collapsed };
  });

  const spring = reduced ? { duration: 0.01 } : motionTokens.spring.snappy;
  const fan = reduced ? { duration: 0.01 } : motionTokens.spring.morph;

  return (
    <div
      className="toast-stack"
      data-sinkable={(sunk && front?.tone === "progress") || undefined}
      role="status"
      style={{ height: frontHeight }}
      onMouseEnter={() => setFanned(true)}
      onMouseLeave={() => setFanned(false)}
      onFocus={() => setFanned(true)}
      onBlur={() => setFanned(false)}
    >
      {/* What moves down is this box, not the stack: the stack keeps the place the pointer is looked for in. */}
      <div className="toast-sink" data-sunk={(sunk && !fanned && front?.tone === "progress") || undefined}>
      <AnimatePresence initial={false}>
        {ranked.map((toast, rank) => (
          <motion.div
            key={toast.key}
            className="toast-slot"
            style={{
              zIndex: toasts.length - rank,
              height: rank > 0 && !fanned ? frontHeight : undefined,
              overflow: rank > 0 && !fanned ? "hidden" : undefined,
              borderRadius: "var(--space-6)",
            }}
            // A new toast rises from its own height below the edge, fading in.
            initial={{ opacity: 0, y: heightOf(toast.key), scale: 1 }}
            animate={{
              opacity: rank < VISIBLE ? 1 : 0,
              y: -lift[rank].up,
              scale: fanned ? 1 : lift[rank].scale,
            }}
            exit={{
              opacity: 0,
              transition: { duration: reduced ? 0.01 : motionTokens.duration.exit },
            }}
            transition={{
              y: fanned ? fan : spring,
              scale: fanned ? fan : spring,
              opacity: { duration: reduced ? 0.01 : motionTokens.duration.fast, ease: [...motionTokens.ease.enter] },
            }}
          >
            <ToastCard toast={toast} onHeight={onHeight} />
          </motion.div>
        ))}
      </AnimatePresence>
      </div>
    </div>
  );
}

export default ToastStack;
