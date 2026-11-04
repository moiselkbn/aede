// The one pill of the notch, shown by the indicator page: whatever the app has to say (a
// dictation, a proposal of the learning shortcut, its result) is drawn in the same pill, which
// changes its content and its size in place instead of one pill going in and another coming out.
// It is only a picture: the window lets the clicks through.
//
// The pill comes out of the notch on an "ease out back" curve and goes back into it on a curve
// that grows it first (see motion.ts). The motion is not computed at every frame: it is
// simulated at once, written as a list of positions (240 per second) and handed to the browser
// as a Web Animation on `transform` and `opacity`. Those are played by Core Animation, at the
// refresh rate of the screen (120 Hz on a ProMotion Mac), while an animation run by JavaScript
// in `requestAnimationFrame` is stuck at 60 Hz.
//
// The box has the size of its content and is only scaled, from the size of the notch to its own
// size. The content is scaled the opposite way so that it keeps its natural size, and the box
// clips it. When the content changes, a new animation starts from where the pill is, with the
// new size, while the new content fades in.

import { CSSProperties, ReactNode, useLayoutEffect, useRef, useState } from "react";
import {
  CLOSE_SECONDS,
  OPEN_SECONDS,
  easeCloseBack,
  easeOutBack,
} from "./motion";
import { DEFAULT_PILL_STYLE, type PillStyle } from "./pill-style";
import "./pill.css";

// The size of the notch (width 0 without a notch, in the menu bar).
export type Notch = { width: number; height: number };

type Size = { width: number; height: number };

// How fast the box slides to the size of a new content, in seconds.
const RESIZE = 0.08;

// Time between two positions of an animation, and the longest an animation can last.
const STEP = 1 / 240;
const STEP_MS = STEP * 1000;
const MAX_SECONDS = 2;

// The content is scaled by 1 / (the scale of the box), which is not defined when the box has
// no size. The content is invisible by then.
const MIN_SCALE = 0.02;

const mix = (from: number, to: number, t: number) => from + (to - from) * t;
const clamp = (value: number) => Math.min(1, Math.max(0, value));

// Where the pill is: `open` (0 = in the notch, 1 = out), and the size of the content as the box
// follows it.
type Pose = { open: number; width: number; height: number };

// Plays the opening (goal 1, an "ease out back" curve) or the closing (goal 0, a curve that
// grows the pill a little, then slows down into the notch) from `from` until it stops, one pose
// per STEP. The box follows `target`, the size of the content, which may have changed since
// `from`. `overshoot` is how far the opening goes past the size (see motion.ts).
function simulate(from: Pose, goal: number, target: Size, overshoot: number): Pose[] {
  const poses = [from];
  let { width, height } = from;
  const follow = 1 - Math.exp(-STEP / RESIZE);
  for (let i = 1; i <= MAX_SECONDS / STEP; i++) {
    let open: number;
    let arrived: boolean;
    if (goal === 1) {
      const t = Math.min(1, (i * STEP) / OPEN_SECONDS);
      open = from.open + (1 - from.open) * easeOutBack(t, overshoot);
      arrived = t === 1;
    } else {
      const t = Math.min(1, (i * STEP) / CLOSE_SECONDS);
      open = from.open * (1 - easeCloseBack(t));
      arrived = t === 1;
    }
    width += (target.width - width) * follow;
    height += (target.height - height) * follow;
    poses.push({ open, width, height });
    if (arrived && Math.abs(width - target.width) < 0.05 && Math.abs(height - target.height) < 0.05) {
      break;
    }
  }
  return poses;
}

// What each part of the pill looks like for one pose. `layout` is the real size of the content.
// Width and height go a little past their size while the pill comes out (the window has room
// for it, see BELOW_ROOM in indicator.rs).
function frameOf(pose: Pose, notch: Notch, layout: Size) {
  const width = mix(notch.width, pose.width, Math.max(0, pose.open));
  const height = mix(notch.height, pose.height, Math.max(0, pose.open));
  const scaleX = width / layout.width;
  const scaleY = height / layout.height;
  // How far each flare is from where it is when the box has its real size.
  const edge = (layout.width - width) / 2;
  return {
    body: `scale(${scaleX}, ${scaleY})`,
    counter: `scale(${1 / Math.max(scaleX, MIN_SCALE)}, ${1 / Math.max(scaleY, MIN_SCALE)})`,
    opacity: clamp((pose.open - 0.3) / 0.5),
    left: `translateX(${edge}px)`,
    right: `translateX(${-edge}px)`,
  };
}

