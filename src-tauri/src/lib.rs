//! App Pilas (Tauri 2). Responsabilidades de este crate:
//! - servidor del socket local (ver `server`), que alimenta el `SessionStore` de pilas-core,
//! - cola FIFO de solicitudes bloqueantes y entrega de decisiones al hook,
//! - comandos para la WebView y eventos hacia ella,
//! - (M1) comportamiento de la ventana como panel del notch.

mod hooks;
mod panel;
mod server;
mod settings;
mod updates;

use pilas_core::cache::{self, CacheClock, CacheTemp};
use pilas_core::decision::{self, Answers, PermissionChoice};
use pilas_core::protocol::AppReply;
use pilas_core::session::{now_ms, MascotState, Session, SessionStore, StoreEvent};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// Qué le llega al hilo de una conexión que espera decisión.
pub(crate) enum Outcome {
    Reply(AppReply),
    /// El hook se desconectó (p. ej. el usuario respondió en la terminal).
    HookGone,
}

pub(crate) struct PendingRequest {
    pub session_id: String,
    pub event: String,
    pub payload: Value,
    pub tx: mpsc::Sender<Outcome>,
}

#[derive(Default)]
pub(crate) struct Inner {
    pub store: SessionStore,
    pub pending: HashMap<String, PendingRequest>,
    /// Ids de solicitud en orden de llegada.
    pub queue: VecDeque<String>,
    /// Ajustes vigentes: se cargan al arrancar y se reemplazan en `set_settings`.
    pub settings: settings::Settings,
    pub mode: panel::IslandMode,
    pub open_height: Option<f64>,
    /// Sesión → id de terminal de Ghostty, guardado tras un salto exacto.
    // shortcut: las entradas de sesiones terminadas no se borran, son pocos bytes; limpiar si crece
    pub terminal_ids: HashMap<String, String>,
    /// Última versión nueva encontrada (`None` = al día o sin comprobar).
    pub update: Option<updates::UpdateInfo>,
    /// TTL forzado con `PILAS_CACHE_TTL_SECS` (solo en compilaciones de depuración).
    pub cache_ttl_override: Option<u64>,
}

#[derive(Default)]
pub struct AppState(pub(crate) Mutex<Inner>);

#[derive(Serialize, Clone)]
pub struct PendingView {
    pub request_id: String,
    pub session_id: String,
    pub project: String,
    /// "permission" | "question"
    pub kind: &'static str,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub permission_suggestions: Option<Value>,
}

/// Estado de caché de una sesión para la UI.
#[derive(Serialize, Clone)]
pub struct CacheView {
    pub temp: CacheTemp,
    pub expires_ms: Option<u64>,
    pub ttl_s: u64,
    pub ttl_detected: bool,
}

#[derive(Serialize, Clone)]
pub struct SessionView {
    #[serde(flatten)]
    pub session: Session,
    pub cache: Option<CacheView>,
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub sessions: Vec<SessionView>,
    pub mascot: MascotState,
    pub pending: Vec<PendingView>,
    /// Peor estado accionable entre las sesiones (`enfriandose` > `fria` > `caliente`).
    pub cache_mascot: Option<CacheTemp>,
}

/// Vista de caché de un reloj en `now`; `None` si no hay estado que mostrar.
fn cache_view(clock: &CacheClock, now: u64, ttl_override: Option<u64>, warn_s: u64) -> Option<CacheView> {
    let clock = CacheClock {
        ttl_s: ttl_override.or(clock.ttl_s),
        ttl_detected: clock.ttl_detected && ttl_override.is_none(),
        ..clock.clone()
    };
    Some(CacheView {
        temp: cache::temperature(&clock, now, warn_s)?,
        expires_ms: cache::expires_ms(&clock),
        ttl_s: cache::ttl_of(&clock),
        ttl_detected: clock.ttl_detected,
    })
}

