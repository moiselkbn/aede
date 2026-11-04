//! The personal dictionary: words, each with the parts the transcription may hear instead ("heard"),
//! saved in dictionary.json in the app data folder, and the replacement of the heard parts by
//! the words in a text. A word may have no heard part: it then replaces nothing (it is kept
//! for a possible later use, such as telling the rewriting model which words to expect).
//!
//! The replacement works on the final text, after the rewriting: the rewriting model never sees
//! the dictionary. A heard part matches whole words only, whatever the case, and ignoring spaces,
//! hyphens and punctuation inside it: "Fynd do doc" and "FyndDoDoc" are the same heard part. The
//! file only stays on this Mac and only the user can read it.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const FILE_NAME: &str = "dictionary.json";

/// Sent to the window whenever the words change, so that it shows the list as it is.
pub const CHANGED_EVENT: &str = "dictionary-changed";

/// Most words a heard part may have: it is looked for at each place of the text.
const MAX_WORDS: usize = 6;

/// Only one write at a time: two saves must not mix, and "clear all" must not cross a save.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// One correction: what the transcription says, and what the user wants instead. The replacement
/// works on these; the file keeps them grouped by word.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub wrong: String,
    pub right: String,
}

/// A word of the dictionary, and the parts the transcription may hear instead of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub right: String,
    #[serde(default)]
    pub heard: Vec<String>,
}

/// The content of the file.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
// A field missing from the file (added in a later version) takes its default value.
#[serde(default)]
struct Dictionary {
    words: Vec<Word>,
    /// The first format of the file, one pair after the other. It is read, turned into words, and
    /// never written again.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pairs: Vec<Pair>,
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(FILE_NAME))
}

fn notify_changed(app: &AppHandle) {
    if let Err(err) = app.emit(CHANGED_EVENT, ()) {
        eprintln!("[dictionary] could not notify the UI: {err}");
    }
}

/// The words saved so far (none if the file does not exist yet).
pub fn load_words(app: &AppHandle) -> Result<Vec<Word>, String> {
    let path = path(app)?;
    let _guard = lock();
    Ok(load_from(&path)?.words)
}

/// The heard parts with their words, one pair each: what the replacement and the learning use.
pub fn load(app: &AppHandle) -> Result<Vec<Pair>, String> {
    Ok(to_pairs(&load_words(app)?))
}

#[tauri::command]
pub fn get_dictionary(app: AppHandle) -> Result<Vec<Word>, String> {
    load_words(&app)
}

/// Creates a word (`previous` is None) or replaces the word called `previous`, with its heard parts.
#[tauri::command]
pub fn save_dictionary_word(
    app: AppHandle,
    previous: Option<String>,
    right: String,
    heard: Vec<String>,
) -> Result<(), String> {
    let path = path(&app)?;
    {
        let _guard = lock();
        let mut dictionary = load_from(&path)?;
        save_word(&mut dictionary.words, previous.as_deref(), &right, &heard)?;
        save_to(&path, &dictionary)?;
    }
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn remove_dictionary_word(app: AppHandle, right: String) -> Result<(), String> {
    let path = path(&app)?;
    {
        let _guard = lock();
        let mut dictionary = load_from(&path)?;
        dictionary.words.retain(|word| word.right != right);
        save_to(&path, &dictionary)?;
    }
    notify_changed(&app);
    Ok(())
}

/// Adds a pair (what the learning shortcut finds). If the heard part is already there, it moves
/// to the new word.
pub fn add(app: &AppHandle, wrong: &str, right: &str) -> Result<(), String> {
    let path = path(app)?;
    let _guard = lock();
    let mut dictionary = load_from(&path)?;
    add_to(&mut dictionary.words, wrong, right)?;
    save_to(&path, &dictionary)
}

/// Deletes the file: no word is left on disk.
pub fn clear(app: &AppHandle) -> Result<(), String> {
    let path = path(app)?;
    let _guard = lock();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer le dictionnaire ({}) : {err}",
            path.display()
        )),
    }
}

fn load_from(path: &Path) -> Result<Dictionary, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Dictionary::default());
        }
        Err(err) => {
            return Err(format!(
                "Dictionnaire illisible ({}) : {err}",
                path.display()
            ));
        }
    };
    let mut dictionary: Dictionary = serde_json::from_str(&text)
        .map_err(|err| format!("Dictionnaire illisible ({}) : {err}", path.display()))?;
    // A file of the first format: its pairs become words.
    for pair in std::mem::take(&mut dictionary.pairs) {
        add_to(&mut dictionary.words, &pair.wrong, &pair.right)
            .map_err(|err| format!("Dictionnaire illisible ({}) : {err}", path.display()))?;
    }
    Ok(dictionary)
}