type PillProps<V> = {
  // What the pill shows; null when it goes back into the notch. A new object means a new
  // content: the pill follows its size.
  view: V | null;
  notch: Notch;
  // How far the pill goes past its size when it comes out to show `view`.
  overshootFor: (view: V) => number;
  // The content of a view, and what sticks to the bottom edge of the box.
  render: (view: V) => ReactNode;
  footer?: (view: V) => ReactNode;
  // Changes when the content changes size without being a new view (the dictation, from the wave
  // to the status line): the pill measures it again and follows.
  sizeKey?: string;
  // The look of the pill (pill-style.ts is the one of the notch; the first launch draws it on a
  // light card, where it has to be plain black).
  look?: PillStyle;
};

export default function Pill<V>({
  view,
  notch,
  overshootFor,
  render,
  footer,
  sizeKey,
  look = DEFAULT_PILL_STYLE,
}: PillProps<V>) {
  // What is drawn: kept while the pill goes back in, after `view` is null.
  const [shown, setShown] = useState<V | null>(view);
  if (view && view !== shown) setShown(view);
  const bodyRef = useRef<HTMLDivElement>(null);
  const counterRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const leftRef = useRef<HTMLDivElement>(null);
  const rightRef = useRef<HTMLDivElement>(null);
  // The poses the running animations play, to know where the pill is when a new one starts.
  const motion = useRef({ poses: null as Pose[] | null, animations: [] as Animation[], token: 0 });
  const notchRef = useRef(notch);
  notchRef.current = notch;

  // Starts the motion toward `goal` (1 = out, 0 = back in the notch), from where the pill is.
  const moveTo = (goal: number, overshoot: number) => {
    const state = motion.current;
    const content = contentRef.current;
    const elements = [bodyRef, counterRef, leftRef, rightRef].map((ref) => ref.current);
    if (!content || elements.some((element) => !element)) return;
    const layout = { width: content.offsetWidth, height: content.offsetHeight };
    const notch = notchRef.current;

    let from: Pose = { open: 0, width: layout.width, height: layout.height };
    const running = state.animations[0];
    if (state.poses && running) {
      const index = Math.round(Number(running.currentTime ?? 0) / STEP_MS);
      from = state.poses[Math.min(state.poses.length - 1, Math.max(0, index))];
    }
    const poses = simulate(from, goal, layout, overshoot);
    const frames = poses.map((pose) => frameOf(pose, notch, layout));
    const timing = {
      duration: (poses.length - 1) * STEP_MS,
      easing: "linear",
      fill: "forwards",
    } as const;

    state.animations.forEach((animation) => animation.cancel());
    const [body, counter, left, right] = elements as HTMLElement[];
    // The first position of the animation, as plain styles: the browser starts a new animation
    // on the compositor a moment later (the first time, it has to create the layers), and until
    // then the pill would show at its full size, with its content, for a few frames.
    body.style.transform = frames[0].body;
    counter.style.transform = frames[0].counter;
    counter.style.opacity = String(frames[0].opacity);
    left.style.transform = frames[0].left;
    right.style.transform = frames[0].right;
    state.animations = [
      body.animate(frames.map((f) => ({ transform: f.body })), timing),
      counter.animate(frames.map((f) => ({ transform: f.counter, opacity: f.opacity })), timing),
      left.animate(frames.map((f) => ({ transform: f.left })), timing),
      right.animate(frames.map((f) => ({ transform: f.right })), timing),
    ];
    state.poses = poses;

    // Back in the notch: the pill is removed. A newer animation (cancelled or replaced this
    // one) means the pill came out again.
    const token = ++state.token;
    if (goal === 0) {
      state.animations[0].finished.then(
        () => {
          if (motion.current.token !== token) return;
          motion.current.poses = null;
          motion.current.animations = [];
          setShown(null);
        },
        () => {},
      );
    }
  };

  // Before the first paint, so that the pill never shows at its full size for one frame.
  useLayoutEffect(() => {
    if (shown) moveTo(view ? 1 : 0, overshootFor(shown));
    // `moveTo` only reads refs.
  }, [shown, view, sizeKey]);

  if (!shown) return null;
  const bodyStyle = { "--notch": `${notch.height}px` } as CSSProperties;
  return (
    // While the pill goes back in, the bar stops: the time is over.
    <div className={view ? "pill" : "pill pill-closing"}>
      <div className="pill-flare pill-flare-left" ref={leftRef} />
      <div className="pill-flare pill-flare-right" ref={rightRef} />
      <div className={`pill-body pill-style-${look}`} ref={bodyRef} style={bodyStyle}>
        <div className="pill-counter" ref={counterRef}>
          <div ref={contentRef}>{render(shown)}</div>
        </div>
        {footer?.(shown)}
      </div>
    </div>
  );
}
