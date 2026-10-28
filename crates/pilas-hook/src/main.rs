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

    // Conectar antes de buscar el TTY: si Pilas no corre, salimos sin trabajo extra.
    let stream = transport::connect()?;
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

    let reply = transport::send(stream, &msg)?;
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
#[cfg(target_os = "macos")]
fn find_tty() -> (Option<u32>, Option<String>) {
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..12 {
        let Some(info) = bsd_info(pid) else { break };
        // NODEV (-1 como dev_t) indica que el proceso no tiene terminal.
        if info.e_tdev != u32::MAX {
            let name = unsafe { libc::devname(info.e_tdev as libc::dev_t, libc::S_IFCHR) };
            if !name.is_null() {
                let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy();
                return (Some(pid), Some(format!("/dev/{name}")));
            }
        }
        if info.pbi_ppid <= 1 {
            break;
        }
        pid = info.pbi_ppid;
    }
    (None, None)
}

/// Información BSD de un proceso (sin lanzar `ps`).
#[cfg(target_os = "macos")]
fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let buf = &mut info as *mut _ as *mut std::ffi::c_void;
    let n = unsafe { libc::proc_pidinfo(pid as i32, libc::PROC_PIDTBSDINFO, 0, buf, size) };
    (n == size).then_some(info)
}

#[cfg(not(target_os = "macos"))]
fn find_tty() -> (Option<u32>, Option<String>) {
    (None, None) // TODO(v1.2): Windows y Linux
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    /// Conecta con la app. Un socket sin listener falla al instante: si Pilas no corre, salimos ya.
    pub fn connect() -> Option<UnixStream> {
        UnixStream::connect(pilas_core::paths::socket_path()?).ok()
    }

    /// Envía el mensaje. Devuelve `Some(Some(reply))` si era bloqueante y la app respondió.
    pub fn send(mut stream: UnixStream, msg: &HookMessage) -> Option<Option<AppReply>> {
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
    pub fn connect() -> Option<()> {
        None // TODO(v1.2): named pipe en Windows
    }
    pub fn send(_stream: (), _msg: &HookMessage) -> Option<Option<AppReply>> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn el_proceso_actual_tiene_info_bsd() {
        let info = bsd_info(std::process::id()).expect("proc_pidinfo del propio proceso");
        assert_eq!(info.pbi_pid, std::process::id());
    }
}
