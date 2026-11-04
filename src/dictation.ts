// Types of what Rust saves (history.rs, feedback.rs) and the date format shared by the pages.

/** One dictation, as saved by src-tauri/src/history.rs. */
export type Entry = {
  at_ms: number;
  text: string;
  /** What Whisper heard, before the rewriting. */
  raw_text: string;
  fallback_reason: string | null;
  paste_error: string | null;
  /** Name of the WAV kept by src-tauri/src/recordings.rs. Null if there is none. */
  audio_file: string | null;
};
export type HistoryFile = { entries: Entry[]; unreadable_lines: number };

/** One note of the user, as saved by src-tauri/src/feedback.rs. */
export type Note = {
  at_ms: number;
  comment: string;
  /** The dictation it is about. Null for a general opinion. */
  dictation: Entry | null;
};
export type NotesFile = { notes: Note[]; unreadable_lines: number };

const TIME = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });
const DAY = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "short" });

/** "Aujourd'hui, 14:32", "Hier, 18:05", "12 oct., 18:05" */
export function formatDate(ms: number) {
  const date = new Date(ms);
  const daysAgo = Math.round(
    (new Date().setHours(0, 0, 0, 0) - new Date(ms).setHours(0, 0, 0, 0)) / 86_400_000,
  );
  if (daysAgo === 0) return `Aujourd'hui, ${TIME.format(date)}`;
  if (daysAgo === 1) return `Hier, ${TIME.format(date)}`;
  return `${DAY.format(date)}, ${TIME.format(date)}`;
}
