import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AudioLines, Check, Copy, Ellipsis, Flag, Pause, Play, RotateCcw, Search, Square, Trash2 } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import Alert from "./Alert";
import Dialog from "./Dialog";
import IconButton from "./IconButton";
import Menu from "./Menu";
import type { MenuItem } from "./Menu";
import NoteDialog from "./NoteDialog";
import { fold } from "./search";
import { formatDate } from "./dictation";
import type { Entry, HistoryFile, Note, NotesFile } from "./dictation";

/** Only one recording plays at a time: starting one stops the other. */
let stopCurrent: (() => void) | null = null;

type PlayState = "idle" | "playing" | "paused";

/** Plays the audio of a dictation. The WAV is only fetched when Listen is pressed. Stop goes back to the start: the next play starts over. */
function useRecording(file: string | null, onError: (message: string) => void) {
  const [state, setState] = useState<PlayState>("idle");
  const audio = useRef<HTMLAudioElement | null>(null);
  const url = useRef<string | null>(null);

  function stop() {
    audio.current?.pause();
    audio.current = null;
    if (url.current) URL.revokeObjectURL(url.current);
    url.current = null;
    setState("idle");
  }

  // The row leaves (deleted, filtered out): its sound stops with it.
  useEffect(() => stop, []);

  async function play() {
    if (!file) return;
    try {
      const bytes = await invoke<ArrayBuffer>("get_recording", { file });
      stopCurrent?.();
      url.current = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
      const element = new Audio(url.current);
      element.onended = stop;
      audio.current = element;
      stopCurrent = stop;
      setState("playing");
      await element.play();
    } catch (err) {
      stop();
      onError(String(err));
    }
  }

  function pause() {
    audio.current?.pause();
    setState("paused");
  }

  function resume() {
    audio.current
      ?.play()
      .then(() => setState("playing"))
      .catch((err) => {
        stop();
        onError(String(err));
      });
  }

  return { state, play, pause, resume, stop };
}

/** Copies the text and shows a check for a moment. */
function CopyButton({ text, onError }: { text: string; onError: (message: string) => void }) {
  const [copied, setCopied] = useState(false);

  function copy() {
    invoke("copy_text", { text })
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      })
      .catch((err) => onError(String(err)));
  }

  return <IconButton label={copied ? "Copié" : "Copier"} icon={copied ? Check : Copy} onClick={copy} />;
}

type RowProps = {
  entry: Entry;
  showHeard: boolean;
  keepAudio: boolean;
  menuOpen: boolean;
  onMenu: (entry: Entry, button: HTMLElement) => void;
  onError: (message: string) => void;
};

/** One dictation: its text, its date and what went differently, and its buttons (shown on hover). */
function EntryRow({ entry, showHeard, keepAudio, menuOpen, onMenu, onError }: RowProps) {
  const outcome = entry.paste_error ? "Collage raté" : entry.fallback_reason ? "Texte brut" : null;
  const recording = useRecording(entry.audio_file, onError);
  const listening = recording.state !== "idle";
  return (
    <li className="entry" data-menu-open={menuOpen || undefined} data-listening={listening || undefined}>
      <div className="entry-text">
        <div className="entry-detail">
          <span>{formatDate(entry.at_ms)}</span>
          {outcome && (
            <>
              <span className="entry-dot" aria-hidden="true" />
              <span className="entry-outcome">{outcome}</span>
            </>
          )}
        </div>
        <p className="entry-body">{entry.text}</p>
        {entry.paste_error && <p className="entry-outcome entry-reason">{entry.paste_error}</p>}
        {entry.fallback_reason && (
          <p className="entry-reason">Reformulation ignorée : {entry.fallback_reason}.</p>
        )}
        {showHeard && (
          <p className="entry-heard">
            <span className="quote-label">Whisper a entendu</span>
            {entry.raw_text}
          </p>
        )}
      </div>
      <div className="entry-actions">
        <CopyButton text={entry.text} onError={onError} />
        {listening ? (
          <>
            <IconButton label="Arrêter" icon={Square} onClick={recording.stop} />
            {recording.state === "playing" ? (
              <IconButton label="Mettre en pause" icon={Pause} onClick={recording.pause} />
            ) : (
              <IconButton label="Reprendre" icon={Play} onClick={recording.resume} />
            )}
          </>
        ) : (
          // Without audio and with the keeping turned off, there is nothing to play: no button.
          (entry.audio_file || keepAudio) && (
            <IconButton
              label={entry.audio_file ? "Écouter" : "Aucun enregistrement"}
              icon={Play}
              onClick={recording.play}
              disabled={!entry.audio_file}
            />
          )
        )}
        <IconButton
          label="Plus"
          icon={Ellipsis}
          onClick={(event) => onMenu(entry, event.currentTarget)}
        />
      </div>
    </li>
  );
}

