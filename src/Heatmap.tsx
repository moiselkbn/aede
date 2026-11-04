import { useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import DayTip, { useDayTip } from "./DayTip";
import { formatCount, formatDay } from "./statsData";
import type { YearActivity } from "./statsData";

// Sizes in the units of the drawing, which scales down when the card is narrower. The 53 weeks of 365 days fit the
// card at 10 units a week (the Figma has cells of 10 every 12, which would need 636 units).
const CELL = 8;
const STEP = 10;
/** A month name is about this many weeks wide: two names closer than that would overlap. */
const MONTH_WEEKS = 4;
/** Room on the left for the names of the days, and above for the names of the months. */
const LEFT = 36;
const TOP = 20;
const LABEL_BASELINE = 11;

const MONTHS = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
/** The rows that carry a name: Monday, Wednesday, Friday. */
const WEEKDAYS: [number, string][] = [
  [0, "Lun"],
  [2, "Mer"],
  [4, "Ven"],
];

const DAY_MS = 86_400_000;

/** 0 for a day without dictation, then 1 to 4 in proportion to the busiest day. */
export function level(count: number, max: number): number {
  if (count <= 0 || max <= 0) return 0;
  return Math.min(4, Math.ceil((count * 4) / max));
}

/**
 * The calendar of the last 365 days (Figma "Calendrier"): a column per week, a row per day, ending today.
 * Hovering or focusing a day opens a bubble with its count and its date, which glides from one day to the next
 * (as in the Arc "Activity Heatmap"). The arrow keys move from day to day.
 */
function Heatmap({ activity }: { activity: YearActivity }) {
  const { start, first_weekday: firstWeekday, counts } = activity;
  const length = counts.length;
  const startMs = Date.parse(`${start}T00:00:00Z`);
  const dayDate = (dayIndex: number) => new Date(startMs + dayIndex * DAY_MS);
  const max = Math.max(0, ...counts);
  const columns = Math.floor((length - 1 + firstWeekday) / 7) + 1;
  const width = LEFT + columns * STEP - (STEP - CELL);
  const height = TOP + 7 * STEP - (STEP - CELL);
  const column = (dayIndex: number) => Math.floor((dayIndex + firstWeekday) / 7);

  const svgRef = useRef<SVGSVGElement>(null);
  const dayTip = useDayTip();
  // The one day that can be reached with the Tab key; the arrows move from there.
  const [focusIndex, setFocusIndex] = useState(length - 1);

  function onKeyDown(event: KeyboardEvent<SVGSVGElement>) {
    const current = (event.target as Element).closest("[data-index]");
    if (!current) return;
    const index = Number(current.getAttribute("data-index"));
    // A column is a week and a row is a day: up and down move by day, left and right by week.
    const moves: Record<string, number> = {
      ArrowUp: index - 1,
      ArrowDown: index + 1,
      ArrowLeft: index - 7,
      ArrowRight: index + 7,
      Home: 0,
      End: length - 1,
    };
    if (event.key === "Escape") {
      dayTip.hide();
    } else if (event.key in moves) {
      event.preventDefault();
      const next = Math.min(Math.max(moves[event.key], 0), length - 1);
      svgRef.current?.querySelector<SVGElement>(`[data-index="${next}"]`)?.focus();
    }
  }

  // A month is named above the week that holds its first day. The month the calendar starts in is named too, unless the
  // next one is too close for both names to fit: then the next one wins.
  const months: { name: string; x: number }[] = [];
  let lastColumn = -MONTH_WEEKS;
  for (let dayIndex = 0; dayIndex < length; dayIndex++) {
    const date = dayDate(dayIndex);
    if (dayIndex > 0 && date.getUTCDate() !== 1) continue;
    const col = column(dayIndex);
    const entry = { name: MONTHS[date.getUTCMonth()], x: LEFT + col * STEP };
    if (col - lastColumn >= MONTH_WEEKS) months.push(entry);
    else if (dayIndex > 0 && months.length === 1) months[0] = entry;
    else continue;
    lastColumn = col;
  }

  const tipIndex = dayTip.tip?.index ?? 0;

  return (
    <div className="heatmap" ref={dayTip.rootRef}>
      <svg
        ref={svgRef}
        viewBox={`0 0 ${width} ${height}`}
        style={{ width: "100%", maxWidth: width }}
        role="group"
        aria-label="Activité des 365 derniers jours, jour par jour"
        onPointerOver={(event) => dayTip.show(event.target as Element)}
        onPointerLeave={dayTip.hideSoon}
        onFocus={(event) => {
          setFocusIndex(Number((event.target as Element).getAttribute("data-index")));
          dayTip.show(event.target as Element);
        }}
        onBlur={dayTip.hideSoon}
        onKeyDown={onKeyDown}
      >
        {months.map(({ name, x }) => (
          <text key={name} className="heatmap-label" x={x} y={LABEL_BASELINE}>
            {name}
          </text>
        ))}
        {WEEKDAYS.map(([row, name]) => (
          <text key={name} className="heatmap-label" x={0} y={TOP + row * STEP + CELL - 1}>
            {name}
          </text>
        ))}
        {counts.map((count, dayIndex) => (
          <rect
            key={dayIndex}
            className={`heat-${level(count, max)}`}
            data-index={dayIndex}
            tabIndex={dayIndex === focusIndex ? 0 : -1}
            aria-label={`${formatCount(count)}, ${formatDay(dayDate(dayIndex))}`}
            x={LEFT + column(dayIndex) * STEP}
            y={TOP + ((dayIndex + firstWeekday) % 7) * STEP}
            width={CELL}
            height={CELL}
            rx={1}
          />
        ))}
      </svg>
      <div className="heatmap-legend">
        <span>Moins</span>
        {[0, 1, 2, 3, 4].map((shade) => (
          <span key={shade} className={`heatmap-key heat-${shade}`} />
        ))}
        <span>Plus</span>
      </div>
      <DayTip state={dayTip} primary={formatCount(counts[tipIndex] ?? 0)} secondary={formatDay(dayDate(tipIndex))} />
    </div>
  );
}

export default Heatmap;
