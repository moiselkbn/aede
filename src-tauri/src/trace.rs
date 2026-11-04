//! A small journal of what the dictation does, kept on disk.
//!
//! It exists to understand rare bugs (a pill stuck after Escape) that cannot be caught by
//! watching the terminal. It holds phases, shortcuts and durations, never the dictated text. The
//! last lines are kept (a few hundred), the file is private to the user (0o600), it never leaves
//! the machine, and "clear all" of the history deletes it.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

const FILE_NAME: &str = "aede.log";

/// When the file has more lines than `MAX_LINES`, only the last `KEEP_LINES` are kept.
const MAX_LINES: usize = 1000;
const KEEP_LINES: usize = 500;

/// The file and the number of lines it holds. One write at a time, and "clear all" waits for it.
struct Journal {
    path: PathBuf,
    lines: usize,
}

static JOURNAL: Mutex<Option<Journal>> = Mutex::new(None);

fn journal() -> std::sync::MutexGuard<'static, Option<Journal>> {
    JOURNAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(FILE_NAME))
}

/// Starts writing the journal. Called once at startup; before it, `write` only prints.
pub fn init(app: &AppHandle) {
    match path(app) {
        Ok(path) => {
            let lines = std::fs::read_to_string(&path)
                .map(|text| text.lines().count())
                .unwrap_or(0);
            *journal() = Some(Journal { path, lines });
            write("[trace] app started");
        }
        Err(message) => eprintln!("[trace] {message}"),
    }
}

/// Prints the line in the terminal and adds it to the journal, with the time of day (UTC).
/// A journal that cannot be written never stops anything.
pub fn write(line: &str) {
    eprintln!("{line}");
    let mut guard = journal();
    let Some(journal) = guard.as_mut() else {
        return;
    };
    let stamped = format!("{} {line}", time_of_day());
    if let Err(err) = append(journal, &stamped) {
        eprintln!("[trace] could not write the journal: {err}");
    }
}

/// `trace!("...")` is `eprintln!` that also goes to the journal.
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::trace::write(&format!($($arg)*))
    };
}
pub(crate) use trace;

/// "14:03:21.482", in UTC.
fn time_of_day() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let in_day = ms % 86_400_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000
    )
}

fn append(journal: &mut Journal, line: &str) -> Result<(), String> {
    if let Some(dir) = journal.path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    if journal.lines >= MAX_LINES {
        keep_last_lines(&mut *journal)?;
    }
    // 0o600: only the user can read the file. Applies when the file is created.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&journal.path)
        .and_then(|mut file| writeln!(file, "{line}"))
        .map_err(|err| err.to_string())?;
    journal.lines += 1;
    Ok(())
}

fn keep_last_lines(journal: &mut Journal) -> Result<(), String> {
    let text = std::fs::read_to_string(&journal.path).map_err(|err| err.to_string())?;
    let all: Vec<&str> = text.lines().collect();
    let kept = &all[all.len().saturating_sub(KEEP_LINES)..];
    let mut content = kept.join("\n");
    content.push('\n');
    // Written next to the file and moved over it, so a crash never leaves half a journal.
    let temp = journal.path.with_extension("log.tmp");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .and_then(|()| std::fs::rename(&temp, &journal.path))
        .map_err(|err| err.to_string())?;
    journal.lines = kept.len();
    Ok(())
}

fn clear_at(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer le journal ({}) : {err}",
            path.display()
        )),
    }
}

/// Deletes the journal. Called by "clear all" of the history. The journal starts again at the
/// next line.
pub fn clear(app: &AppHandle) -> Result<(), String> {
    let path = path(app)?;
    let mut guard = journal();
    if let Some(journal) = guard.as_mut() {
        journal.lines = 0;
    }
    clear_at(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_journal(name: &str) -> Journal {
        let dir = std::env::temp_dir().join(format!("aede-trace-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Journal {
            path: dir.join(FILE_NAME),
            lines: 0,
        }
    }

    #[test]
    fn lines_are_added_and_the_file_is_private() {
        let mut journal = temp_journal("private");
        append(&mut journal, "one").unwrap();
        append(&mut journal, "two").unwrap();
        assert_eq!(
            std::fs::read_to_string(&journal.path).unwrap(),
            "one\ntwo\n"
        );
        let mode = std::fs::metadata(&journal.path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn only_the_last_lines_are_kept_when_the_file_gets_long() {
        let mut journal = temp_journal("rotate");
        for i in 0..=MAX_LINES {
            append(&mut journal, &format!("line {i}")).unwrap();
        }
        let text = std::fs::read_to_string(&journal.path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), KEEP_LINES + 1);
        assert_eq!(*lines.last().unwrap(), format!("line {MAX_LINES}"));
        assert_eq!(lines[0], format!("line {}", MAX_LINES - KEEP_LINES));
        let mode = std::fs::metadata(&journal.path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn clearing_removes_the_file() {
        let mut journal = temp_journal("clear");
        append(&mut journal, "one").unwrap();
        clear_at(&journal.path).unwrap();
        assert!(!journal.path.exists());
        // Nothing to remove is no error.
        clear_at(&journal.path).unwrap();
    }

    #[test]
    fn the_time_of_day_has_the_expected_shape() {
        let time = time_of_day();
        assert_eq!(time.len(), 12);
        assert_eq!(&time[2..3], ":");
        assert_eq!(&time[8..9], ".");
    }
}
