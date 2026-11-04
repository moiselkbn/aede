//! The two model files: where they live on disk and where they come from.
//!
//! They sit in the app data folder, named after the bundle identifier:
//! ~/Library/Application Support/com.lukebanumoise.aede/models/
//! An environment variable can point to another file while developing.
//!
//! The download addresses are pinned to one revision on Hugging Face, and the size and
//! SHA-256 are those of the files measured in prototypes 2 and 3: a file that changes on the
//! server is refused instead of silently replacing the measured model.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// One model file the app needs.
pub struct ModelFile {
    /// Shown to the user.
    pub label: &'static str,
    /// Name of the file inside the models folder.
    pub file_name: &'static str,
    pub url: &'static str,
    /// Size in bytes.
    pub size: u64,
    pub sha256: &'static str,
    /// Overrides the location of this file (development only).
    pub env_var: &'static str,
}

/// Speech to text, chosen in prototype 2 (docs/prototype-2.md).
pub const WHISPER: ModelFile = ModelFile {
    label: "Modèle de transcription (Whisper large-v3-turbo)",
    file_name: "ggml-large-v3-turbo.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
    size: 1_624_555_275,
    sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    env_var: "AEDE_MODEL_PATH",
};

/// Rewriting, chosen in prototype 3 (docs/prototype-3.md).
pub const LLM: ModelFile = ModelFile {
    label: "Modèle de reformulation (Qwen3 4B Instruct)",
    file_name: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    url: "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/a06e946bb6b655725eafa393f4a9745d460374c9/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    size: 2_497_281_120,
    sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
    env_var: "AEDE_LLM_PATH",
};

/// Every model the app needs, in download order (the smaller first).
pub const ALL: [&ModelFile; 2] = [&WHISPER, &LLM];

/// The folder that holds the model files.
pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join("models"))
}

/// Where `model` is: the environment variable if set, the models folder otherwise.
pub fn path(app: &AppHandle, model: &ModelFile) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(model.env_var) {
        return Ok(PathBuf::from(path));
    }
    Ok(dir(app)?.join(model.file_name))
}

/// What the UI shows about one model file.
#[derive(Serialize)]
pub struct ModelInfo {
    label: &'static str,
    file_name: &'static str,
    size: u64,
    /// The file is on disk. A downloaded file is only given its final name once its SHA-256
    /// is checked, so being there is enough: it is not checked again at every start.
    present: bool,
}

/// Lets the UI know which models are on disk and which ones need downloading.
#[tauri::command]
pub fn get_models_status(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    ALL.iter()
        .map(|model| {
            Ok(ModelInfo {
                label: model.label,
                file_name: model.file_name,
                size: model.size,
                present: path(&app, model)?.is_file(),
            })
        })
        .collect()
}

/// Same as `path`, for tests that run without the app: the folder is rebuilt from the
/// identifier written in tauri.conf.json, so it is not repeated in the code.
#[cfg(test)]
pub fn path_for_tests(model: &ModelFile) -> PathBuf {
    if let Some(path) = std::env::var_os(model.env_var) {
        return PathBuf::from(path);
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let identifier = config["identifier"].as_str().unwrap();
    PathBuf::from(std::env::var_os("HOME").unwrap())
        .join("Library/Application Support")
        .join(identifier)
        .join("models")
        .join(model.file_name)
}
