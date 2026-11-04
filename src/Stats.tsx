import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import SegmentedControl from "@/components/arc/segmented-control/segmented-control";
import Activity from "./Activity";
import Alert from "./Alert";
import StatCard from "./StatCard";
import { PERIODS } from "./statsData";
import type { Stats as StatsData } from "./statsData";

/** The statistics page (Figma screen 05): the period filter, the four cards, and the activity. */
function Stats() {
  const [period, setPeriod] = useState("30");
  const [stats, setStats] = useState<StatsData | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    invoke<StatsData>("get_stats", { period: Number(period) })
      .then((data) => {
        setStats(data);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  }, [period]);

  useEffect(() => {
    load();
    // Rust says when a dictation was counted or the counters were reset.
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen("stats-changed", load).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  const summary = stats?.summary;
  return (
    <>
      <div className="page-header">
        <h1 className="page-title">Statistiques</h1>
        <SegmentedControl
          className="period-filter"
          label="Période"
          options={PERIODS}
          value={period}
          onValueChange={setPeriod}
        />
      </div>
      {error && <Alert tone="danger" title={error} />}
      {summary && (
        <div className="stat-cards">
          <StatCard
            label="Dictées"
            value={summary.dictations.value}
            change={summary.dictations.change}
          />
          <StatCard
            label="Mots dictés"
            value={summary.words.value}
            change={summary.words.change}
          />
          <StatCard
            label="Vitesse moyenne"
            value={summary.words_per_minute.value}
            suffix={"\u00a0WPM"}
            change={summary.words_per_minute.change}
          />
          <StatCard
            label="Série en cours"
            value={summary.streak}
            suffix={summary.streak > 1 ? "\u00a0jours" : "\u00a0jour"}
          />
        </div>
      )}
      {stats && <Activity stats={stats} />}
      <p className="stats-note">
        Les statistiques comptent toutes tes dictées, même celles supprimées de l'historique. Seuls des compteurs par
        jour sont gardés, jamais le texte. Remise à zéro dans Réglages.
      </p>
    </>
  );
}

export default Stats;
