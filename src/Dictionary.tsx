import { useCallback, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Pencil, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import SegmentedControl from "@/components/arc/segmented-control/segmented-control";
import Alert from "./Alert";
import Dialog from "./Dialog";
import IconButton from "./IconButton";
import SettingsRow, { SettingsSection } from "./SettingsRow";
import { fold } from "./search";

/** One word and the parts the transcription may hear instead, as saved by src-tauri/src/dictionary.rs. */
type Word = { right: string; heard: string[] };

/** The orders of the list: the newest words first (the default), the oldest first, or alphabetical. */
const ORDERS = [
  { value: "recent", label: "Récents" },
  { value: "old", label: "Anciens" },
  { value: "alpha", label: "Ordre alphabétique" },
];

/** "a, b ,, c" becomes ["a", "b", "c"]. */
function parseHeard(text: string): string[] {
  return text
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

type FormProps = {
  right: string;
  heard: string;
  onRight: (value: string) => void;
  onHeard: (value: string) => void;
};

/** The two fields of a word: the word, and what is often heard instead (several, separated by commas). */
function WordFields({ right, heard, onRight, onHeard }: FormProps) {
  return (
    <>
      <label className="field field-raised">
        <input
          type="text"
          placeholder="Nouveau mot"
          aria-label="Mot"
          value={right}
          onChange={(event) => onRight(event.target.value)}
        />
      </label>
      <label className="field field-raised">
        <input
          type="text"
          placeholder="Souvent entendu comme"
          aria-label="Souvent entendu comme"
          value={heard}
          onChange={(event) => onHeard(event.target.value)}
        />
      </label>
    </>
  );
}

/** The window that changes a word and its variants. */
function EditDialog({ word, onClose }: { word: Word; onClose: () => void }) {
  const [right, setRight] = useState(word.right);
  const [heard, setHeard] = useState(word.heard.join(", "));
  const [error, setError] = useState<string | null>(null);

  function save(event: FormEvent) {
    event.preventDefault();
    invoke("save_dictionary_word", { previous: word.right, right, heard: parseHeard(heard) })
      .then(onClose)
      .catch((err) => setError(String(err)));
  }

  return (
    <Dialog
      title="Modifier le mot"
      description="« Souvent entendu comme » : plusieurs variantes possibles, séparées par des virgules."
      onClose={onClose}
      actions={
        <>
          <Button variant="secondary" size="sm" className="action-button" onClick={onClose}>
            Annuler
          </Button>
          <Button size="sm" className="accent-button" onClick={save}>
            Enregistrer
          </Button>
        </>
      }
    >
      <form className="word-form word-form-stacked" onSubmit={save}>
        <WordFields right={right} heard={heard} onRight={setRight} onHeard={setHeard} />
        {/* Enter saves, even though the buttons of the window are outside the form. */}
        <button type="submit" hidden />
      </form>
      {error && <p className="dialog-error">{error}</p>}
    </Dialog>
  );
}

/** The dictionary page (Figma screen 03): add a word, search, and change or remove the words. */
function Dictionary() {
  const [words, setWords] = useState<Word[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [order, setOrder] = useState("recent");
  const [right, setRight] = useState("");
  const [heard, setHeard] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Word | null>(null);
  const [removing, setRemoving] = useState<Word | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const load = useCallback(() => {
    invoke<Word[]>("get_dictionary")
      .then((list) => {
        setWords(list);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    load();
    // Rust says when a word was learned by the shortcut, saved, removed, or when "Effacer les données" emptied the list.
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen("dictionary-changed", load).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  function add(event: FormEvent) {
    event.preventDefault();
    invoke("save_dictionary_word", { previous: null, right, heard: parseHeard(heard) })
      .then(() => {
        setRight("");
        setHeard("");
        setFormError(null);
      })
      .catch((err) => setFormError(String(err)));
  }

  function remove(word: Word) {
    invoke("remove_dictionary_word", { right: word.right })
      .then(() => {
        setRemoving(null);
        setRemoveError(null);
      })
      .catch((err) => setRemoveError(String(err)));
  }

  const needle = fold(query.trim());
  // The file keeps the words in the order they were added: the newest is the last one.
  const inFile = words ?? [];
  const ordered =
    order === "alpha"
      ? [...inFile].sort((a, b) => a.right.localeCompare(b.right, "fr", { sensitivity: "base" }))
      : order === "old"
        ? inFile
        : [...inFile].reverse();
  const shown = ordered.filter(
    (word) => !needle || fold(word.right).includes(needle) || word.heard.some((part) => fold(part).includes(needle)),
  );
  const hasWords = words !== null && words.length > 0;

  return (
    <>
      <h1 className="page-title">Dictionnaire</h1>
      <p className="page-intro">
        Tes mots, avec ce que la transcription comprend parfois à leur place : « Souvent entendu comme » est corrigé dans
        le texte collé (plusieurs variantes possibles, séparées par des virgules). Le raccourci « Apprendre un mot » en
        ajoute aussi.
      </p>
      <form className="word-form" onSubmit={add}>
        <WordFields right={right} heard={heard} onRight={setRight} onHeard={setHeard} />
        <Button type="submit" size="sm" className="accent-button">
          Ajouter
        </Button>
      </form>
      {formError && <p className="dialog-error">{formError}</p>}
      {error && <Alert tone="danger" title={error} />}
      {hasWords && (
        <div className="dictionary-tools">
          <SegmentedControl
            className="period-filter"
            label="Ordre"
            options={ORDERS}
            value={order}
            onValueChange={setOrder}
          />
          <label className="field search">
            <input
              type="search"
              placeholder="Rechercher un mot"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <Search size={16} strokeWidth={1.75} aria-hidden="true" />
          </label>
        </div>
      )}
      {hasWords && shown.length === 0 && <p className="page-soon">Aucun mot ne correspond à cette recherche.</p>}
      {shown.length > 0 && (
        <SettingsSection className="dictionary-list">
          {shown.map((word) => (
            <SettingsRow
              key={word.right}
              title={word.right}
              detail={
                word.heard.length > 0 ? (
                  <>
                    entendu comme <span className="heard">{word.heard.join(", ")}</span>
                  </>
                ) : undefined
              }
            >
              <span className="word-actions">
                <IconButton label="Modifier" icon={Pencil} onClick={() => setEditing(word)} />
                <IconButton label="Supprimer" icon={Trash2} onClick={() => setRemoving(word)} />
              </span>
            </SettingsRow>
          ))}
        </SettingsSection>
      )}
      {editing && <EditDialog word={editing} onClose={() => setEditing(null)} />}
      {removing && (
        <Dialog
          title={`Supprimer « ${removing.right} » ?`}
          description="Le mot et ses variantes sortent du dictionnaire. Les textes déjà collés ne changent pas."
          onClose={() => {
            setRemoving(null);
            setRemoveError(null);
          }}
          actions={
            <>
              <Button
                variant="secondary"
                size="sm"
                className="action-button"
                onClick={() => {
                  setRemoving(null);
                  setRemoveError(null);
                }}
              >
                Annuler
              </Button>
              <Button variant="danger" size="sm" onClick={() => remove(removing)}>
                Supprimer
              </Button>
            </>
          }
        >
          {removeError && <p className="dialog-error">{removeError}</p>}
        </Dialog>
      )}
    </>
  );
}

export default Dictionary;