/// Writes to a temporary file, then renames it: a crash in the middle never leaves a half file.
fn save_to(path: &Path, dictionary: &Dictionary) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(dictionary)
        .map_err(|err| format!("Impossible d'écrire le dictionnaire : {err}"))?;
    let temporary = path.with_extension("json.tmp");
    let failed = |err: std::io::Error| {
        format!(
            "Impossible d'enregistrer le dictionnaire ({}) : {err}",
            path.display()
        )
    };
    // 0o600: only the user can read the file. Applies when the file is created.
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(failed)?;
    std::fs::rename(&temporary, path).map_err(failed)
}

fn to_pairs(words: &[Word]) -> Vec<Pair> {
    words
        .iter()
        .flat_map(|word| {
            word.heard.iter().map(|heard| Pair {
                wrong: heard.clone(),
                right: word.right.clone(),
            })
        })
        .collect()
}

/// Checks a heard part against its word and returns what it is compared by.
fn check_heard(wrong: &str, right: &str) -> Result<String, String> {
    if wrong.is_empty() || right.is_empty() {
        return Err("Le mot mal entendu et le bon mot ne peuvent pas être vides.".into());
    }
    if wrong == right {
        return Err("Le mot mal entendu et le bon mot sont identiques.".into());
    }
    let key = normalize(wrong);
    if key.is_empty() {
        return Err("Le mot mal entendu doit contenir au moins une lettre ou un chiffre.".into());
    }
    if wrong.split_whitespace().count() > MAX_WORDS {
        return Err(format!(
            "Le mot mal entendu a plus de {MAX_WORDS} mots : c'est trop long pour être cherché."
        ));
    }
    Ok(key)
}

/// Adds a heard part to a word, which is created if needed. A heard part that was under another
/// word moves; a word left without any heard part by that move is dropped.
fn add_to(words: &mut Vec<Word>, wrong: &str, right: &str) -> Result<(), String> {
    let (wrong, right) = (wrong.trim(), right.trim());
    let key = check_heard(wrong, right)?;
    let holders: Vec<String> = words
        .iter()
        .filter(|word| word.heard.iter().any(|heard| normalize(heard) == key))
        .map(|word| word.right.clone())
        .collect();
    for word in words.iter_mut() {
        word.heard.retain(|heard| normalize(heard) != key);
    }
    words.retain(|word| {
        !(word.heard.is_empty() && holders.contains(&word.right) && word.right != right)
    });
    match words.iter_mut().find(|word| word.right == right) {
        Some(word) => word.heard.push(wrong.to_string()),
        None => words.push(Word {
            right: right.to_string(),
            heard: vec![wrong.to_string()],
        }),
    }
    Ok(())
}

/// Creates the word `right` with its heard parts, or replaces the word called `previous`.
fn save_word(
    words: &mut Vec<Word>,
    previous: Option<&str>,
    right: &str,
    heard: &[String],
) -> Result<(), String> {
    let right = right.trim();
    if right.is_empty() {
        return Err("Le mot ne peut pas être vide.".into());
    }
    let mut cleaned: Vec<String> = Vec::new();
    for part in heard
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
    {
        let key = check_heard(part, right)?;
        if !cleaned.iter().any(|kept| normalize(kept) == key) {
            cleaned.push(part.to_string());
        }
    }
    let position = match previous {
        Some(name) => Some(
            words
                .iter()
                .position(|word| word.right == name)
                .ok_or("Ce mot n'existe plus dans le dictionnaire.")?,
        ),
        None => None,
    };
    for (index, word) in words.iter().enumerate() {
        if Some(index) == position {
            continue;
        }
        if word.right == right {
            return Err(format!("« {right} » est déjà dans le dictionnaire."));
        }
        for part in &cleaned {
            if word
                .heard
                .iter()
                .any(|kept| normalize(kept) == normalize(part))
            {
                return Err(format!(
                    "« {part} » est déjà une variante de « {} ».",
                    word.right
                ));
            }
        }
    }
    let word = Word {
        right: right.to_string(),
        heard: cleaned,
    };
    match position {
        Some(index) => words[index] = word,
        None => words.push(word),
    }
    Ok(())
}