/// Rango de lo accionable; `None` para estados que no avisan (compactada).
fn actionable_rank(temp: CacheTemp) -> Option<u8> {
    match temp {
        CacheTemp::Caliente => Some(0),
        CacheTemp::Fria => Some(1),
        CacheTemp::Enfriandose => Some(2),
        CacheTemp::Compactada => None,
    }
}

pub(crate) fn snapshot(inner: &Inner) -> Snapshot {
    let pending = inner
        .queue
        .iter()
        .filter_map(|rid| {
            let p = inner.pending.get(rid)?;
            Some(PendingView {
                request_id: rid.clone(),
                session_id: p.session_id.clone(),
                project: inner
                    .store
                    .get(&p.session_id)
                    .map(|s| s.project.clone())
                    .unwrap_or_default(),
                kind: if p.event == "PreToolUse" {
                    "question"
                } else {
                    "permission"
                },
                tool_name: p.payload.get("tool_name").and_then(Value::as_str).map(str::to_string),
                tool_input: p.payload.get("tool_input").cloned(),
                permission_suggestions: p.payload.get("permission_suggestions").cloned(),
            })
        })
        .collect();
    let now = now_ms();
    let ttl_override = inner.cache_ttl_override;
    let enabled = inner.settings.cache_enabled;
    let warn_s = u64::from(inner.settings.cache_warn_min) * 60;
    let sessions: Vec<SessionView> = inner
        .store
        .list()
        .into_iter()
        .map(|s| SessionView {
            cache: if enabled {
                cache_view(&s.cache, now, ttl_override, warn_s)
            } else {
                None
            },
            session: s.clone(),
        })
        .collect();
    let cache_mascot = sessions
        .iter()
        .filter_map(|s| s.cache.as_ref().map(|c| c.temp))
        .filter_map(|t| actionable_rank(t).map(|r| (r, t)))
        .max_by_key(|(r, _)| *r)
        .map(|(_, t)| t);
    Snapshot {
        sessions,
        mascot: inner.store.mascot_state(),
        pending,
        cache_mascot,
    }
}

pub(crate) fn emit_snapshot(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = snapshot(&state.0.lock().unwrap());
    let _ = app.emit("pilas://snapshot", snap);
}

/// Saca una solicitud de la cola y le entrega la respuesta a su conexión.
fn deliver(
    app: &AppHandle,
    state: &AppState,
    request_id: &str,
    build: impl FnOnce(&PendingRequest) -> Option<Value>,
) -> Result<(), String> {
    {
        let mut inner = state.0.lock().unwrap();
        let req = inner.pending.remove(request_id).ok_or("la solicitud ya no existe")?;
        inner.queue.retain(|r| r != request_id);
        let stdout = build(&req);
        let _ = req.tx.send(Outcome::Reply(AppReply::new(request_id, stdout)));
        inner.store.resolve_pending(&req.session_id);
    }
    emit_snapshot(app);
    Ok(())
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    snapshot(&state.0.lock().unwrap())
}

#[tauri::command]
fn resolve_permission(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    choice: PermissionChoice,
) -> Result<(), String> {
    deliver(&app, &state, &request_id, |req| {
        decision::permission_output(&choice, &req.payload)
    })
}

/// `answers = None` → responder en la terminal.
#[tauri::command]
fn answer_question(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    answers: Option<Answers>,
) -> Result<(), String> {
    deliver(&app, &state, &request_id, |req| match &answers {
        Some(a) => decision::question_output(&req.payload, a),
        None => Some(decision::question_to_terminal()),
    })
}

/// La UI avisa del modo visible (y de la altura de la tarjeta abierta) para dimensionar la ventana.
#[tauri::command]
fn set_island_mode(app: AppHandle, state: State<'_, AppState>, mode: panel::IslandMode, open_height: Option<f64>) {
    {
        let mut inner = state.0.lock().unwrap();
        inner.mode = mode;
        inner.open_height = open_height;
    }
    apply_layout(&app);
}

