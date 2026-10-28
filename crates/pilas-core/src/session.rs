//! Sesiones de Claude Code y su máquina de estados (docs/SPEC.md, "Sesiones y estados").
//!
//!   SessionStart → EnEspera ─UserPromptSubmit→ Trabajando ─permiso/pregunta→ Pendiente
//!   Pendiente ─respondes→ Trabajando ─Stop→ EnEspera (+ evento Finished: la pilita se llena)
//!   Trabajando ─StopFailure→ Error ─UserPromptSubmit→ Trabajando
//!   SessionEnd en cualquier estado → la sesión se elimina

use crate::protocol::{self, HookMessage};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    EnEspera,
    Trabajando,
    Pendiente,
    Error,
}

/// Estado agregado que muestra la pilita.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MascotState {
    Dormida,
    Cargando,
    Alerta,
}
// "Llena" es transitoria: la UI la muestra ~4 s al recibir `StoreEvent::Finished`.

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub session_id: String,
    pub agent: String,
    pub cwd: String,
    /// Última carpeta del `cwd`.
    pub project: String,
    pub tty: Option<String>,
    pub pid: Option<u32>,
    pub term_program: Option<String>,
    pub state: SessionState,
    /// "Editando src/app.ts", "Ejecutando npm test"…
    pub last_action: Option<String>,
    /// Solicitudes bloqueantes abiertas (la app gestiona la cola y la baja al resolver).
    pub pending: u32,
    pub last_assistant_message: Option<String>,
    /// Milisegundos desde UNIX_EPOCH del último evento.
    pub updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreEvent {
    Updated(String),
    Removed(String),
    /// Una sesión terminó su turno (Trabajando → EnEspera por `Stop`).
    Finished(String),
}

#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: HashMap<String, Session>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn project_of(cwd: &str) -> String {
    cwd.trim_end_matches('/').rsplit('/').next().unwrap_or(cwd).to_string()
}

/// Descripción corta de lo que hace una herramienta, para la lista de sesiones.
pub fn describe_tool(payload: &Value) -> Option<String> {
    let tool = protocol::tool_name(payload)?;
    let input = payload.get("tool_input");
    let field = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str);
    let short = |p: &str| p.rsplit('/').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/");
    Some(match tool {
        "Bash" => format!("Ejecutando {}", field("command").unwrap_or("un comando")),
        "Edit" | "MultiEdit" => format!("Editando {}", field("file_path").map(short).unwrap_or_default()),
        "Write" => format!("Escribiendo {}", field("file_path").map(short).unwrap_or_default()),
        "Read" => format!("Leyendo {}", field("file_path").map(short).unwrap_or_default()),
        "Grep" | "Glob" => "Buscando en el proyecto".to_string(),
        "WebFetch" | "WebSearch" => "Buscando en la web".to_string(),
        "AskUserQuestion" => "Tiene una pregunta".to_string(),
        other => format!("Usando {other}"),
    })
}

