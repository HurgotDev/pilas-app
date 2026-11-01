//! Updates via `tauri-plugin-updater`: background checks, a notice in the menu bar
//! and in Settings, and installing only when the UI asks for it.

use crate::{settings, AppState};
use serde::Serialize;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_updater::UpdaterExt;

/// Same contract as `UpdateInfo` in `ui/src/island/api.ts`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// The "Update to X…" item is only in the menu while a new version exists.
pub struct UpdateMenu {
    pub menu: Menu<Wry>,
    pub item: MenuItem<Wry>,
}

/// `Ok(None)` means up to date.
async fn fetch(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version,
        notes: u.body,
    }))
}

fn publish(app: &AppHandle, info: Option<UpdateInfo>) {
    app.state::<AppState>().0.lock().unwrap().update = info.clone();
    let _ = app.emit("pilas://update", &info);
    let um = app.state::<UpdateMenu>();
    let _ = um.menu.remove(&um.item); // absent is fine
    if let Some(info) = &info {
        let _ = um.item.set_text(format!("Update to {}…", info.version));
        let _ = um.menu.insert(&um.item, 0);
    }
}

/// First check after 10 s, then every 24 h, while the setting is on.
/// Failures (no network, release without latest.json) are ignored and keep the previous state.
pub fn spawn_checks(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        loop {
            if settings::load().check_updates {
                tauri::async_runtime::block_on(async {
                    if let Ok(info) = fetch(&app).await {
                        publish(&app, info);
                    }
                });
            }
            std::thread::sleep(Duration::from_secs(24 * 60 * 60));
        }
    });
}

#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> Option<UpdateInfo> {
    state.0.lock().unwrap().update.clone()
}

/// Manual check: ignores the setting and does return errors to the UI.
#[tauri::command]
pub async fn check_updates_now(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let info = fetch(&app).await?;
    publish(&app, info.clone());
    Ok(info)
}

/// Only runs on an explicit request from the UI.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if !settings::in_app_bundle() {
        return Err("Updates can only be installed from Pilas.app, not in development".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No update is available")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}
