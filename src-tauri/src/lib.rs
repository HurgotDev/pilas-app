//! App Pilas (Tauri 2). Responsabilidades de este crate:
//! - servidor del socket local (ver `server`), que alimenta el `SessionStore` de pilas-core,
//! - cola FIFO de solicitudes bloqueantes y entrega de decisiones al hook,
//! - comandos para la WebView y eventos hacia ella,
//! - (M1) comportamiento de la ventana como panel del notch.

mod panel;
mod server;

use pilas_core::decision::{self, Answers, PermissionChoice};
use pilas_core::protocol::AppReply;
use pilas_core::session::{MascotState, Session, SessionStore, StoreEvent};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

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
    pub screen: panel::ScreenChoice,
    pub mode: panel::IslandMode,
    pub open_height: Option<f64>,
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
        (inner.screen.clone(), inner.mode, inner.open_height)
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
            cwd: Some(s.cwd.clone()),
            cached_terminal_id: None, // TODO(M6): cachear el id devuelto por un salto exacto
        }
    };
    pilas_jump::jumper_for(target.term_program.as_deref())
        .jump(&target)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// El proceso sigue existiendo (señal 0; EPERM también cuenta: existe, sin permiso).
fn pid_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false };
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
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

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // sin ícono en el Dock

            if let Some(window) = app.get_webview_window("island") {
                panel::make_panel(&window);
            }
            apply_layout(app.handle());
            let handle = app.handle().clone();
            panel::watch_screens(move || apply_layout(&handle));

            server::start(app.handle().clone())?;
            spawn_prune(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            resolve_permission,
            answer_question,
            jump_to_session,
            set_island_mode
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Pilas");
}