/// What is compared: only the letters and digits, in lower case.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A word of a text, with the punctuation around it kept apart.
struct Token<'a> {
    /// The spaces and line breaks before the word.
    space: &'a str,
    /// The punctuation before the first letter ("(" in "(Versel").
    lead: &'a str,
    /// From the first letter or digit to the last one.
    core: &'a str,
    /// The punctuation after the last letter ("," in "Versel,").
    trail: &'a str,
}

/// Cuts a text into words. Also returns the spaces left after the last word.
fn tokenize(text: &str) -> (Vec<Token<'_>>, &str) {
    let mut tokens = Vec::new();
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let space = &rest[..rest.len() - trimmed.len()];
        if trimmed.is_empty() {
            return (tokens, space);
        }
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let word = &trimmed[..end];
        rest = &trimmed[end..];
        let first = word.find(|c: char| c.is_alphanumeric());
        let last = word
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_alphanumeric())
            .map(|(i, c)| i + c.len_utf8());
        tokens.push(match (first, last) {
            (Some(first), Some(last)) => Token {
                space,
                lead: &word[..first],
                core: &word[first..last],
                trail: &word[last..],
            },
            // Only punctuation: it can never be part of a heard part.
            _ => Token {
                space,
                lead: word,
                core: "",
                trail: "",
            },
        });
    }
}

/// Can these words be one heard part? No punctuation inside it, apart from around it.
fn is_candidate(window: &[Token]) -> bool {
    let last = window.len() - 1;
    window.iter().enumerate().all(|(i, token)| {
        !token.core.is_empty()
            && (i == 0 || token.lead.is_empty())
            && (i == last || token.trail.is_empty())
    })
}

