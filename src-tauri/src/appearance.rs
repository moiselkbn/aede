//! The look of Aède the user can choose: the theme of the window (follow the Mac, light, dark)
//! and the style of the notch pill (fade, black). Kept in the settings; a change is sent to every
//! window, so that the pill (a window of its own) follows without a restart.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::settings;

/// Sent to every window when the look changes, with an `Appearance` as payload.
const CHANGED_EVENT: &str = "appearance-changed";

/// The theme of the window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follows the Mac (light or dark).
    #[default]
    Auto,
    Light,
    Dark,
}

/// The style of the notch pill (see `src/pill-style.ts` for what each one looks like).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PillStyle {
    #[default]
    Fade,
    Black,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    pub pill_style: PillStyle,
}

#[tauri::command]
pub fn get_appearance(app: AppHandle) -> Result<Appearance, String> {
    Ok(settings::load(&app)?.appearance)
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: Theme) -> Result<(), String> {
    change(&app, |appearance| appearance.theme = theme)
}

#[tauri::command]
pub fn set_pill_style(app: AppHandle, style: PillStyle) -> Result<(), String> {
    change(&app, |appearance| appearance.pill_style = style)
}

fn change(app: &AppHandle, edit: impl FnOnce(&mut Appearance)) -> Result<(), String> {
    let mut current = settings::load(app)?;
    edit(&mut current.appearance);
    settings::save(app, &current)?;
    app.emit(CHANGED_EVENT, current.appearance)
        .map_err(|err| format!("Impossible d'appliquer l'apparence : {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_follows_the_mac_with_the_fade_pill() {
        let appearance = Appearance::default();
        assert_eq!(appearance.theme, Theme::Auto);
        assert_eq!(appearance.pill_style, PillStyle::Fade);
    }

    #[test]
    fn the_front_end_sees_plain_lowercase_words() {
        let json = serde_json::to_string(&Appearance {
            theme: Theme::Dark,
            pill_style: PillStyle::Black,
        })
        .unwrap();
        assert_eq!(json, r#"{"theme":"dark","pill_style":"black"}"#);
    }
}