/// Recoloca la isla con la pantalla, el modo y la altura guardados.
fn apply_layout(app: &AppHandle) {
    let (screen, mode, open_height) = {
        let state = app.state::<AppState>();
        let inner = state.0.lock().unwrap();
        (inner.settings.screen.clone(), inner.mode, inner.open_height)
    };
    panel::place(app, screen, mode, open_height);
}

#[tauri::command]
fn jump_to_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let target = {
        let inner = state.0.lock().unwrap();
        let s = inner.store.get(&session_id).ok_or("sesión desconocida")?;
        pilas_jump::JumpTarget {
            term_program: s.term_program.clone(),
            tty: s.tty.clone(),
            pid: s.pid,
            cwd: Some(s.cwd.clone()),
            cached_terminal_id: inner.terminal_ids.get(&session_id).cloned(),
        }
    };
    let had_cache = target.cached_terminal_id.is_some();
    let outcome = pilas_jump::jumper_for(target.term_program.as_deref())
        .jump(&target)
        .map_err(|e| e.to_string())?;
    let mut inner = state.0.lock().unwrap();
    match outcome {
        pilas_jump::JumpOutcome::Exact { terminal_id: Some(id) } => {
            inner.terminal_ids.insert(session_id, id);
        }
        pilas_jump::JumpOutcome::AppOnly if had_cache => {
            inner.terminal_ids.remove(&session_id);
        }
        _ => {}
    }
    Ok(())
}

/// El proceso sigue existiendo (señal 0; EPERM también cuenta: existe, sin permiso).
fn pid_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

type CacheTemps = Vec<(String, Option<CacheTemp>)>;

/// Sesiones que en `current` están en `enfriandose` y en `prev` no lo estaban (un aviso por episodio).
fn newly_cooling(prev: &CacheTemps, current: &CacheTemps) -> Vec<String> {
    let cooling = |temps: &CacheTemps, id: &str| {
        temps
            .iter()
            .any(|(sid, t)| sid == id && *t == Some(CacheTemp::Enfriandose))
    };
    current
        .iter()
        .filter(|(id, _)| cooling(current, id) && !cooling(prev, id))
        .map(|(id, _)| id.clone())
        .collect()
}

/// Cada 5 s recalcula la temperatura de las cachés, emite el snapshot solo si algún estado cambió
/// y `pilas://cache-cooling` por cada sesión que acaba de pasar a `enfriandose`.
fn spawn_cache_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<(CacheTemps, Option<CacheTemp>)> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let current = {
                let state = app.state::<AppState>();
                let inner = state.0.lock().unwrap();
                let snap = snapshot(&inner);
                let temps: Vec<_> = snap
                    .sessions
                    .iter()
                    .map(|s| (s.session.session_id.clone(), s.cache.as_ref().map(|c| c.temp)))
                    .collect();
                (temps, snap.cache_mascot)
            };
            let cooling = newly_cooling(last.as_ref().map_or(&Vec::new(), |l| &l.0), &current.0);
            if last.as_ref() != Some(&current) {
                last = Some(current);
                emit_snapshot(&app); // después de soltar el lock: emit_snapshot lo toma
            }
            for id in cooling {
                let _ = app.emit("pilas://cache-cooling", id);
            }
        }
    });
}

/// Lee la cola del transcript tras `delay_ms` y la aplica al reloj de la sesión.
/// Un fallo de IO no cambia nada; el contenido nunca se guarda ni se registra.
pub(crate) fn spawn_transcript_read(app: AppHandle, session_id: String, path: String, delay_ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        let Some(text) = read_tail(&path, 512 * 1024) else {
            return;
        };
        let parsed = cache::parse_transcript_tail(&text);
        {
            let state = app.state::<AppState>();
            let mut inner = state.0.lock().unwrap();
            let Some(s) = inner.store.get_mut(&session_id) else {
                return;
            };
            cache::apply_transcript(&mut s.cache, &parsed);
        }
        emit_snapshot(&app); // después de soltar el lock: emit_snapshot lo toma
    });
}

