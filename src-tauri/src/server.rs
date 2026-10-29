//! Servidor del socket local: una conexión por invocación de `pilas-hook`, NDJSON.

use crate::{emit_snapshot, ensure_focus_watch, AppState, Outcome, PendingRequest};
use pilas_core::protocol::{HookMessage, PROTOCOL_VERSION};
use pilas_core::session::StoreEvent;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(unix)]
pub fn start(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let path = pilas_core::paths::socket_path().ok_or("no se pudo resolver la ruta del socket")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = std::fs::remove_file(&path); // socket huérfano de una ejecución anterior
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || handle(app, stream));
        }
    });
    Ok(())
}

#[cfg(not(unix))]
pub fn start(_app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    Ok(()) // TODO(v1.2): named pipe en Windows
}

#[cfg(unix)]
fn handle(app: AppHandle, stream: std::os::unix::net::UnixStream) {
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let Ok(msg) = serde_json::from_str::<HookMessage>(&line) else {
        return;
    };
    if msg.v > PROTOCOL_VERSION {
        return; // versión más nueva que la nuestra: el hook sale sin decisión
    }

    // Antes del lock: osascript tarda ~100 ms y no debe bloquear la UI.
    let quiet = msg.blocking
        && pilas_jump::is_focused(&pilas_jump::JumpTarget {
            term_program: msg.term_program.clone(),
            tty: msg.tty.clone(),
            pid: msg.pid,
            ..Default::default()
        });
    let state = app.state::<AppState>();
    let rx = {
        let mut inner = state.0.lock().unwrap();
        let events = inner.store.apply(&msg);
        for ev in &events {
            if let StoreEvent::Finished(id) = ev {
                let _ = app.emit("pilas://finished", id.clone());
            }
        }
        if msg.blocking {
            let (tx, rx) = mpsc::channel();
            let session_id = pilas_core::protocol::session_id(&msg.payload)
                .unwrap_or_default()
                .to_string();
            inner.pending.insert(
                msg.id.clone(),
                PendingRequest {
                    session_id,
                    event: msg.event.clone(),
                    payload: msg.payload.clone(),
                    tx: tx.clone(),
                    quiet,
                },
            );
            inner.queue.push_back(msg.id.clone());
            // Vigila si el hook se va (el usuario respondió en la terminal o Claude Code canceló).
            std::thread::spawn(move || {
                let mut buf = [0u8; 1];
                let _ = reader.get_mut().read(&mut buf); // EOF o error = el hook cerró
                let _ = tx.send(Outcome::HookGone);
            });
            Some(rx)
        } else {
            None
        }
    };
    emit_snapshot(&app);
    if quiet {
        ensure_focus_watch(&app);
    } else if msg.blocking {
        let _ = app.emit("pilas://pending", msg.id.clone()); // la UI abre la isla y suena la alerta
    }

    let Some(rx) = rx else { return };
    match rx.recv() {
        Ok(Outcome::Reply(reply)) => {
            let mut stream = stream;
            if let Ok(mut out) = serde_json::to_string(&reply) {
                out.push('\n');
                let _ = stream.write_all(out.as_bytes());
            }
        }
        Ok(Outcome::HookGone) | Err(_) => {
            // Descartar la tarjeta: la decisión ya no tiene a quién llegar.
            let mut inner = state.0.lock().unwrap();
            if let Some(req) = inner.pending.remove(&msg.id) {
                inner.queue.retain(|r| r != &msg.id);
                inner.store.resolve_pending(&req.session_id);
            }
            drop(inner);
            emit_snapshot(&app);
        }
    }
}
