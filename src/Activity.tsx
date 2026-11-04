import { useState } from "react";
import SegmentedControl from "@/components/arc/segmented-control/segmented-control";
import Heatmap from "./Heatmap";
import MonthBars from "./MonthBars";
import { formatDecimal, formatInteger } from "./statsData";
import type { MonthActivity, Stats } from "./statsData";

const VIEWS = [
  { value: "year", label: "12 mois" },
  { value: "last", label: "Le mois dernier" },
  { value: "current", label: "Ce mois-ci" },
];

/** "Septembre 2026". */
function monthTitle({ year, month }: MonthActivity): string {
  const text = new Date(Date.UTC(year, month - 1, 1)).toLocaleDateString("fr-FR", {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The activity block (Figma "Activité"): the days of the year as a calendar, or the days of a month as bars. */
function Activity({ stats }: { stats: Stats }) {
  const [view, setView] = useState("year");
  const month = view === "last" ? stats.last_month : stats.this_month;
  const { year } = stats;

  return (
    <section className="activity-card">
      <div className="activity-header">
        {view === "year" ? (
          <p className="activity-title">
            {formatInteger(year.total)} {year.total > 1 ? "dictées" : "dictée"} sur 12 mois
          </p>
        ) : (
          <div className="activity-summary">
            <p className="activity-month">{monthTitle(month)}</p>
            <p className="activity-average">
              <span className="activity-average-value">{formatDecimal(month.average)}</span>
              <span className="activity-average-label">
                {month.average >= 2 ? "dictées" : "dictée"} en moyenne
              </span>
            </p>
          </div>
        )}
        <SegmentedControl
          className="period-filter"
          label="Activité"
          options={VIEWS}
          value={view}
          onValueChange={setView}
        />
      </div>
      {view === "year" ? <Heatmap activity={year} /> : <MonthBars activity={month} />}
    </section>
  );
}

export default Activity;