/// Últimos `max` bytes de un archivo como texto (UTF-8 con pérdida).
fn read_tail(path: &str, max: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// `PILAS_CACHE_TTL_SECS` fuerza el TTL de todas las sesiones; solo en depuración.
fn cache_ttl_override() -> Option<u64> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("PILAS_CACHE_TTL_SECS").ok()?.parse().ok()
}

/// Cada 30 s quita las sesiones cuyo proceso ya no existe (sin SessionEnd).
fn spawn_prune(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let changed = {
            let state = app.state::<AppState>();
            let mut inner = state.0.lock().unwrap();
            let removed: Vec<String> = inner
                .store
                .prune(pid_alive)
                .into_iter()
                .filter_map(|e| match e {
                    StoreEvent::Removed(id) => Some(id),
                    _ => None,
                })
                .collect();
            // Las solicitudes de esas sesiones ya no tienen a quién responder.
            let gone: Vec<String> = inner
                .pending
                .iter()
                .filter(|(_, req)| removed.contains(&req.session_id))
                .map(|(rid, _)| rid.clone())
                .collect();
            for rid in gone {
                if let Some(req) = inner.pending.remove(&rid) {
                    let _ = req.tx.send(Outcome::HookGone);
                }
                inner.queue.retain(|r| r != &rid);
            }
            !removed.is_empty()
        };
        if changed {
            emit_snapshot(&app); // después de soltar el lock: emit_snapshot lo toma
        }
    });
}

#[tauri::command]
fn get_settings() -> settings::Settings {
    settings::load()
}

/// Persiste, recoloca la isla, sincroniza el LaunchAgent y avisa a todas las ventanas
/// (también con un snapshot nuevo: los ajustes de caché cambian lo que muestra).
#[tauri::command]
fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: settings::Settings) -> Result<(), String> {
    settings::save(&settings)?;
    state.0.lock().unwrap().settings = settings.clone();
    apply_layout(&app);
    emit_snapshot(&app);
    settings::sync_launch_agent(settings.launch_at_login)?;
    let _ = app.emit("pilas://settings", &settings);
    Ok(())
}

/// Async a propósito: un comando síncrono corre en el hilo principal y bloquearía `screen_names`.
#[tauri::command]
async fn list_screens(app: AppHandle) -> Vec<String> {
    panel::screen_names(&app)
}

#[tauri::command]
fn hooks_status() -> hooks::HooksInfo {
    hooks::info()
}

#[tauri::command]
fn preview_hooks(action: String) -> Result<Vec<pilas_core::installer::DiffLine>, String> {
    hooks::preview(hooks::Action::parse(&action)?)
}

/// Devuelve la ruta del backup si se escribió `settings.json` y ya existía.
#[tauri::command]
fn apply_hooks(action: String) -> Result<Option<String>, String> {
    let backup = hooks::apply(hooks::Action::parse(&action)?)?;
    Ok(backup.map(|p| p.display().to_string()))
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    open_settings_window(&app).map_err(|e| e.to_string())
}

/// Muestra la ventana de ajustes, o la crea si no existe.
fn open_settings_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("settings").is_some() {
        panel::bring_to_front(app, "settings");
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Pilas")
        .inner_size(520.0, 560.0)
        .resizable(false)
        .visible(false)
        .center()
        .build()?;
    panel::bring_to_front(app, "settings");
    Ok(())
}

/// Lado en píxeles del icono de la barra de menú (18 pt a 2x).
const TRAY_SIZE: usize = 36;
/// Margen en píxeles entre la mascota y el borde del icono.
const TRAY_MARGIN: f64 = 1.0;
/// Submuestras por eje para el antialias (4x4 da 17 niveles de cobertura).
const TRAY_SUB: usize = 4;
/// Ojos un 20 % mayores que en el SVG: a 18 pt los agujeros se perderían.
const EYE_SCALE: f64 = 1.2;
/// Caja de la silueta en unidades del viewBox (144x156) de `ui/src/mascot/pilita.ts`:
/// x0, y0, x1, y1 (sin contar el brazo, que no cabe en el icono).
const PILITA_BOX: (f64, f64, f64, f64) = (22.0, 18.0, 118.0, 151.0);

