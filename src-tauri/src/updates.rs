//! Actualizaciones con `tauri-plugin-updater`: búsqueda en segundo plano, aviso en
//! el menú de la barra y en Ajustes, e instalación solo cuando la UI la pide.

use crate::{settings, AppState};
use serde::Serialize;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_updater::UpdaterExt;

/// Mismo contrato que `UpdateInfo` en `ui/src/island/api.ts`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Entrada "Update to X…" del menú de la barra; solo está en el menú si hay versión nueva.
pub struct UpdateMenu {
    pub menu: Menu<Wry>,
    pub item: MenuItem<Wry>,
}

/// Consulta el endpoint de `tauri.conf.json`. `Ok(None)` = ya está al día.
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

/// Guarda el resultado, avisa a las ventanas y actualiza el menú de la barra.
fn publish(app: &AppHandle, info: Option<UpdateInfo>) {
    app.state::<AppState>().0.lock().unwrap().update = info.clone();
    let _ = app.emit("pilas://update", &info);
    let um = app.state::<UpdateMenu>();
    let _ = um.menu.remove(&um.item); // si no estaba, no pasa nada
    if let Some(info) = &info {
        let _ = um.item.set_text(format!("Update to {}…", info.version));
        let _ = um.menu.insert(&um.item, 0);
    }
}

/// Primera búsqueda a los 10 s y después cada 24 h, si el ajuste está activo.
/// Los fallos (sin red, release sin latest.json) se ignoran: se conserva el estado anterior.
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

/// Búsqueda manual: ignora el ajuste y sí devuelve los errores a la UI.
#[tauri::command]
pub async fn check_updates_now(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let info = fetch(&app).await?;
    publish(&app, info.clone());
    Ok(info)
}

/// Descarga, instala y reinicia. Solo se ejecuta por petición explícita de la UI.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if !settings::in_app_bundle() {
        return Err("Las actualizaciones solo se instalan desde Pilas.app, no en desarrollo".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No hay ninguna actualización disponible")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}
