import { useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import DayTip, { useDayTip } from "./DayTip";
import { formatCount, formatDay, formatDecimal, formatInteger } from "./statsData";
import type { MonthActivity } from "./statsData";

// Sizes in the units of the drawing, which scales with the width of the card. The width is the one of the Figma (564);
// the height is lower than the Figma (190), at the request of the author.
const WIDTH = 564;
const HEIGHT = 130;
/** The plot itself: from the left edge to the axis, and from the top line to the base line. */
const PLOT_WIDTH = 532;
const TOP = 12;
const BASE = 104;
const AXIS_X = 542;
/** A bar takes this share of the room of its day. */
const BAR_SHARE = 0.58;
const GRID_LINES = 3;
/** Two labels closer than this (in drawing units) would overlap. */
const LABEL_HEIGHT = 14;

/** Steps that read well on an axis, from small to large. */
function niceSteps(): number[] {
  const steps: number[] = [];
  for (let power = 1; power <= 1_000_000; power *= 10) {
    for (const factor of [1, 2, 3, 5]) steps.push(factor * power);
  }
  return steps;
}

/** The step of the axis: three steps are enough to reach the busiest day (0, 3, 6, 9 for a best day of 8). */
export function axisStep(max: number): number {
  const wanted = Math.max(1, Math.ceil(max / GRID_LINES));
  return niceSteps().find((step) => step >= wanted) ?? wanted;
}

/** The days that carry a number under their bar: the first, then every fifth. */
function isLabelled(day: number): boolean {
  return day === 1 || day % 5 === 0;
}

/**
 * A bar per day of a month, with the average as a dashed line (Figma "Graphique en barres"). Hovering or focusing a day
 * opens a bubble with its count and its date, like the calendar of the year; the arrows move from day to day.
 */
function MonthBars({ activity }: { activity: MonthActivity }) {
  const { counts, elapsed, average, month, year } = activity;
  const step = axisStep(Math.max(0, ...counts));
  const top = step * GRID_LINES;
  const y = (value: number) => BASE - (value / top) * (BASE - TOP);
  const slot = PLOT_WIDTH / counts.length;
  const barWidth = slot * BAR_SHARE;
  const ticks = Array.from({ length: GRID_LINES + 1 }, (_, index) => index * step);
  const monthName = new Date(Date.UTC(year, month - 1, 1)).toLocaleDateString("fr-FR", {
    month: "long",
    timeZone: "UTC",
  });
  const dayDate = (index: number) => new Date(Date.UTC(year, month - 1, 1 + index));

  const svgRef = useRef<SVGSVGElement>(null);
  // The bubble points at the top of the bar, not at the hit area, which is as tall as the chart. It uses a marker that
  // is always at least a pixel high: a day without dictation has a bar of no height, which a browser does not place.
  const dayTip = useDayTip((day) =>
    svgRef.current?.querySelector(`.chart-anchor[data-anchor="${day.getAttribute("data-index")}"]`) ?? null,
  );
  // The one day that can be reached with the Tab key; the arrows move from there.
  const [focusIndex, setFocusIndex] = useState(Math.max(0, elapsed - 1));
  const tipIndex = dayTip.tip?.index ?? 0;

  function onKeyDown(event: KeyboardEvent<SVGSVGElement>) {
    const current = (event.target as Element).closest("[data-index]");
    if (!current) return;
    const index = Number(current.getAttribute("data-index"));
    const moves: Record<string, number> = {
      ArrowLeft: index - 1,
      ArrowRight: index + 1,
      Home: 0,
      End: elapsed - 1,
    };
    if (event.key === "Escape") {
      dayTip.hide();
    } else if (event.key in moves) {
      event.preventDefault();
      const next = Math.min(Math.max(moves[event.key], 0), elapsed - 1);
      svgRef.current?.querySelector<SVGElement>(`.chart-hit[data-index="${next}"]`)?.focus();
    }
  }

  return (
    <div className="month-chart" ref={dayTip.rootRef}>
      <svg
        ref={svgRef}
        className="month-bars"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="group"
        aria-label={`Dictées par jour, ${monthName} ${year}`}
        onPointerOver={(event) => dayTip.show(event.target as Element)}
        onPointerLeave={dayTip.hideSoon}
        onFocus={(event) => {
          setFocusIndex(Number((event.target as Element).getAttribute("data-index")));
          dayTip.show(event.target as Element);
        }}
        onBlur={dayTip.hideSoon}
        onKeyDown={onKeyDown}
      >
        {ticks.map((tick) => (
          <g key={tick}>
            <line className="chart-grid" x1={0} x2={PLOT_WIDTH} y1={y(tick)} y2={y(tick)} />
            {/* The label of the average wins when it would sit on top of this one. */}
            {!(elapsed > 0 && Math.abs(y(tick) - y(average)) < LABEL_HEIGHT) && (
              <text className="chart-label" x={AXIS_X} y={y(tick)} dominantBaseline="middle">
                {formatInteger(tick)}
              </text>
            )}
          </g>
        ))}
        {counts.slice(0, elapsed).map((count, index) => (
          <rect
            key={index}
            className="chart-bar"
            x={index * slot + (slot - barWidth) / 2}
            y={y(count)}
            width={barWidth}
            height={BASE - y(count)}
            rx={1}
          />
        ))}
        {counts.slice(0, elapsed).map((count, index) => (
          <rect
            key={index}
            className="chart-anchor"
            data-anchor={index}
            x={index * slot + (slot - barWidth) / 2}
            y={y(count)}
            width={barWidth}
            height={Math.max(1, BASE - y(count))}
          />
        ))}
        {elapsed > 0 && (
          <>
            <line className="chart-average" x1={0} x2={PLOT_WIDTH} y1={y(average)} y2={y(average)} />
            <text className="chart-label chart-label-strong" x={AXIS_X} y={y(average)} dominantBaseline="middle">
              {formatDecimal(average)}
            </text>
          </>
        )}
        {/* One hit area per day, as tall as the plot: a short bar is as easy to reach as a tall one. */}
        {counts.slice(0, elapsed).map((count, index) => (
          <rect
            key={index}
            className="chart-hit"
            data-index={index}
            tabIndex={index === focusIndex ? 0 : -1}
            aria-label={`${formatCount(count)}, ${formatDay(dayDate(index))}`}
            x={index * slot}
            y={0}
            width={slot}
            height={BASE}
          />
        ))}
        {counts.map((_, index) =>
          isLabelled(index + 1) ? (
            <text
              key={index}
              className="chart-label"
              x={index * slot + slot / 2}
              y={HEIGHT - 8}
              textAnchor="middle"
            >
              {index + 1}
            </text>
          ) : null,
        )}
      </svg>
      <DayTip state={dayTip} primary={formatCount(counts[tipIndex] ?? 0)} secondary={formatDay(dayDate(tipIndex))} />
    </div>
  );
}

export default MonthBars;