/// Replaces in `text` every heard part of the dictionary by its wanted word. The punctuation
/// and the spaces around the replaced words stay as they were.
pub fn apply(pairs: &[Pair], text: &str) -> String {
    let keys: Vec<(String, &str)> = pairs
        .iter()
        .map(|pair| (normalize(&pair.wrong), pair.right.as_str()))
        .filter(|(key, _)| !key.is_empty())
        .collect();
    if keys.is_empty() {
        return text.to_string();
    }
    let (tokens, tail) = tokenize(text);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < tokens.len() {
        // The longest heard part first: "Fynd de doc" before "Fynd".
        let found = (1..=MAX_WORDS.min(tokens.len() - i))
            .rev()
            .filter(|&len| is_candidate(&tokens[i..i + len]))
            .find_map(|len| {
                let joined: String = tokens[i..i + len]
                    .iter()
                    .map(|t| normalize(t.core))
                    .collect();
                keys.iter()
                    .find(|(key, _)| *key == joined)
                    .map(|(_, right)| (len, *right))
            });
        match found {
            Some((len, right)) => {
                out.push_str(tokens[i].space);
                out.push_str(tokens[i].lead);
                out.push_str(right);
                out.push_str(tokens[i + len - 1].trail);
                i += len;
            }
            None => {
                let token = &tokens[i];
                out.push_str(token.space);
                out.push_str(token.lead);
                out.push_str(token.core);
                out.push_str(token.trail);
                i += 1;
            }
        }
    }
    out.push_str(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(wrong: &str, right: &str) -> Pair {
        Pair {
            wrong: wrong.into(),
            right: right.into(),
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aede-dictionary-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn a_word_is_replaced() {
        let pairs = [pair("Versel", "Vercel")];
        assert_eq!(
            apply(&pairs, "Déploie sur Versel puis ouvre une pull request."),
            "Déploie sur Vercel puis ouvre une pull request."
        );
    }

    #[test]
    fn the_case_of_the_text_does_not_matter() {
        let pairs = [pair("versel", "Vercel")];
        assert_eq!(apply(&pairs, "VERSEL et Versel"), "Vercel et Vercel");
    }

    #[test]
    fn only_whole_words_are_replaced() {
        let pairs = [pair("Versel", "Vercel")];
        assert_eq!(
            apply(&pairs, "Versels, Reversel et Versel."),
            "Versels, Reversel et Vercel."
        );
    }

    #[test]
    fn punctuation_around_the_word_stays() {
        let pairs = [pair("Versel", "Vercel")];
        assert_eq!(
            apply(&pairs, "(Versel), « Versel » ; Versel !"),
            "(Vercel), « Vercel » ; Vercel !"
        );
    }

    #[test]
    fn three_words_become_one() {
        let pairs = [pair("Fynd de doc", "FindTheDoc")];
        assert_eq!(
            apply(&pairs, "Fynd de doc : une application utile."),
            "FindTheDoc : une application utile."
        );
    }

    #[test]
    fn one_word_becomes_three() {
        let pairs = [pair("FindTheDoc", "Find The Doc")];
        assert_eq!(
            apply(&pairs, "Ouvre FindTheDoc, puis ferme."),
            "Ouvre Find The Doc, puis ferme."
        );
    }

    #[test]
    fn spaces_and_hyphens_inside_the_heard_part_do_not_matter() {
        let pairs = [pair("FyndDoDoc", "FindTheDoc")];
        assert_eq!(
            apply(&pairs, "Fynd do doc, Fynd-do-doc et fynddodoc"),
            "FindTheDoc, FindTheDoc et FindTheDoc"
        );
    }

    #[test]
    fn a_comma_inside_the_heard_part_blocks_the_match() {
        let pairs = [pair("Fynd de doc", "FindTheDoc")];
        assert_eq!(apply(&pairs, "Fynd, de doc"), "Fynd, de doc");
    }

    #[test]
    fn the_longest_heard_part_wins() {
        let pairs = [pair("Fynd", "Find"), pair("Fynd de doc", "FindTheDoc")];
        assert_eq!(apply(&pairs, "Fynd de doc et Fynd"), "FindTheDoc et Find");
    }

    #[test]
    fn spaces_and_line_breaks_are_kept() {
        let pairs = [pair("Versel", "Vercel")];
        assert_eq!(
            apply(&pairs, "  Sur  Versel\n\n- Versel \n"),
            "  Sur  Vercel\n\n- Vercel \n"
        );
    }

    #[test]
    fn an_empty_dictionary_changes_nothing() {
        assert_eq!(apply(&[], "Déploie sur Versel."), "Déploie sur Versel.");
    }

    #[test]
    fn the_replacement_can_be_applied_twice() {
        let pairs = [pair("Versel", "Vercel")];
        let once = apply(&pairs, "Sur Versel.");
        assert_eq!(apply(&pairs, &once), once);
    }

    fn word(right: &str, heard: &[&str]) -> Word {
        Word {
            right: right.into(),
            heard: heard.iter().map(|part| part.to_string()).collect(),
        }
    }

    #[test]
    fn a_heard_part_is_added_to_its_word() {
        let mut words = Vec::new();
        add_to(&mut words, " Fynd do doc ", "FindTheDoc").unwrap();
        add_to(&mut words, "Fynd de doc", "FindTheDoc").unwrap();
        assert_eq!(
            words,
            vec![word("FindTheDoc", &["Fynd do doc", "Fynd de doc"])]
        );
        assert_eq!(
            to_pairs(&words),
            vec![
                pair("Fynd do doc", "FindTheDoc"),
                pair("Fynd de doc", "FindTheDoc")
            ]
        );
    }

    #[test]
    fn a_heard_part_moves_to_another_word() {
        let mut words = Vec::new();
        add_to(&mut words, " Fynd do doc ", "FindTheDoc").unwrap();
        add_to(&mut words, "FyndDoDoc", "Find The Doc").unwrap();
        // The first word lost its only heard part and is dropped with it.
        assert_eq!(words, vec![word("Find The Doc", &["FyndDoDoc"])]);
    }

    #[test]
    fn a_word_written_by_hand_survives_a_move() {
        let mut words = vec![word("Camille", &[]), word("FindTheDoc", &["Fynd do doc"])];
        add_to(&mut words, "Fynd do doc", "Find The Doc").unwrap();
        assert_eq!(
            words,
            vec![word("Camille", &[]), word("Find The Doc", &["Fynd do doc"])]
        );
    }

    #[test]
    fn bad_pairs_are_refused_with_a_message() {
        let mut words = Vec::new();
        assert!(
            add_to(&mut words, "", "Vercel")
                .unwrap_err()
                .contains("vides")
        );
        assert!(
            add_to(&mut words, "Versel", " ")
                .unwrap_err()
                .contains("vides")
        );
        assert!(
            add_to(&mut words, "Vercel", "Vercel")
                .unwrap_err()
                .contains("identiques")
        );
        assert!(
            add_to(&mut words, "...", "Vercel")
                .unwrap_err()
                .contains("lettre")
        );
        assert!(
            add_to(&mut words, "a b c d e f g", "x")
                .unwrap_err()
                .contains("trop long")
        );
        assert!(words.is_empty());
    }

    #[test]
    fn a_case_change_alone_is_a_valid_pair() {
        let mut words = Vec::new();
        add_to(&mut words, "findthedoc", "FindTheDoc").unwrap();
        assert_eq!(
            apply(&to_pairs(&words), "Ouvre findthedoc."),
            "Ouvre FindTheDoc."
        );
    }

    #[test]
    fn a_word_without_heard_part_replaces_nothing() {
        let words = vec![word("Camille", &[])];
        assert!(to_pairs(&words).is_empty());
        assert_eq!(apply(&to_pairs(&words), "Salut Julien."), "Salut Julien.");
    }

    #[test]
    fn a_word_is_created_with_its_heard_parts() {
        let mut words = Vec::new();
        let heard = [
            " super base ".to_string(),
            "".to_string(),
            "Super-Base".to_string(),
            "super basse".to_string(),
        ];
        save_word(&mut words, None, " Supabase ", &heard).unwrap();
        // The empty one is dropped, and "Super-Base" is the same heard part as "super base".
        assert_eq!(
            words,
            vec![word("Supabase", &["super base", "super basse"])]
        );
    }

    #[test]
    fn a_word_without_variant_can_be_created() {
        let mut words = Vec::new();
        save_word(&mut words, None, "Camille", &[]).unwrap();
        assert_eq!(words, vec![word("Camille", &[])]);
    }

    #[test]
    fn a_word_is_replaced_in_place_and_can_be_renamed() {
        let mut words = vec![
            word("A", &["a1"]),
            word("Supabase", &["super base"]),
            word("Z", &[]),
        ];
        save_word(
            &mut words,
            Some("Supabase"),
            "SupaBase",
            &["super base".to_string(), "soupe à base".to_string()],
        )
        .unwrap();
        assert_eq!(
            words,
            vec![
                word("A", &["a1"]),
                word("SupaBase", &["super base", "soupe à base"]),
                word("Z", &[])
            ]
        );
    }

    #[test]
    fn saving_a_word_refuses_a_double() {
        let mut words = vec![word("Supabase", &["super base"]), word("Camille", &[])];
        let message = save_word(&mut words, None, "Supabase", &[]).unwrap_err();
        assert!(message.contains("déjà dans le dictionnaire"));
        let message =
            save_word(&mut words, None, "Autre", &["Super Base".to_string()]).unwrap_err();
        assert!(message.contains("déjà une variante de « Supabase »"));
        let message = save_word(&mut words, Some("Camille"), "Supabase", &[]).unwrap_err();
        assert!(message.contains("déjà dans le dictionnaire"));
        let message = save_word(&mut words, Some("Disparu"), "X", &[]).unwrap_err();
        assert!(message.contains("n'existe plus"));
        let message = save_word(&mut words, None, "  ", &[]).unwrap_err();
        assert!(message.contains("vide"));
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn a_word_keeps_its_own_heard_parts_when_saved_again() {
        let mut words = vec![word("Supabase", &["super base"])];
        save_word(
            &mut words,
            Some("Supabase"),
            "Supabase",
            &["super base".to_string()],
        )
        .unwrap();
        assert_eq!(words, vec![word("Supabase", &["super base"])]);
    }

    #[test]
    fn no_file_gives_an_empty_dictionary() {
        assert_eq!(load_from(&temp_file("none")), Ok(Dictionary::default()));
    }

    #[test]
    fn saved_words_are_read_back_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_file("saved");
        let dictionary = Dictionary {
            words: vec![word("Vercel", &["Versel"]), word("Camille", &[])],
            pairs: Vec::new(),
        };
        save_to(&path, &dictionary).unwrap();
        assert_eq!(load_from(&path), Ok(dictionary));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("pairs"));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_of_the_first_format_becomes_words() {
        let path = temp_file("migration");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"pairs":[{"wrong":"super base","right":"Supabase"},{"wrong":"Versel","right":"Vercel"},{"wrong":"super basse","right":"Supabase"}]}"#,
        )
        .unwrap();
        let dictionary = load_from(&path).unwrap();
        assert_eq!(
            dictionary.words,
            vec![
                word("Supabase", &["super base", "super basse"]),
                word("Vercel", &["Versel"])
            ]
        );
        assert!(dictionary.pairs.is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_broken_file_is_an_error() {
        let path = temp_file("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert!(
            load_from(&path)
                .unwrap_err()
                .starts_with("Dictionnaire illisible")
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
