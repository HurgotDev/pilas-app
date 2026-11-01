//! Pilas settings (`data_dir()/settings.json`) and the launch-at-login LaunchAgent.

use crate::error::CommandError;
use crate::panel::ScreenChoice;
use pilas_core::paths::data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// An unknown value (e.g. from a newer version) counts as `Auto`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skin {
    Classic,
    Halloween,
    Christmas,
    #[default]
    #[serde(other)]
    Auto,
}

/// Same contract as `ui/src/island/api.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub screen: ScreenChoice,
    pub sound_enabled: bool,
    pub volume: f64,
    pub open_on_pending: bool,
    pub launch_at_login: bool,
    /// `None` means the system language.
    pub lang: Option<String>,
    pub onboarded: bool,
    pub check_updates: bool,
    /// `Auto` follows the local date (see `ui/src/skins/index.ts`).
    pub skin: Skin,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            screen: ScreenChoice::default(),
            sound_enabled: true,
            volume: 0.5,
            open_on_pending: true,
            launch_at_login: true,
            lang: None,
            onboarded: false,
            check_updates: true,
            skin: Skin::default(),
        }
    }
}

fn settings_file() -> Result<PathBuf, CommandError> {
    data_dir()
        .map(|d| d.join("settings.json"))
        .ok_or(CommandError::PathUnavailable)
}

/// Missing or invalid settings fall back to the defaults.
pub fn load() -> Settings {
    settings_file()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Atomic write (temp file + rename).
pub fn save(settings: &Settings) -> Result<(), CommandError> {
    let path = settings_file()?;
    let dir = path.parent().ok_or(CommandError::PathUnavailable)?;
    fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(settings)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text + "\n")?;
    Ok(fs::rename(&tmp, &path)?)
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf, CommandError> {
    let home = std::env::var_os("HOME").ok_or(CommandError::PathUnavailable)?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/dev.pilas.app.plist"))
}

#[cfg(target_os = "macos")]
fn plist_xml(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>dev.pilas.app</string>\n\
         <key>ProgramArguments</key><array><string>{exe}</string></array>\n\
         <key>RunAtLoad</key><true/>\n\
         </dict></plist>\n"
    )
}

/// In `tauri dev` the binary lives in target/, not in a .app.
pub fn in_app_bundle() -> bool {
    std::env::current_exe()
        .map(|e| e.to_string_lossy().contains(".app/Contents/MacOS/"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn sync_launch_agent(enabled: bool) -> Result<(), CommandError> {
    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy();
    // In development, leave the user's LaunchAgent alone.
    if !in_app_bundle() {
        return Ok(());
    }
    let plist = launch_agent_path()?;
    if enabled {
        let dir = plist.parent().ok_or(CommandError::PathUnavailable)?;
        fs::create_dir_all(dir)?;
        Ok(fs::write(&plist, plist_xml(&exe))?)
    } else {
        match fs::remove_file(&plist) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sync_launch_agent(_enabled: bool) -> Result<(), CommandError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn screen_choice_matches_contract() {
        assert_eq!(serde_json::to_value(ScreenChoice::Main).unwrap(), json!("main"));
        assert_eq!(serde_json::to_value(ScreenChoice::Notch).unwrap(), json!("notch"));
        let named = ScreenChoice::Named("Studio".into());
        assert_eq!(serde_json::to_value(&named).unwrap(), json!({ "named": "Studio" }));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.2}"#).unwrap();
        assert_eq!(s.volume, 0.2);
        assert!(s.sound_enabled);
        assert!(!s.onboarded);
        assert!(s.check_updates);
        assert_eq!(s.screen, ScreenChoice::Main);
    }

    #[test]
    fn missing_or_unknown_skin_is_auto_without_losing_settings() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.2}"#).unwrap();
        assert_eq!(s.skin, Skin::Auto);
        let s: Settings = serde_json::from_str(r#"{"skin":"pirate","volume":0.2,"lang":"es"}"#).unwrap();
        assert_eq!(s.skin, Skin::Auto);
        assert_eq!(s.volume, 0.2);
        assert_eq!(s.lang.as_deref(), Some("es"));
    }

    #[test]
    fn skin_round_trips_in_snake_case() {
        for (skin, text) in [
            (Skin::Auto, "auto"),
            (Skin::Classic, "classic"),
            (Skin::Halloween, "halloween"),
            (Skin::Christmas, "christmas"),
        ] {
            assert_eq!(serde_json::to_value(skin).unwrap(), json!(text));
            assert_eq!(serde_json::from_value::<Skin>(json!(text)).unwrap(), skin);
        }
    }

    #[test]
    fn ui_ids_match_enum() {
        // Shared with the UI skin registry (ui/src/skins/index.ts).
        let ids: Vec<String> = serde_json::from_str(include_str!("../../ui/src/skins/ids.json")).unwrap();
        let mut seen = Vec::new();
        for id in &ids {
            let skin: Skin = serde_json::from_value(json!(id)).unwrap();
            assert_ne!(skin, Skin::Auto, "{id} is not a Skin variant");
            assert_eq!(serde_json::to_value(skin).unwrap(), json!(id));
            assert!(!seen.contains(&skin), "{id} repeated");
            seen.push(skin);
        }
        // Exhaustive match: a new variant does not compile until it is counted here.
        let non_auto = |s: Skin| match s {
            Skin::Classic | Skin::Halloween | Skin::Christmas => 1,
            Skin::Auto => 0,
        };
        let variants = [Skin::Classic, Skin::Halloween, Skin::Christmas, Skin::Auto];
        assert_eq!(ids.len(), variants.into_iter().map(non_auto).sum::<usize>());
    }
}