type Dialogs =
  | { kind: "report"; entry: Entry }
  | { kind: "delete"; entry: Entry }
  | { kind: "rewrite"; entry: Entry };

/** The new rewriting of a dictation, next to the current text. The history is not changed. */
function RewriteDialog({ entry, onClose }: { entry: Entry; onClose: () => void }) {
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<string>("rewrite_again", { atMs: entry.at_ms })
      .then((text) => !cancelled && setResult(text))
      .catch((err) => !cancelled && setError(String(err)));
    return () => {
      cancelled = true;
    };
  }, [entry.at_ms]);

  function copy() {
    if (result === null) return;
    invoke("copy_text", { text: result })
      .then(() => setCopied(true))
      .catch((err) => setError(String(err)));
  }

  return (
    <Dialog
      title="Reformuler à nouveau"
      description="Le texte brut repasse dans la reformulation. L'historique n'est pas modifié."
      width={480}
      onClose={onClose}
      actions={
        <>
          <Button variant="secondary" size="sm" className="action-button" onClick={onClose}>
            Fermer
          </Button>
          <Button size="sm" className="accent-button" onClick={copy} disabled={result === null}>
            {copied ? "Copié" : "Copier le nouveau texte"}
          </Button>
        </>
      }
    >
      <p className="quote">
        <span className="quote-label">Texte actuel</span>
        {entry.text}
      </p>
      {error ? (
        <p className="dialog-error">{error}</p>
      ) : result === null ? (
        <p className="dialog-note">Reformulation en cours…</p>
      ) : (
        <>
          <p className="quote">
            <span className="quote-label">Nouvelle reformulation</span>
            {result}
          </p>
          {result === entry.text && (
            <p className="dialog-note">Le résultat est identique au texte actuel.</p>
          )}
        </>
      )}
    </Dialog>
  );
}