/// Escala y desplazamiento (s, ox, oy) con los que px = ox + u * s, py = oy + v * s.
fn tray_map() -> (f64, f64, f64) {
    let (x0, y0, x1, y1) = PILITA_BOX;
    let size = TRAY_SIZE as f64;
    let s = (size - 2.0 * TRAY_MARGIN) / (y1 - y0);
    let ox = (size - (x1 - x0) * s) / 2.0 - x0 * s;
    let oy = TRAY_MARGIN - y0 * s;
    (s, ox, oy)
}

/// ¿Cae el punto (u, v) del viewBox dentro de la pilita, fuera de los ojos?
fn pilita_contains(u: f64, v: f64) -> bool {
    // Rectángulo redondeado (x0, y0, x1, y1, radio), igual que el `rect` del SVG.
    let in_rect = |(x0, y0, x1, y1, r): (f64, f64, f64, f64, f64)| {
        let dx = ((u - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - r)).max(0.0);
        let dy = ((v - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - r)).max(0.0);
        dx * dx + dy * dy <= r * r
    };
    let in_ellipse = |cx: f64, cy: f64, rx: f64, ry: f64| ((u - cx) / rx).powi(2) + ((v - cy) / ry).powi(2) <= 1.0;
    let body = in_rect((22.0, 30.0, 118.0, 142.0, 38.0)) || in_rect((55.0, 18.0, 85.0, 36.0, 7.0));
    let feet = in_ellipse(52.0, 144.0, 11.0, 7.0) || in_ellipse(88.0, 144.0, 11.0, 7.0);
    let eye = |cx: f64| in_ellipse(cx, 72.0, 9.0 * EYE_SCALE, 12.0 * EYE_SCALE);
    (body || feet) && !eye(52.0) && !eye(88.0)
}

/// Alfa de cada píxel del icono: cobertura de la silueta con supermuestreo.
fn tray_alpha() -> Vec<u8> {
    let (s, ox, oy) = tray_map();
    let n = (TRAY_SUB * TRAY_SUB) as u32;
    let mut alpha = vec![0u8; TRAY_SIZE * TRAY_SIZE];
    for py in 0..TRAY_SIZE {
        for px in 0..TRAY_SIZE {
            let mut inside = 0u32;
            for sy in 0..TRAY_SUB {
                for sx in 0..TRAY_SUB {
                    let fx = px as f64 + (sx as f64 + 0.5) / TRAY_SUB as f64;
                    let fy = py as f64 + (sy as f64 + 0.5) / TRAY_SUB as f64;
                    inside += u32::from(pilita_contains((fx - ox) / s, (fy - oy) / s));
                }
            }
            alpha[py * TRAY_SIZE + px] = (inside * 255 / n) as u8;
        }
    }
    alpha
}

/// Icono de plantilla de la pilita: macOS usa solo el alfa, así que el color da igual.
fn tray_icon() -> Image<'static> {
    let rgba: Vec<u8> = tray_alpha().into_iter().flat_map(|a| [0, 0, 0, a]).collect();
    Image::new_owned(rgba, TRAY_SIZE as u32, TRAY_SIZE as u32)
}

