/** What src-tauri/src/stats.rs sends: the cards, then the three activity views. */
export type Metric = { value: number; change: number | null };

export type Summary = {
  dictations: Metric;
  words: Metric;
  words_per_minute: Metric;
  streak: number;
};

export type YearActivity = {
  /** "2025-10-30": the first day of the calendar, 364 days before today. */
  start: string;
  /** Of that first day: 0 for Monday, up to 6 for Sunday. */
  first_weekday: number;
  /** Dictations of each of the 365 days, the last one being today. */
  counts: number[];
  total: number;
};

export type MonthActivity = {
  year: number;
  /** 1 for January, up to 12. */
  month: number;
  counts: number[];
  elapsed: number;
  average: number;
};

export type Stats = {
  today: string;
  summary: Summary;
  year: YearActivity;
  last_month: MonthActivity;
  this_month: MonthActivity;
};

/** The period filter: the value is the number of days Rust is asked for. */
export const PERIODS = [
  { value: "1", label: "1J" },
  { value: "7", label: "7J" },
  { value: "30", label: "30J" },
  { value: "90", label: "3M" },
  { value: "365", label: "1Y" },
];

const integer = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });

/** "3 420": rounded, with the French thousands separator. */
export function formatInteger(value: number): string {
  return integer.format(value);
}

export type ChangeTone = "success" | "danger" | "neutral";

/** "+12 %", "−3 %" or "0 %", with the tone of its badge. */
export function formatChange(change: number): { text: string; tone: ChangeTone } {
  const rounded = Math.round(change);
  if (rounded > 0) return { text: `+${rounded} %`, tone: "success" };
  if (rounded < 0) return { text: `−${-rounded} %`, tone: "danger" };
  return { text: "0 %", tone: "neutral" };
}

const decimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });

/** "4,4": one decimal at most, with the French comma. */
export function formatDecimal(value: number): string {
  return decimal.format(value);
}

const dayFormat = new Intl.DateTimeFormat("fr-FR", {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

/** "jeu. 29 oct. 2026", for a calendar day given as a date at midnight UTC. */
export function formatDay(date: Date): string {
  return dayFormat.format(date);
}

/** "5 dictées", "1 dictée" or "Aucune dictée". */
export function formatCount(count: number): string {
  if (count === 0) return "Aucune dictée";
  return `${formatInteger(count)} ${count > 1 ? "dictées" : "dictée"}`;
}
