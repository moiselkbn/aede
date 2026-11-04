//! Downloads the missing model files, only when the user asks for it.
//!
//! This is the only network access of the app. It runs the curl command that comes with
//! macOS (no extra library), writes into "<file>.part", picks up a download cut halfway, and
//! checks the SHA-256 of the file before giving it its final name: a broken or altered file
//! is never used.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::models::{self, ModelFile};
use crate::{rewrite, transcription};

/// Full paths, so that the app never runs another program with the same name.
const CURL: &str = "/usr/bin/curl";
const SHASUM: &str = "/usr/bin/shasum";

/// How often the progress is sent to the UI.
const POLL: Duration = Duration::from_millis(500);

/// A download that receives nothing for this long is cut, so that the error shows quickly
/// (Wi-Fi turned off). Before the first byte of a run, the server gets longer to answer.
const STALL_AFTER: Duration = Duration::from_secs(5);
const FIRST_BYTE_WITHIN: Duration = Duration::from_secs(15);

/// While the connection is lost, the download tries again this often.
const RETRY_EVERY: Duration = Duration::from_secs(3);

/// The sentence shown when the network is gone. The ".part" file stays, so it is true.
const LOST_CONNECTION: &str = "Connexion perdue. Le reste est conservé.";
const DISK_FULL: &str = "Plus assez de place sur le disque. Le reste est conservé.";

/// Names of the events the UI listens to.
const PROGRESS_EVENT: &str = "download-progress";
const FINISHED_EVENT: &str = "download-finished";
/// Sent with `true` when the connection is lost (the download waits and tries again by itself),
/// and with `false` as soon as bytes arrive again.
const WAITING_EVENT: &str = "download-waiting";

