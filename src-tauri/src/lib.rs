mod appearance;
mod audio;
mod cancel;
mod corrections;
mod dictation;
mod dictionary;
mod download;
mod feedback;
mod field;
mod first_launch;
mod history;
mod indicator;
mod learn;
mod login_item;
mod mic_test;
mod microphone;
mod models;
mod paste;
mod permissions;
mod recordings;
mod rewrite;
mod settings;
mod shortcut;
mod stats;
mod trace;
mod transcription;
mod tray;
mod trial;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(dictation::Dictation::default())
        .manage(transcription::Transcriber::default())
        .manage(rewrite::Rewriter::default())
        .manage(download::Downloader::default())
        .manage(mic_test::MicTest::default())
        .manage(shortcut::CurrentShortcut::default())
        .manage(learn::CurrentLearnShortcut::default())
        .manage(trial::Trial::default())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // A menu bar app: no Dock icon and no place in Cmd+Tab while the window is hidden
            // (tray.rs adds them when the window opens). It is reached from its icon in the
            // menu bar.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            trace::init(app.handle());
            shortcut::register(app.handle());
            learn::register(app.handle());
            transcription::load_in_background(app.handle());
            rewrite::start(app.handle());
            tray::create(app.handle())?;
            // The indicator is a help, not a need: the dictation works without it.
            if let Err(err) = indicator::create(app.handle()) {
                eprintln!("[indicator] {err}");
            }
            first_launch::show_window_if_needed(app.handle());
            Ok(())
        })
        .on_window_event(tray::hide_instead_of_closing)
        .invoke_handler(tauri::generate_handler![
            shortcut::get_shortcut_status,
            shortcut::set_shortcut,
            shortcut::pause_shortcuts,
            shortcut::resume_shortcuts,
            learn::get_learn_shortcut_status,
            learn::set_learn_shortcut,
            dictionary::get_dictionary,
            dictionary::save_dictionary_word,
            dictionary::remove_dictionary_word,
            permissions::get_permissions_status,
            permissions::request_accessibility,
            permissions::request_microphone,
            permissions::microphone_authorization,
            permissions::open_microphone_settings,
            transcription::get_model_status,
            rewrite::get_rewrite_status,
            models::get_models_status,
            download::download_models,
            mic_test::start_mic_test,
            mic_test::stop_mic_test,
            microphone::list_microphones,
            microphone::set_microphone,
            appearance::get_appearance,
            appearance::set_theme,
            appearance::set_pill_style,
            trial::set_shortcut_test,
            trial::set_learn_test,
            learn::add_trial_word,
            trial::set_dictation_trial,
            trial::set_setup_open,
            indicator::show_notice,
            first_launch::needs_setup,
            history::get_history,
            history::clear_history,
            history::delete_entry,
            rewrite::rewrite_again,
            history::copy_text,
            recordings::get_recording,
            recordings::get_keep_recordings,
            recordings::set_keep_recordings,
            feedback::get_feedback,
            feedback::add_feedback,
            login_item::get_launch_at_login,
            login_item::set_launch_at_login,
            login_item::open_login_items_settings,
            stats::get_stats,
            stats::reset_stats
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                // Quitting stops a download in progress; the next one picks up from there.
                download::stop(app);
                // Both models must leave the GPU before the process ends, or ggml aborts.
                rewrite::stop(app);
                transcription::unload(app);
            }
            // Opening the app again (from the Finder) brings the window back. Not only when no
            // window is visible: the indicator on the notch always is.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_window(app),
            _ => {}
        });
}