/** Every dictation that produced a text, newest first. */
function History() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [unreadable, setUnreadable] = useState(0);
  const [notes, setNotes] = useState<Note[]>([]);
  const [unreadableNotes, setUnreadableNotes] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [dialog, setDialog] = useState<Dialogs | null>(null);
  const [menu, setMenu] = useState<{ entry: Entry; anchor: DOMRect } | null>(null);
  const [heard, setHeard] = useState<number[]>([]);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [keepAudio, setKeepAudio] = useState(false);

  useEffect(() => {
    invoke<boolean>("get_keep_recordings")
      .then(setKeepAudio)
      .catch((err) => setError(String(err)));
    invoke<HistoryFile>("get_history")
      .then((file) => {
        setEntries(file.entries.slice().reverse());
        setUnreadable(file.unreadable_lines);
      })
      .catch((err) => setError(String(err)));
    invoke<NotesFile>("get_feedback")
      .then((file) => {
        setNotes(file.notes);
        setUnreadableNotes(file.unreadable_lines);
      })
      .catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const subscribe = <T,>(name: string, handler: (payload: T) => void) =>
      listen<T>(name, (event) => handler(event.payload)).then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    subscribe<Entry>("history-added", (entry) => setEntries((prev) => [entry, ...prev]));
    subscribe<string>("history-error", setError);
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  const closeDialog = () => {
    setDialog(null);
    setDialogError(null);
  };

  function openMenu(entry: Entry, button: HTMLElement) {
    setMenu({ entry, anchor: button.getBoundingClientRect() });
  }

  function menuItems(entry: Entry): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({
      label: "Reformuler à nouveau",
      icon: RotateCcw,
      onSelect: () => setDialog({ kind: "rewrite", entry }),
    });
    // A report is about the audio of the dictation: none is kept, nothing to report on.
    if (entry.audio_file) {
      items.push({
        label: "Signaler un problème",
        icon: Flag,
        onSelect: () => setDialog({ kind: "report", entry }),
      });
    }
    // What Whisper heard is only worth showing when the rewriting changed it.
    if (entry.raw_text !== entry.text) {
      const shown = heard.includes(entry.at_ms);
      items.push({
        label: shown ? "Masquer ce que Whisper a entendu" : "Voir ce que Whisper a entendu",
        icon: AudioLines,
        onSelect: () =>
          setHeard((list) =>
            shown ? list.filter((at) => at !== entry.at_ms) : [...list, entry.at_ms],
          ),
      });
    }
    items.push({
      label: "Supprimer",
      icon: Trash2,
      onSelect: () => setDialog({ kind: "delete", entry }),
      danger: true,
    });
    return items;
  }

  function saveReport(entry: Entry, comment: string) {
    return invoke<Note>("add_feedback", { entryAtMs: entry.at_ms, comment }).then((note) => {
      setNotes((prev) => [...prev, note]);
      closeDialog();
    });
  }

  function deleteEntry(entry: Entry) {
    invoke("delete_entry", { atMs: entry.at_ms })
      .then(() => {
        setEntries((prev) => prev.filter((e) => e.at_ms !== entry.at_ms));
        setNotes((prev) => prev.filter((n) => n.dictation?.at_ms !== entry.at_ms));
        closeDialog();
      })
      .catch((err) => setDialogError(String(err)));
  }

  const needle = fold(query.trim());
  const shown = needle ? entries.filter((entry) => fold(entry.text).includes(needle)) : entries;
  const plural = (n: number, one: string, many: string) => (n > 1 ? many : one);

  return (
    <>
      <h1 className="page-title">Historique</h1>
      <div className="page-block">
        <label className="field search">
          <input
            type="search"
            placeholder="Rechercher dans l'historique"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <Search size={16} strokeWidth={1.75} aria-hidden="true" />
        </label>
        {error && <Alert tone="danger" title={error} />}
        {unreadable > 0 && (
          <Alert
            tone="danger"
            title={`${unreadable} ${plural(unreadable, "dictée illisible", "dictées illisibles")} dans le fichier de l'historique`}
            text={`${plural(unreadable, "Elle n'est pas affichée.", "Elles ne sont pas affichées.")}`}
          />
        )}
        {unreadableNotes > 0 && (
          <Alert
            tone="danger"
            title={`${unreadableNotes} ${plural(unreadableNotes, "note illisible", "notes illisibles")} dans le fichier des notes`}
            text={`${plural(unreadableNotes, "Elle n'est pas affichée.", "Elles ne sont pas affichées.")}`}
          />
        )}
        {shown.length > 0 ? (
          <ul className="list">
            {shown.map((entry) => (
              <EntryRow
                key={entry.at_ms}
                entry={entry}
                showHeard={heard.includes(entry.at_ms)}
                keepAudio={keepAudio}
                menuOpen={menu?.entry.at_ms === entry.at_ms}
                onMenu={openMenu}
                onError={setError}
              />
            ))}
          </ul>
        ) : (
          !error && (
            <p className="page-soon">
              {entries.length === 0
                ? "Aucune dictée pour l'instant. Chaque texte dicté apparaîtra ici, même si le collage échoue."
                : `Aucune dictée ne correspond à « ${query.trim()} ».`}
            </p>
          )
        )}
      </div>
      {menu && <Menu anchor={menu.anchor} items={menuItems(menu.entry)} onClose={() => setMenu(null)} />}

      {dialog?.kind === "report" && (
        <NoteDialog
          title="Signaler un problème"
          description="Décris ce qui ne va pas dans cette dictée."
          quote={dialog.entry.text}
          placeholder="Qu'est-ce qui ne va pas dans cette dictée ?"
          footnote="Enregistré sur ce Mac avec la dictée, jamais envoyé."
          notes={notes.filter((note) => note.dictation?.at_ms === dialog.entry.at_ms)}
          onSave={(comment) => saveReport(dialog.entry, comment)}
          onClose={closeDialog}
        />
      )}
      {dialog?.kind === "rewrite" && <RewriteDialog entry={dialog.entry} onClose={closeDialog} />}
      {dialog?.kind === "delete" && (
        <Dialog
          title="Supprimer cette dictée ?"
          description="Le texte, l'enregistrement et les notes liées seront supprimés de ce Mac. Impossible de revenir en arrière."
          onClose={closeDialog}
          actions={
            <>
              <Button variant="secondary" size="sm" className="action-button" onClick={closeDialog}>
                Annuler
              </Button>
              <Button variant="danger" size="sm" onClick={() => deleteEntry(dialog.entry)}>
                Supprimer
              </Button>
            </>
          }
        >
          <p className="quote">{dialog.entry.text}</p>
          {dialogError && <p className="dialog-error">{dialogError}</p>}
        </Dialog>
      )}
    </>
  );
}

export default History;