/// Where one file is at.
#[derive(Clone, Serialize)]
pub struct Progress {
    file_name: &'static str,
    /// "download", then "check" while the SHA-256 is computed (a few seconds).
    step: &'static str,
    downloaded: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
pub struct Waiting {
    waiting: bool,
}

/// Sent once at the end: every file is there, or the reason why not.
#[derive(Clone, Serialize)]
pub struct Finished {
    error: Option<String>,
}

/// Shared between the download thread and the end of the app.
#[derive(Default)]
pub struct Downloader {
    running: AtomicBool,
    /// The curl process at work, so that quitting the app can stop it.
    curl: Mutex<Option<Child>>,
}

fn lock(curl: &Mutex<Option<Child>>) -> MutexGuard<'_, Option<Child>> {
    curl.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts downloading the missing models on their own thread and returns at once.
/// The UI follows with the progress events.
#[tauri::command]
pub fn download_models(
    app: AppHandle,
    downloader: tauri::State<'_, Downloader>,
) -> Result<(), String> {
    if downloader.running.swap(true, Ordering::SeqCst) {
        return Err("Un téléchargement est déjà en cours.".into());
    }
    let thread_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("model-download".into())
        .spawn(move || {
            let result = download_missing(&thread_app);
            // What was downloaded is loaded at once, without restarting the app. Even after an
            // error: the first file may be there already.
            transcription::reload_if_failed(&thread_app);
            rewrite::restart_if_failed(&thread_app);
            thread_app
                .state::<Downloader>()
                .running
                .store(false, Ordering::SeqCst);
            let finished = Finished {
                error: result.err(),
            };
            if let Err(err) = thread_app.emit(FINISHED_EVENT, finished) {
                eprintln!("[download] could not notify the UI: {err}");
            }
        });
    if let Err(err) = spawned {
        downloader.running.store(false, Ordering::SeqCst);
        return Err(format!("Le téléchargement ne démarre pas : {err}"));
    }
    Ok(())
}

/// Stops curl if it is running. Called when the app quits: otherwise curl would keep going
/// on its own. The ".part" file stays, and the next download picks up from there.
pub fn stop(app: &AppHandle) {
    if let Some(curl) = lock(&app.state::<Downloader>().curl).as_mut()
        && let Err(err) = curl.kill()
    {
        eprintln!("[download] could not stop curl: {err}");
    }
}

fn download_missing(app: &AppHandle) -> Result<(), String> {
    let downloader = app.state::<Downloader>();
    for model in models::ALL {
        let dest = models::path(app, model)?;
        if dest.is_file() {
            continue;
        }
        eprintln!("[download] {} -> {}", model.url, dest.display());
        let start = Instant::now();
        let mut send = |step, downloaded| {
            let progress = Progress {
                file_name: model.file_name,
                step,
                downloaded,
                total: model.size,
            };
            if let Err(err) = app.emit(PROGRESS_EVENT, progress) {
                eprintln!("[download] could not notify the UI: {err}");
            }
        };
        let mut set_waiting = |waiting| {
            if let Err(err) = app.emit(WAITING_EVENT, Waiting { waiting }) {
                eprintln!("[download] could not notify the UI: {err}");
            }
        };
        // The card of the model says which one it is: the message keeps to the reason.
        download_until_done(
            model,
            &dest,
            &downloader.curl,
            &mut send,
            &mut set_waiting,
            RETRY_EVERY,
        )
        .inspect_err(|message| {
            eprintln!("[download] {} : {message}", model.label);
        })?;
        eprintln!(
            "[download] {} ready in {:.0} s",
            model.file_name,
            start.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

/// Like `download_file`, but when the connection is lost it waits `retry_every` and starts
/// again from where the ".part" file stops, for as long as the app runs. `waiting` is called with
/// `true` at the first loss and with `false` when bytes arrive again. Any other error ends it.
fn download_until_done(
    model: &ModelFile,
    dest: &Path,
    curl: &Mutex<Option<Child>>,
    progress: &mut dyn FnMut(&'static str, u64),
    waiting: &mut dyn FnMut(bool),
    retry_every: Duration,
) -> Result<(), String> {
    // The size on disk at the moment of the loss: more than that means the connection is back.
    let mut lost_at: Option<u64> = None;
    loop {
        let result = download_file(model, dest, curl, &mut |step, bytes| {
            if lost_at.is_some_and(|at| bytes > at) {
                lost_at = None;
                waiting(false);
            }
            progress(step, bytes);
        });
        match result {
            Err(message) if message == LOST_CONNECTION => {
                if lost_at.is_none() {
                    waiting(true);
                }
                lost_at = Some(size_on_disk(&part_path(dest)));
                std::thread::sleep(retry_every);
            }
            other => return other,
        }
    }
}

/// Downloads `model` to `dest`, through `dest.part`. Calls `progress` with the step and the
/// number of bytes on disk.
fn download_file(
    model: &ModelFile,
    dest: &Path,
    curl: &Mutex<Option<Child>>,
    progress: &mut dyn FnMut(&'static str, u64),
) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    let part = part_path(dest);
    // Bigger than expected: it is not a piece of this file, start again.
    if size_on_disk(&part) > model.size {
        remove(&part)?;
    }
    if size_on_disk(&part) < model.size {
        run_curl(model.url, &part, curl, &mut |bytes| {
            progress("download", bytes)
        })?;
    }

    let received = size_on_disk(&part);
    if received != model.size {
        return Err(format!(
            "Fichier incomplet ({received} octets sur {}). Relance le téléchargement pour le reprendre.",
            model.size
        ));
    }
    progress("check", received);
    if sha256_of(&part)? != model.sha256 {
        remove(&part)?;
        return Err(
            "Le fichier reçu n'est pas celui attendu (empreinte SHA-256 différente). \
                    Il a été supprimé, relance le téléchargement."
                .into(),
        );
    }
    std::fs::rename(&part, dest).map_err(|err| {
        format!(
            "Impossible de renommer {} en {} : {err}",
            part.display(),
            dest.display()
        )
    })
}

/// Runs curl until it ends, sending the size of the file on disk every half second.
fn run_curl(
    url: &str,
    part: &Path,
    curl: &Mutex<Option<Child>>,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let child = Command::new(CURL)
        .args([
            "--fail",        // an HTTP error is an error, not a file
            "--location",    // Hugging Face redirects to its file servers
            "--silent",      // no progress bar on the terminal
            "--show-error",  // but the error message, if any
            "--continue-at", // resume from the end of the ".part" file
            "-",
            "--connect-timeout",
            "15",
            // Give up when less than 1 kB/s arrives for 15 s: a stuck connection (Wi-Fi cut).
            "--speed-limit",
            "1000",
            "--speed-time",
            "15",
            "--output",
        ])
        .arg(part)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("curl ne démarre pas : {err}"))?;
    *lock(curl) = Some(child);

    let mut last_size = size_on_disk(part);
    let mut last_change = Instant::now();
    let mut got_data = false;
    let status = loop {
        let finished = match lock(curl).as_mut() {
            Some(child) => child.try_wait(),
            None => return Err("Téléchargement interrompu.".into()),
        };
        match finished {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => return Err(format!("curl ne répond plus : {err}")),
        }
        let size = size_on_disk(part);
        if size != last_size {
            last_size = size;
            last_change = Instant::now();
            got_data = true;
        } else if last_change.elapsed()
            > if got_data {
                STALL_AFTER
            } else {
                FIRST_BYTE_WITHIN
            }
        {
            // Nothing arrives: stop curl ourselves instead of waiting for its own timeout.
            if let Some(mut stuck) = lock(curl).take() {
                let _ = stuck.kill();
                let _ = stuck.wait();
            }
            return Err(LOST_CONNECTION.into());
        }
        progress(size);
        std::thread::sleep(POLL);
    };
    let child = lock(curl).take();
    progress(size_on_disk(part));
    if status.success() {
        return Ok(());
    }
    let mut detail = String::new();
    if let Some(mut stderr) = child.and_then(|child| child.stderr) {
        // Only to enrich the message: without it the reason below is still shown.
        let _ = stderr.read_to_string(&mut detail);
    }
    Err(curl_error(status.code(), detail.trim()))
}

/// Turns the most common curl exit codes into a sentence for the user. The raw curl message
/// goes to the log only: it is not written for the user.
fn curl_error(code: Option<i32>, detail: &str) -> String {
    eprintln!("[download] curl exit code {code:?}: {detail}");
    // A full disk comes as code 23, or as code 56 with a message about writing the file.
    let writing_failed = detail.to_lowercase().contains("writing");
    match code {
        Some(23) => DISK_FULL.into(),
        Some(_) if writing_failed => DISK_FULL.into(),
        // 56 is a failure while receiving: the connection broke.
        Some(6 | 7 | 28 | 56) => LOST_CONNECTION.into(),
        Some(22) => "Le serveur refuse de donner le fichier.".into(),
        None => "Téléchargement arrêté.".into(),
        Some(_) => "Téléchargement impossible. Réessaie avec « Reprendre ».".into(),
    }
}

/// The SHA-256 of a file, computed by the shasum command of macOS.
fn sha256_of(path: &Path) -> Result<String, String> {
    let output = Command::new(SHASUM)
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(|err| format!("shasum ne démarre pas : {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "Vérification du fichier impossible : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    // The output is "<hash>  <path>".
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// 0 when the file does not exist.
fn size_on_disk(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn remove(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path)
        .map_err(|err| format!("Impossible de supprimer {} : {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 MB of known content, and its SHA-256.
    const CONTENT_SHA256: &str = "ec21d64624228af3ecd4bdaa8239e32ed943b01e26934cd5610fddb361426dc6";

    fn content() -> Vec<u8> {
        b"0123456789".repeat(100_000)
    }

    /// A fresh folder with a source file, served through a file:// address.
    fn setup(name: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("aede-download-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.bin");
        std::fs::write(&source, content()).unwrap();
        (dir, format!("file://{}", source.display()))
    }

    fn model(url: &str, sha256: &'static str) -> ModelFile {
        ModelFile {
            label: "test",
            file_name: "model.bin",
            url: url.to_string().leak(),
            size: content().len() as u64,
            sha256,
            env_var: "AEDE_TEST_UNUSED",
        }
    }

    fn download(model: &ModelFile, dest: &Path) -> (Result<(), String>, Vec<&'static str>) {
        let mut steps = Vec::new();
        let result = download_file(model, dest, &Mutex::new(None), &mut |step, _| {
            if steps.last() != Some(&step) {
                steps.push(step);
            }
        });
        (result, steps)
    }

    #[test]
    fn downloads_checks_and_renames_the_file() {
        let (dir, url) = setup("ok");
        let dest = dir.join("models/model.bin");
        let (result, steps) = download(&model(&url, CONTENT_SHA256), &dest);
        assert_eq!(result, Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), content());
        assert!(!part_path(&dest).exists());
        assert_eq!(steps, ["download", "check"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resumes_a_download_cut_halfway() {
        let (dir, url) = setup("resume");
        let dest = dir.join("model.bin");
        std::fs::write(part_path(&dest), &content()[..400_000]).unwrap();
        let (result, _) = download(&model(&url, CONTENT_SHA256), &dest);
        assert_eq!(result, Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), content());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_complete_part_file_is_only_checked() {
        let (dir, _) = setup("complete");
        let dest = dir.join("model.bin");
        std::fs::write(part_path(&dest), content()).unwrap();
        // The address does not exist: curl must not run.
        let (result, steps) = download(&model("file:///nonexistent", CONTENT_SHA256), &dest);
        assert_eq!(result, Ok(()));
        assert_eq!(steps, ["check"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_with_the_wrong_sha256_is_deleted() {
        let (dir, url) = setup("sha");
        let dest = dir.join("model.bin");
        let (result, _) = download(&model(&url, "0000"), &dest);
        assert!(result.unwrap_err().contains("SHA-256"));
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_source_gives_a_clear_error() {
        let (dir, _) = setup("missing");
        let dest = dir.join("model.bin");
        let (result, _) = download(&model("file:///nonexistent/x.bin", CONTENT_SHA256), &dest);
        let message = result.unwrap_err();
        assert!(
            message.starts_with("Téléchargement impossible"),
            "{message}"
        );
        assert!(!dest.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A server that sends 100 kB and then says nothing, like a Wi-Fi turned off: the download
    /// must stop by itself after a few seconds, keep what arrived and say the connection is lost.
    #[test]
    fn a_stalled_connection_stops_quickly_and_keeps_what_arrived() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                content().len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&content()[..100_000]).unwrap();
            stream.flush().unwrap();
            // Silence, until the client has given up and closed.
            let mut rest = Vec::new();
            let _ = stream.read_to_end(&mut rest);
        });

        let dir = std::env::temp_dir().join(format!("aede-download-stall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("model.bin");
        let start = Instant::now();
        let (result, _) = download(&model(&url, CONTENT_SHA256), &dest);

        assert_eq!(result, Err(LOST_CONNECTION.to_string()));
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "took {:?}",
            start.elapsed()
        );
        // curl writes by blocks, so not exactly the 100 kB sent, but the start is kept.
        let kept = size_on_disk(&part_path(&dest));
        assert!(kept > 0 && kept <= 100_000, "{kept} bytes kept");
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The connection drops once, then comes back: the download finishes without any click.
    #[test]
    fn a_download_resumes_by_itself_when_the_connection_comes_back() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            // First connection: 100 kB, then silence until the client leaves.
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                content().len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&content()[..100_000]).unwrap();
            stream.flush().unwrap();
            let mut rest = Vec::new();
            let _ = stream.read_to_end(&mut rest);
            // Second connection: curl asks for the rest with a Range header.
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let n = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]).to_lowercase();
            let from: usize = request
                .split("range: bytes=")
                .nth(1)
                .and_then(|r| r.split('-').next())
                .and_then(|r| r.trim().parse().ok())
                .expect("the retry must resume with a Range header");
            let body = &content()[from..];
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\n\r\n",
                body.len(),
                content().len() - 1,
                content().len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });

        let dir = std::env::temp_dir().join(format!("aede-download-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("model.bin");
        let mut changes = Vec::new();
        let result = download_until_done(
            &model(&url, CONTENT_SHA256),
            &dest,
            &Mutex::new(None),
            &mut |_, _| {},
            &mut |waiting| changes.push(waiting),
            Duration::from_millis(200),
        );

        assert_eq!(result, Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), content());
        assert_eq!(changes, [true, false]);
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn common_curl_codes_are_explained() {
        assert_eq!(curl_error(Some(6), ""), LOST_CONNECTION);
        assert_eq!(curl_error(Some(28), "Operation too slow"), LOST_CONNECTION);
        assert_eq!(
            curl_error(
                Some(56),
                "curl: (56) Failure writing output to destination, passed 16375 returned 2551"
            ),
            DISK_FULL
        );
        assert_eq!(curl_error(Some(23), ""), DISK_FULL);
        assert_eq!(
            curl_error(
                Some(56),
                "curl: (56) Recv failure: Connection reset by peer"
            ),
            LOST_CONNECTION
        );
        // An unknown failure never shows the raw message of curl.
        let message = curl_error(Some(99), "curl: (99) something technical");
        assert!(!message.contains("curl"), "{message}");
        assert!(curl_error(Some(23), "x").contains("Plus assez de place"));
        assert!(!curl_error(Some(99), "x").contains(": x"));
    }

    /// Downloads the real Whisper model (1.6 GB) from Hugging Face into a temporary folder,
    /// to check the pinned address, the size and the SHA-256. Skipped by default.
    /// Run it with: cargo test -- --ignored --nocapture downloads_the_real_whisper_model
    #[test]
    #[ignore]
    fn downloads_the_real_whisper_model() {
        let dir = std::env::temp_dir().join("aede-download-real");
        let dest = dir.join(models::WHISPER.file_name);
        let _ = std::fs::remove_file(&dest);
        let start = Instant::now();
        let mut last = 0;
        let result = download_file(
            &models::WHISPER,
            &dest,
            &Mutex::new(None),
            &mut |step, bytes| {
                if bytes / 100_000_000 != last / 100_000_000 || step == "check" {
                    eprintln!("{step} {} MB", bytes / 1_000_000);
                }
                last = bytes;
            },
        );
        eprintln!("{:.0} s", start.elapsed().as_secs_f64());
        assert_eq!(result, Ok(()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
