//! Local socket server: one NDJSON connection per `pilas-hook` run.

use crate::{emit_snapshot, AppState, Outcome, PendingRequest};
use pilas_core::decision;
use pilas_core::protocol::{AppReply, HookMessage, PROTOCOL_VERSION};
use pilas_core::session::StoreEvent;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(unix)]
pub fn start(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let path = pilas_core::paths::socket_path().ok_or("could not resolve the socket path")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = std::fs::remove_file(&path); // orphan socket from an earlier run
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
    Ok(()) // TODO(v1.2): named pipe on Windows
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
        return; // newer than us: the hook exits without a decision
    }

    // Before the lock: osascript takes ~100 ms and must not block the UI.
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
        let session_id = pilas_core::protocol::session_id(&msg.payload)
            .unwrap_or_default()
            .to_string();
        if quiet {
            // The focused terminal asks by itself: do not queue, and undo the pending count from `apply`.
            inner.store.resolve_pending(&session_id);
            None
        } else if msg.blocking {
            let (tx, rx) = mpsc::channel();
            inner.pending.insert(
                msg.id.clone(),
                PendingRequest {
                    session_id,
                    event: msg.event.clone(),
                    payload: msg.payload.clone(),
                    tx: tx.clone(),
                },
            );
            inner.queue.push_back(msg.id.clone());
            // Watch for the hook leaving (answered in the terminal or cancelled by Claude Code).
            std::thread::spawn(move || {
                let mut buf = [0u8; 1];
                let _ = reader.get_mut().read(&mut buf); // EOF or error means the hook closed
                let _ = tx.send(Outcome::HookGone);
            });
            Some(rx)
        } else {
            None
        }
    };
    emit_snapshot(&app);
    if quiet {
        // Same as "answer in the terminal": without stdout Claude Code shows its native dialog.
        let stdout = (msg.event == "PreToolUse").then(decision::question_to_terminal);
        write_reply(stream, &AppReply::new(msg.id, stdout));
        return;
    }
    if msg.blocking {
        let _ = app.emit("pilas://pending", msg.id.clone()); // the UI opens the island and plays the alert
    }

    let Some(rx) = rx else { return };
    match rx.recv() {
        Ok(Outcome::Reply(reply)) => write_reply(stream, &reply),
        Ok(Outcome::HookGone) | Err(_) => {
            // Drop the card: the decision has nowhere to go.
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

#[cfg(unix)]
fn write_reply(mut stream: std::os::unix::net::UnixStream, reply: &AppReply) {
    if let Ok(mut out) = serde_json::to_string(reply) {
        out.push('\n');
        let _ = stream.write_all(out.as_bytes());
    }
}
