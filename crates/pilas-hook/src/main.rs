//! `pilas-hook`: lo ejecuta Claude Code en cada evento (forma exec, ver docs/SPEC.md).
//!
//! Reglas de oro:
//! - Siempre sale con código 0. Cualquier fallo = sin salida = Claude Code sigue su flujo normal.
//! - No interpreta el payload: lo reenvía íntegro. Solo decide si debe esperar respuesta.
//! - Nunca reintenta.

use pilas_core::protocol::{self, AppReply, HookMessage, AGENT_CLAUDE_CODE, PROTOCOL_VERSION};
use std::io::Read;

fn main() {
    let _ = run();
    std::process::exit(0);
}

fn run() -> Option<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&input).ok()?;
    let event = payload.get("hook_event_name")?.as_str()?.to_string();

    if protocol::is_ignored(&event, &payload) {
        return None; // p. ej. PermissionRequest de AskUserQuestion: salida vacía
    }

    let blocking = protocol::is_blocking(&event, &payload);
    let (pid, tty) = find_tty();
    let msg = HookMessage {
        v: PROTOCOL_VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        agent: AGENT_CLAUDE_CODE.to_string(),
        event,
        blocking,
        tty,
        pid,
        term_program: std::env::var("TERM_PROGRAM").ok().map(|s| s.to_lowercase()),
        payload,
    };

    let reply = transport::send(&msg)?;
    if let Some(reply) = reply {
        if reply.id != msg.id || reply.v > PROTOCOL_VERSION {
            return None;
        }
        if let Some(out) = reply.stdout {
            println!("{out}");
        }
    }
    Some(())
}

/// Recorre los procesos ancestros hasta el primero con TTY (el proceso de Claude Code).
/// Los hooks corren sin terminal controladora, así que no podemos leer /dev/tty.
#[cfg(unix)]
fn find_tty() -> (Option<u32>, Option<String>) {
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..12 {
        let out = std::process::Command::new("ps")
            .args(["-o", "ppid=,tty=", "-p", &pid.to_string()])
            .output();
        let Ok(out) = out else { break };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut parts = text.split_whitespace();
        let (Some(ppid), Some(tty)) = (parts.next(), parts.next()) else {
            break;
        };
        if tty != "??" && tty != "?" {
            let tty = if tty.starts_with("/dev/") {
                tty.to_string()
            } else {
                format!("/dev/{tty}")
            };
            return (Some(pid), Some(tty));
        }
        match ppid.parse::<u32>() {
            Ok(p) if p > 1 => pid = p,
            _ => break,
        }
    }
    (None, None)
}

#[cfg(not(unix))]
fn find_tty() -> (Option<u32>, Option<String>) {
    (None, None) // TODO(v1.2): Windows
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    /// Envía el mensaje. Devuelve `Some(Some(reply))` si era bloqueante y la app respondió.
    pub fn send(msg: &HookMessage) -> Option<Option<AppReply>> {
        let path = pilas_core::paths::socket_path()?;
        // Un socket local sin listener falla al instante: si Pilas no corre, salimos ya.
        let mut stream = UnixStream::connect(path).ok()?;
        stream.set_write_timeout(Some(Duration::from_millis(300))).ok()?;
        let mut line = serde_json::to_string(msg).ok()?;
        line.push('\n');
        stream.write_all(line.as_bytes()).ok()?;
        if !msg.blocking {
            return Some(None);
        }
        // Esperamos sin límite: el `timeout` del hook en settings.json (3600 s) es el tope real.
        stream.set_read_timeout(None).ok()?;
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        reader.read_line(&mut resp).ok()?;
        Some(Some(serde_json::from_str(&resp).ok()?))
    }
}

#[cfg(not(unix))]
mod transport {
    use super::*;
    pub fn send(_msg: &HookMessage) -> Option<Option<AppReply>> {
        None // TODO(v1.2): named pipe en Windows
    }
}
