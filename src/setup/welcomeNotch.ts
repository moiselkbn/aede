/** The notch of the welcome screen closes and opens like the one of the app (motion.ts): its body
 *  is scaled between the size of the pill and the size of the notch, the two flares slide along
 *  its edges and the wave fades. Played by the browser (Web Animations): only transforms and
 *  opacities move. The values come from the animation validated by the author (Figma screen 10). */
const NOTCH = { width: 185, height: 32 };
const PILL = { width: 234, height: 59 };

export type PillParts = {
  body: HTMLElement;
  left: HTMLElement;
  right: HTMLElement;
  wave: HTMLElement;
};

/** Closes (or opens) the pill. The returned promise ends with the animation, or when it is cancelled. */
export function movePill(parts: PillParts, close: boolean): Promise<void> {
  const { body, left, right, wave } = parts;
  const shut = `scale(${NOTCH.width / PILL.width}, ${NOTCH.height / PILL.height})`;
  const dx = (PILL.width - NOTCH.width) / 2;
  const ms = close ? 550 : 450;
  const easing = close ? "cubic-bezier(.5,-.8,.2,1)" : "cubic-bezier(.2,1.9,.4,1)";
  // Closed, the body ends on the still notch drawn under it: no fade, so no grey rim.
  const between = (open: Keyframe, closed: Keyframe) => (close ? [open, closed] : [closed, open]);
  if (!close) [body, left, right, wave].forEach((element) => element.getAnimations().forEach((a) => a.cancel()));
  const options = { duration: ms, easing, fill: close ? ("forwards" as const) : ("none" as const) };
  const animations = [
    body.animate(between({ transform: "none" }, { transform: shut }), options),
    left.animate(between({ transform: "none" }, { transform: `translateX(${dx}px)` }), options),
    right.animate(between({ transform: "none" }, { transform: `translateX(${-dx}px)` }), options),
    wave.animate(
      close
        ? [{ opacity: 1 }, { opacity: 0, offset: 0.4 }, { opacity: 0 }]
        : [{ opacity: 0 }, { opacity: 0, offset: 0.5 }, { opacity: 1 }],
      { duration: ms, easing: "linear", fill: options.fill },
    ),
  ];
  return Promise.all(animations.map((animation) => animation.finished)).then(
    () => undefined,
    () => undefined,
  );
}

/** Back to the open pill, whatever the animations left behind. */
export function resetPill(parts: PillParts) {
  Object.values(parts).forEach((element) => element.getAnimations().forEach((a) => a.cancel()));
}