impl SessionStore {
    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }

    /// Sesiones ordenadas: pendientes primero, luego por actividad reciente.
    pub fn list(&self) -> Vec<&Session> {
        let mut v: Vec<_> = self.sessions.values().collect();
        v.sort_by(|a, b| (b.pending > 0).cmp(&(a.pending > 0)).then(b.updated_ms.cmp(&a.updated_ms)));
        v
    }

    pub fn mascot_state(&self) -> MascotState {
        let s = self.sessions.values();
        if s.clone().any(|x| x.pending > 0 || x.state == SessionState::Pendiente) {
            MascotState::Alerta
        } else if s.clone().any(|x| x.state == SessionState::Trabajando) {
            MascotState::Cargando
        } else {
            MascotState::Dormida
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<Session> {
        self.sessions.remove(id)
    }

    /// Marca una solicitud bloqueante como resuelta (o abandonada).
    pub fn resolve_pending(&mut self, id: &str) -> Option<StoreEvent> {
        let s = self.sessions.get_mut(id)?;
        s.pending = s.pending.saturating_sub(1);
        if s.pending == 0 && s.state == SessionState::Pendiente {
            s.state = SessionState::Trabajando;
        }
        s.updated_ms = now_ms();
        Some(StoreEvent::Updated(id.to_string()))
    }

    /// Aplica un mensaje del hook y devuelve qué cambió.
    pub fn apply(&mut self, msg: &HookMessage) -> Vec<StoreEvent> {
        if protocol::is_ignored(&msg.event, &msg.payload) {
            return vec![];
        }
        let Some(id) = protocol::session_id(&msg.payload).map(str::to_string) else {
            return vec![];
        };
        if msg.event == "SessionEnd" {
            return self.sessions.remove(&id).map(|_| vec![StoreEvent::Removed(id)]).unwrap_or_default();
        }

        let cwd = msg.payload.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
        let s = self.sessions.entry(id.clone()).or_insert_with(|| Session {
            session_id: id.clone(),
            agent: msg.agent.clone(),
            project: project_of(&cwd),
            cwd: cwd.clone(),
            tty: None,
            pid: None,
            term_program: None,
            state: SessionState::EnEspera,
            last_action: None,
            pending: 0,
            last_assistant_message: None,
            updated_ms: 0,
        });
        if !cwd.is_empty() && s.cwd != cwd {
            s.project = project_of(&cwd);
            s.cwd = cwd;
        }
        if msg.tty.is_some() {
            s.tty = msg.tty.clone();
        }
        if msg.pid.is_some() {
            s.pid = msg.pid;
        }
        if msg.term_program.is_some() {
            s.term_program = msg.term_program.clone();
        }
        s.updated_ms = now_ms();

        let mut out = vec![StoreEvent::Updated(id.clone())];
        match msg.event.as_str() {
            "SessionStart" => s.state = SessionState::EnEspera,
            "UserPromptSubmit" => s.state = SessionState::Trabajando,
            "PreToolUse" => {
                s.last_action = describe_tool(&msg.payload);
                if msg.blocking {
                    s.pending += 1;
                    s.state = SessionState::Pendiente;
                } else if s.state != SessionState::Pendiente {
                    s.state = SessionState::Trabajando;
                }
            }
            "PermissionRequest" => {
                s.pending += 1;
                s.state = SessionState::Pendiente;
            }
            "PostToolUse" | "PostToolUseFailure" => {
                if s.pending == 0 {
                    s.state = SessionState::Trabajando;
                }
            }
            "Stop" => {
                let was_working = s.state != SessionState::EnEspera;
                s.state = SessionState::EnEspera;
                s.pending = 0;
                s.last_assistant_message =
                    msg.payload.get("last_assistant_message").and_then(Value::as_str).map(str::to_string);
                if was_working {
                    out.push(StoreEvent::Finished(id.clone()));
                }
            }
            "StopFailure" => s.state = SessionState::Error,
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AGENT_CLAUDE_CODE, PROTOCOL_VERSION};
    use serde_json::json;

    fn msg(event: &str, extra: Value) -> HookMessage {
        let mut payload = json!({ "session_id": "s1", "cwd": "/Users/hugo/dev/pilas-app", "hook_event_name": event });
        if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        HookMessage {
            v: PROTOCOL_VERSION,
            id: "x".into(),
            agent: AGENT_CLAUDE_CODE.into(),
            event: event.into(),
            blocking: protocol::is_blocking(event, &payload),
            tty: Some("/dev/ttys004".into()),
            pid: Some(1),
            term_program: Some("ghostty".into()),
            payload,
        }
    }

    #[test]
    fn ciclo_completo() {
        let mut st = SessionStore::default();
        st.apply(&msg("SessionStart", json!({})));
        assert_eq!(st.get("s1").unwrap().project, "pilas-app");
        assert_eq!(st.mascot_state(), MascotState::Dormida);

        st.apply(&msg("UserPromptSubmit", json!({})));
        assert_eq!(st.mascot_state(), MascotState::Cargando);

        st.apply(&msg("PermissionRequest", json!({ "tool_name": "Bash" })));
        assert_eq!(st.get("s1").unwrap().state, SessionState::Pendiente);
        assert_eq!(st.mascot_state(), MascotState::Alerta);

        st.resolve_pending("s1");
        assert_eq!(st.get("s1").unwrap().state, SessionState::Trabajando);

        let ev = st.apply(&msg("Stop", json!({ "last_assistant_message": "Listo" })));
        assert!(ev.contains(&StoreEvent::Finished("s1".into())));
        assert_eq!(st.get("s1").unwrap().last_assistant_message.as_deref(), Some("Listo"));

        let ev = st.apply(&msg("SessionEnd", json!({})));
        assert_eq!(ev, vec![StoreEvent::Removed("s1".into())]);
        assert!(st.get("s1").is_none());
    }

    #[test]
    fn pregunta_no_se_duplica() {
        let mut st = SessionStore::default();
        st.apply(&msg("PreToolUse", json!({ "tool_name": "AskUserQuestion", "tool_input": {} })));
        st.apply(&msg("PermissionRequest", json!({ "tool_name": "AskUserQuestion" })));
        assert_eq!(st.get("s1").unwrap().pending, 1);
    }

    #[test]
    fn describe_herramientas() {
        let p = json!({ "tool_name": "Edit", "tool_input": { "file_path": "/a/b/src/app.ts" } });
        assert_eq!(describe_tool(&p).unwrap(), "Editando src/app.ts");
        let p = json!({ "tool_name": "Bash", "tool_input": { "command": "npm test" } });
        assert_eq!(describe_tool(&p).unwrap(), "Ejecutando npm test");
    }
}
