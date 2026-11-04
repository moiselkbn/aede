import { useState } from "react";
import { Button } from "@/components/arc/button/button";
import Dialog from "./Dialog";
import { formatDate } from "./dictation";
import type { Note } from "./dictation";

type Props = {
  title: string;
  description: string;
  /** The text the note is about, shown above the field. */
  quote?: string;
  placeholder: string;
  /** Said under the field: where the note goes. */
  footnote: string;
  /** Notes already written, oldest first. */
  notes: Note[];
  onSave: (comment: string) => Promise<void>;
  onClose: () => void;
};

/** Figma screen 07: write a note, which stays on this Mac. Used for a dictation and for an opinion. */
function NoteDialog({ title, description, quote, placeholder, footnote, notes, onSave, onClose }: Props) {
  const [comment, setComment] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function save() {
    setSaving(true);
    setError(null);
    onSave(comment).catch((err) => {
      setError(String(err));
      setSaving(false);
    });
  }

  return (
    <Dialog
      title={title}
      description={description}
      width={480}
      onClose={onClose}
      actions={
        <>
          <Button
            variant="secondary"
            size="sm"
            className="action-button"
            onClick={onClose}
            disabled={saving}
          >
            Annuler
          </Button>
          <Button
            size="sm"
            className="accent-button"
            onClick={save}
            disabled={saving || comment.trim() === ""}
          >
            Enregistrer
          </Button>
        </>
      }
    >
      {quote && <p className="quote">{quote}</p>}
      {notes.map((note) => (
        <p className="quote note-card" key={note.at_ms}>
          <span className="quote-label">Note du {formatDate(note.at_ms)}</span>
          {note.comment}
        </p>
      ))}
      <textarea
        className="field field-area"
        autoFocus
        placeholder={placeholder}
        value={comment}
        onChange={(event) => setComment(event.target.value)}
      />
      <p className="dialog-note">{footnote}</p>
      {error && <p className="dialog-error">{error}</p>}
    </Dialog>
  );
}

export default NoteDialog;
