import { AnimatedCounter } from "@/components/arc/animated-counter/animated-counter";
import { Badge } from "@/components/arc/badge/badge";
import { formatChange } from "./statsData";

type Props = {
  label: string;
  value: number;
  /** Written after the number, with a no-break space in front (a plain one would be dropped), such as "\u00a0WPM". */
  suffix?: string;
  /** Percent compared with the period before. Nothing is shown when there is none. */
  change?: number | null;
};

/** One card of the statistics (Figma "Carte"): its name, its change, and its big number, which rolls to each new value. */
function StatCard({ label, value, suffix, change }: Props) {
  const shown = change == null ? null : formatChange(change);
  return (
    <div className="stat-card">
      <div className="stat-card-header">
        <p className="stat-card-label">{label}</p>
        {shown && (
          <Badge className="stat-change" data-tone={shown.tone}>
            {shown.text}
          </Badge>
        )}
      </div>
      {/* Rolls up from zero each time the page opens (animateOnView), and from the old number when it changes. */}
      <div className="stat-card-value" data-unit={suffix ? "" : undefined}>
        {/* The Swedish locale groups thousands with a full no-break space; the French one uses a narrow one that looks glued at this size. */}
        <AnimatedCounter value={value} suffix={suffix} locale="sv-SE" animateOnView />
      </div>
    </div>
  );
}

export default StatCard;
