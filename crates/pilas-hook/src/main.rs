//! `pilas-hook`: Claude Code runs it on every event (exec form, see docs/SPEC.md).
//!
//! - Always exits 0. Any failure means no output, so Claude Code carries on as usual.
//! - Does not interpret the payload: forwards it whole and only decides whether to wait for a reply.
//! - Never retries.

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
        return None;
    }

    // Connect before looking up the TTY: if Pilas is not running, exit without extra work.
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

/// Walks up the ancestors to the first one with a TTY (the Claude Code process).
/// Hooks run without a controlling terminal, so /dev/tty cannot be read.
#[cfg(target_os = "macos")]
fn find_tty() -> (Option<u32>, Option<String>) {
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..12 {
        let Some(info) = bsd_info(pid) else { break };
        // NODEV (-1 as dev_t) means the process has no terminal.
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

/// Avoids spawning `ps`.
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
    (None, None) // TODO(v1.2): Windows and Linux
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    /// A socket without a listener fails instantly, so this returns at once when Pilas is not running.
    pub fn connect() -> Option<UnixStream> {
        UnixStream::connect(pilas_core::paths::socket_path()?).ok()
    }

    /// `Some(Some(reply))` only if the message was blocking and the app replied.
    pub fn send(mut stream: UnixStream, msg: &HookMessage) -> Option<Option<AppReply>> {
        stream.set_write_timeout(Some(Duration::from_millis(300))).ok()?;
        let mut line = serde_json::to_string(msg).ok()?;
        line.push('\n');
        stream.write_all(line.as_bytes()).ok()?;
        if !msg.blocking {
            return Some(None);
        }
        // No limit: the hook `timeout` in settings.json (3600 s) is the real cap.
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
        None // TODO(v1.2): named pipe on Windows
    }
    pub fn send(_stream: (), _msg: &HookMessage) -> Option<Option<AppReply>> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn current_process_has_bsd_info() {
        let info = bsd_info(std::process::id()).expect("proc_pidinfo of own process");
        assert_eq!(info.pbi_pid, std::process::id());
    }
}
