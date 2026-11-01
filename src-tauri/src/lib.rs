mod hooks;
mod panel;
mod server;
mod settings;
mod updates;

use pilas_core::decision::{self, Answers, PermissionChoice};
use pilas_core::protocol::AppReply;
use pilas_core::session::{MascotState, Session, SessionStore, StoreEvent};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub(crate) enum Outcome {
    Reply(AppReply),
    /// The hook disconnected (e.g. the user answered in the terminal).
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
    /// Request ids in arrival order.
    pub queue: VecDeque<String>,
    pub screen: panel::ScreenChoice,
    pub mode: panel::IslandMode,
    pub open_height: Option<f64>,
    /// Session → Ghostty terminal id, saved after an exact jump.
    // shortcut: entries of ended sessions are never removed (a few bytes each); clean up if it grows
    pub terminal_ids: HashMap<String, String>,
    /// `None` means up to date or not checked yet.
    pub update: Option<updates::UpdateInfo>,
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

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub sessions: Vec<Session>,
    pub mascot: MascotState,
    pub pending: Vec<PendingView>,
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
    Snapshot {
        sessions: inner.store.list().into_iter().cloned().collect(),
        mascot: inner.store.mascot_state(),
        pending,
    }
}

pub(crate) fn emit_snapshot(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = snapshot(&state.0.lock().unwrap());
    let _ = app.emit("pilas://snapshot", snap);
}

fn deliver(
    app: &AppHandle,
    state: &AppState,
    request_id: &str,
    build: impl FnOnce(&PendingRequest) -> Option<Value>,
) -> Result<(), String> {
    {
        let mut inner = state.0.lock().unwrap();
        let req = inner.pending.remove(request_id).ok_or("the request no longer exists")?;
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

/// `answers = None` → answer in the terminal.
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

/// The UI reports its visible mode (and open card height) so the window can be sized.
#[tauri::command]
fn set_island_mode(app: AppHandle, state: State<'_, AppState>, mode: panel::IslandMode, open_height: Option<f64>) {
    {
        let mut inner = state.0.lock().unwrap();
        inner.mode = mode;
        inner.open_height = open_height;
    }
    apply_layout(&app);
}

fn apply_layout(app: &AppHandle) {
    let (screen, mode, open_height) = {
        let state = app.state::<AppState>();
        let inner = state.0.lock().unwrap();
        (inner.screen.clone(), inner.mode, inner.open_height)
    };
    panel::place(app, screen, mode, open_height);
}

#[tauri::command]
fn jump_to_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let target = {
        let inner = state.0.lock().unwrap();
        let s = inner.store.get(&session_id).ok_or("unknown session")?;
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

/// Signal 0; EPERM also counts: the process exists but is not ours.
fn pid_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

/// Sessions can die without a SessionEnd, so prune them every 30 s.
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
            // Requests from those sessions have nobody left to answer.
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
            emit_snapshot(&app); // after releasing the lock: emit_snapshot takes it
        }
    });
}

#[tauri::command]
fn get_settings() -> settings::Settings {
    settings::load()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: settings::Settings) -> Result<(), String> {
    settings::save(&settings)?;
    state.0.lock().unwrap().screen = settings.screen.clone();
    apply_layout(&app);
    settings::sync_launch_agent(settings.launch_at_login)?;
    let _ = app.emit("pilas://settings", &settings);
    Ok(())
}

/// Async on purpose: a sync command runs on the main thread and would deadlock `screen_names`.
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

/// Returns the backup path if `settings.json` already existed and was written.
#[tauri::command]
fn apply_hooks(action: String) -> Result<Option<String>, String> {
    let backup = hooks::apply(hooks::Action::parse(&action)?)?;
    Ok(backup.map(|p| p.display().to_string()))
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    open_settings_window(&app).map_err(|e| e.to_string())
}

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

/// Menu bar icon side in pixels (18 pt at 2x).
const TRAY_SIZE: usize = 36;
const TRAY_MARGIN: f64 = 1.0;
/// Antialias subsamples per axis (4x4 gives 17 coverage levels).
const TRAY_SUB: usize = 4;
/// Eyes 20 % larger than in the SVG: at 18 pt the holes would disappear.
const EYE_SCALE: f64 = 1.2;
/// Silhouette box in viewBox units (144x156) of `ui/src/mascot/pilita.ts`:
/// x0, y0, x1, y1 (without the arm, which does not fit in the icon).
const PILITA_BOX: (f64, f64, f64, f64) = (22.0, 18.0, 118.0, 151.0);

/// (s, ox, oy) such that px = ox + u * s, py = oy + v * s.
fn tray_map() -> (f64, f64, f64) {
    let (x0, y0, x1, y1) = PILITA_BOX;
    let size = TRAY_SIZE as f64;
    let s = (size - 2.0 * TRAY_MARGIN) / (y1 - y0);
    let ox = (size - (x1 - x0) * s) / 2.0 - x0 * s;
    let oy = TRAY_MARGIN - y0 * s;
    (s, ox, oy)
}

fn pilita_contains(u: f64, v: f64) -> bool {
    // Rounded rect (x0, y0, x1, y1, radius), same as the SVG `rect`.
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

/// Template icon: macOS only uses the alpha, so the color does not matter.
fn tray_icon() -> Image<'static> {
    let rgba: Vec<u8> = tray_alpha().into_iter().flat_map(|a| [0, 0, 0, a]).collect();
    Image::new_owned(rgba, TRAY_SIZE as u32, TRAY_SIZE as u32)
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    // shortcut: English-only labels because the backend has no i18n; translate when more native text appears
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Pilas", true, None::<&str>)?;
    // updates::publish inserts "Update to X…" at the top when a new version exists.
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
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // no Dock icon

            let settings = settings::load();
            app.state::<AppState>().0.lock().unwrap().screen = settings.screen.clone();

            if let Some(window) = app.get_webview_window("island") {
                panel::make_panel(&window);
            }
            apply_layout(app.handle());
            let handle = app.handle().clone();
            panel::watch_screens(move || apply_layout(&handle));

            server::start(app.handle().clone())?;
            spawn_prune(app.handle().clone());
            setup_tray(app.handle())?;
            // Recopy the hook if the app ships a new one (settings.json is not touched).
            std::thread::spawn(hooks::refresh_hook);
            updates::spawn_checks(app.handle().clone());

            // Offer repair on launch: first run, or hooks not in order.
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
        .expect("error while starting Pilas");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_is_template_36x36() {
        let icon = tray_icon();
        let rgba = icon.rgba();
        assert_eq!(rgba.len(), 36 * 36 * 4);
        assert_eq!(icon.width(), 36);
        assert_eq!(icon.height(), 36);
        let pixel = |u: f64, v: f64| {
            let (s, ox, oy) = tray_map();
            let (px, py) = ((ox + u * s) as usize, (oy + v * s) as usize);
            (py * TRAY_SIZE + px) * 4
        };
        assert_eq!(rgba[3], 0, "top-left corner must be transparent");
        assert_eq!(rgba[rgba.len() - 1], 0, "bottom-right corner must be transparent");
        assert_eq!(rgba[pixel(70.0, 120.0) + 3], 255, "body must be opaque");
        assert_eq!(rgba[pixel(70.0, 25.0) + 3], 255, "cap must be opaque");
        assert_eq!(rgba[pixel(52.0, 144.0) + 3], 255, "feet must be opaque");
        assert_eq!(rgba[pixel(52.0, 72.0) + 3], 0, "left eye must be a hole");
        assert_eq!(rgba[pixel(88.0, 72.0) + 3], 0, "right eye must be a hole");
    }
}