/// Icono de la barra de menú: abrir ajustes o salir.
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    // shortcut: etiquetas en inglés porque el backend no tiene i18n; traducir cuando haya más textos nativos
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Pilas", true, None::<&str>)?;
    // "Update to X…" se inserta arriba en updates::publish cuando hay versión nueva.
    let update_item = MenuItem::with_id(app, "update", "Update…", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;
    app.manage(updates::UpdateMenu {
        menu: menu.clone(),
        item: update_item,
    });
    TrayIconBuilder::new()
        .icon(tray_icon())
        .menu(&menu)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" | "update" => {
                let _ = open_settings_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // sin ícono en el Dock

            let settings = settings::load();
            app.state::<AppState>().0.lock().unwrap().settings = settings.clone();

            if let Some(window) = app.get_webview_window("island") {
                panel::make_panel(&window);
            }
            apply_layout(app.handle());
            let handle = app.handle().clone();
            panel::watch_screens(move || apply_layout(&handle));

            server::start(app.handle().clone())?;
            app.state::<AppState>().0.lock().unwrap().cache_ttl_override = cache_ttl_override();
            spawn_prune(app.handle().clone());
            spawn_cache_watch(app.handle().clone());
            setup_tray(app.handle())?;
            // Recopia el hook si la app trae una versión nueva (no toca settings.json).
            std::thread::spawn(hooks::refresh_hook);
            updates::spawn_checks(app.handle().clone());

            // Oferta de reparación al arrancar: primera vez, o hooks que no están bien.
            if !settings.onboarded || hooks::info().status != "ok" {
                let _ = open_settings_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            resolve_permission,
            answer_question,
            jump_to_session,
            set_island_mode,
            get_settings,
            set_settings,
            list_screens,
            hooks_status,
            preview_hooks,
            apply_hooks,
            open_settings,
            updates::update_status,
            updates::check_updates_now,
            updates::install_update
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Pilas");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enfriandose_avisa_una_vez_por_episodio() {
        let t = |pairs: &[(&str, Option<CacheTemp>)]| -> CacheTemps {
            pairs.iter().map(|(id, t)| (id.to_string(), *t)).collect()
        };
        use CacheTemp::*;
        let empty = t(&[]);
        let a_hot = t(&[("a", Some(Caliente)), ("b", None)]);
        let a_cool = t(&[("a", Some(Enfriandose)), ("b", Some(Enfriandose))]);
        let a_cool_b_cold = t(&[("a", Some(Enfriandose)), ("b", Some(Fria))]);
        assert_eq!(newly_cooling(&empty, &a_cool), vec!["a", "b"]);
        assert_eq!(newly_cooling(&a_hot, &a_cool), vec!["a", "b"]);
        // Sigue enfriándose: no repite.
        assert!(newly_cooling(&a_cool, &a_cool_b_cold).is_empty());
        // Sale (caliente otra vez) y vuelve: episodio nuevo.
        assert_eq!(newly_cooling(&a_hot, &a_cool_b_cold), vec!["a"]);
        // Desactivado (None) no avisa.
        assert!(newly_cooling(&a_cool, &t(&[("a", None)])).is_empty());
    }

    #[test]
    fn tray_icon_is_template_36x36() {
        let icon = tray_icon();
        let rgba = icon.rgba();
        assert_eq!(rgba.len(), 36 * 36 * 4);
        assert_eq!(icon.width(), 36);
        assert_eq!(icon.height(), 36);
        // Índice del píxel que contiene el punto (u, v) del viewBox de la mascota.
        let pixel = |u: f64, v: f64| {
            let (s, ox, oy) = tray_map();
            let (px, py) = ((ox + u * s) as usize, (oy + v * s) as usize);
            (py * TRAY_SIZE + px) * 4
        };
        assert_eq!(rgba[3], 0, "la esquina superior izquierda debe ser transparente");
        assert_eq!(
            rgba[rgba.len() - 1],
            0,
            "la esquina inferior derecha debe ser transparente"
        );
        assert_eq!(rgba[pixel(70.0, 120.0) + 3], 255, "el cuerpo debe ser opaco");
        assert_eq!(rgba[pixel(70.0, 25.0) + 3], 255, "el remate debe ser opaco");
        assert_eq!(rgba[pixel(52.0, 144.0) + 3], 255, "los pies deben ser opacos");
        assert_eq!(rgba[pixel(52.0, 72.0) + 3], 0, "el ojo izquierdo debe ser un agujero");
        assert_eq!(rgba[pixel(88.0, 72.0) + 3], 0, "el ojo derecho debe ser un agujero");
    }
}
