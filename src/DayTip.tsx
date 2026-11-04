import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { animate, motion, useMotionValue, useReducedMotion } from "motion/react";
import { motionTokens } from "@/components/arc/lib/motion-tokens";

/** The day hovered or focused, and the element the bubble points at. */
type Tip = { index: number; anchor: Element };

/**
 * The state of the bubble of a chart of days (as in the Arc "Activity Heatmap"): which day it shows, whether it is open,
 * and where it sits. A day is any element with a `data-index`; `anchorOf` can point the bubble at another element than
 * the one hovered (the top of a bar, for a hit area as tall as the chart).
 */
export function useDayTip(anchorOf?: (day: Element) => Element | null) {
  const rootRef = useRef<HTMLDivElement>(null);
  const reduced = !!useReducedMotion();
  const [tip, setTip] = useState<Tip | null>(null);
  const [open, setOpen] = useState(false);
  const wasOpen = useRef(false);
  const hideTimer = useRef(0);
  const x = useMotionValue(0);
  const y = useMotionValue(0);

  function show(target: Element | null) {
    const day = target?.closest("[data-index]");
    const anchor = day && (anchorOf ? anchorOf(day) : day);
    if (!day || !anchor) return;
    window.clearTimeout(hideTimer.current);
    setTip({ index: Number(day.getAttribute("data-index")), anchor });
    setOpen(true);
  }

  function hideSoon() {
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setOpen(false), 110);
  }

  useEffect(() => () => window.clearTimeout(hideTimer.current), []);

  // The bubble jumps to its day when it opens, then glides on a spring from day to day.
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!tip || !root) return;
    const box = root.getBoundingClientRect();
    const target = tip.anchor.getBoundingClientRect();
    const nextX = target.left - box.left + target.width / 2;
    const nextY = target.top - box.top;
    if (open && wasOpen.current && !reduced) {
      animate(x, nextX, motionTokens.spring.snappy);
      animate(y, nextY, motionTokens.spring.snappy);
    } else {
      x.jump(nextX);
      y.jump(nextY);
    }
    wasOpen.current = open;
  }, [tip, open, reduced, x, y]);

  return { rootRef, tip, open, reduced, x, y, show, hideSoon, hide: () => setOpen(false) };
}

type Props = {
  state: ReturnType<typeof useDayTip>;
  primary: string;
  secondary: string;
};

/** The bubble itself: a count on top, a date below, above the day it points at. */
function DayTip({ state, primary, secondary }: Props) {
  const { open, tip, reduced, x, y } = state;
  return (
    <motion.div className="day-tip" style={{ x, y }} aria-hidden="true">
      <motion.div
        className="day-tip-bubble"
        style={{ x: "-50%" }}
        initial={false}
        animate={open && tip ? { opacity: 1, scale: 1 } : { opacity: 0, scale: reduced ? 1 : 0.96 }}
        transition={
          reduced
            ? { duration: motionTokens.duration.fast }
            : open
              ? motionTokens.spring.snappy
              : { duration: motionTokens.duration.instant }
        }
      >
        <span className="day-tip-primary">{primary}</span>
        <span className="day-tip-secondary">{secondary}</span>
      </motion.div>
    </motion.div>
  );
}

export default DayTip;
