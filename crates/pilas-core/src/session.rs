//! Sesiones de Claude Code y su máquina de estados (docs/SPEC.md, "Sesiones y estados").
//!
//!   SessionStart → EnEspera ─UserPromptSubmit→ Trabajando ─permiso/pregunta→ Pendiente
//!   Pendiente ─respondes→ Trabajando ─Stop→ EnEspera (+ evento Finished: la pilita se llena)
//!   Trabajando ─StopFailure→ Error ─UserPromptSubmit→ Trabajando
//!   SessionEnd en cualquier estado → la sesión se elimina

use crate::cache::CacheClock;
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
    /// Qué hace la última herramienta; la UI lo traduce.
    pub last_action: Option<LastAction>,
    /// Solicitudes bloqueantes abiertas (la app gestiona la cola y la baja al resolver).
    pub pending: u32,
    pub last_assistant_message: Option<String>,
    /// Milisegundos desde UNIX_EPOCH del último evento.
    pub updated_ms: u64,
    /// Reloj de la caché del prompt; la vista (`cache::temperature`) la construye la app.
    #[serde(skip)]
    pub cache: CacheClock,
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

/// Reloj de pared en milisegundos desde UNIX_EPOCH.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn project_of(cwd: &str) -> String {
    cwd.trim_end_matches('/').rsplit('/').next().unwrap_or(cwd).to_string()
}

/// Última acción de una sesión: datos puros, el texto lo pone la UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastAction {
    pub kind: ActionKind,
    /// Comando (Bash), ruta corta (archivos) o nombre de la herramienta (Other).
    pub arg: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Bash,
    Edit,
    Write,
    Read,
    SearchProject,
    SearchWeb,
    Question,
    Other,
}

/// Qué hace una herramienta, para la lista de sesiones.
pub fn describe_tool(payload: &Value) -> Option<LastAction> {
    let tool = protocol::tool_name(payload)?;
    let input = payload.get("tool_input");
    let field = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str);
    let short = |p: &str| {
        p.rsplit('/')
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/")
    };
    let (kind, arg) = match tool {
        "Bash" => (ActionKind::Bash, field("command").map(str::to_string)),
        "Edit" | "MultiEdit" => (ActionKind::Edit, field("file_path").map(short)),
        "Write" => (ActionKind::Write, field("file_path").map(short)),
        "Read" => (ActionKind::Read, field("file_path").map(short)),
        "Grep" | "Glob" => (ActionKind::SearchProject, None),
        "WebFetch" | "WebSearch" => (ActionKind::SearchWeb, None),
        "AskUserQuestion" => (ActionKind::Question, None),
        other => (ActionKind::Other, Some(other.to_string())),
    };
    Some(LastAction { kind, arg })
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
        v.sort_by(|a, b| {
            (b.pending > 0)
                .cmp(&(a.pending > 0))
                .then(b.updated_ms.cmp(&a.updated_ms))
        });
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

    /// Quita las sesiones cuyo `pid` ya no existe (sin `SessionEnd`). Sin `pid` se quedan.
    pub fn prune(&mut self, alive: impl Fn(u32) -> bool) -> Vec<StoreEvent> {
        let mut out = vec![];
        self.sessions.retain(|id, s| {
            let keep = s.pid.is_none_or(&alive);
            if !keep {
                out.push(StoreEvent::Removed(id.clone()));
            }
            keep
        });
        out
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
            return self
                .sessions
                .remove(&id)
                .map(|_| vec![StoreEvent::Removed(id)])
                .unwrap_or_default();
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
            cache: CacheClock::default(),
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

        // Solo la conversación principal: los eventos de subagentes llevan `agent_id`.
        if msg.payload.get("agent_id").is_none() {
            let source = msg.payload.get("source").and_then(Value::as_str);
            match (msg.event.as_str(), source) {
                ("SessionStart", Some("startup" | "clear")) => s.cache = CacheClock::default(),
                ("UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "Stop", _) => {
                    s.cache.last_ms = Some(now_ms());
                }
                _ => {}
            }
        }

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
                s.last_assistant_message = msg
                    .payload
                    .get("last_assistant_message")
                    .and_then(Value::as_str)
                    .map(str::to_string);
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
    use crate::cache::CacheClock;
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
        st.apply(&msg(
            "PreToolUse",
            json!({ "tool_name": "AskUserQuestion", "tool_input": {} }),
        ));
        st.apply(&msg("PermissionRequest", json!({ "tool_name": "AskUserQuestion" })));
        assert_eq!(st.get("s1").unwrap().pending, 1);
    }

    #[test]
    fn prune_quita_solo_pids_muertos() {
        let mut st = SessionStore::default();
        st.apply(&msg("SessionStart", json!({}))); // s1, pid 1 (muerto)
        let mut vivo = msg("SessionStart", json!({ "session_id": "s2" }));
        vivo.pid = Some(2);
        st.apply(&vivo);
        let mut sin_pid = msg("SessionStart", json!({ "session_id": "s3" }));
        sin_pid.pid = None;
        st.apply(&sin_pid);

        let ev = st.prune(|pid| pid == 2);
        assert_eq!(ev, vec![StoreEvent::Removed("s1".into())]);
        assert!(st.get("s1").is_none());
        assert!(st.get("s2").is_some());
        assert!(st.get("s3").is_some());
    }

    #[test]
    fn describe_herramientas() {
        let p = json!({ "tool_name": "Edit", "tool_input": { "file_path": "/a/b/src/app.ts" } });
        let a = describe_tool(&p).unwrap();
        assert_eq!((a.kind, a.arg.as_deref()), (ActionKind::Edit, Some("src/app.ts")));
        let p = json!({ "tool_name": "Bash", "tool_input": { "command": "npm test" } });
        let a = describe_tool(&p).unwrap();
        assert_eq!((a.kind, a.arg.as_deref()), (ActionKind::Bash, Some("npm test")));
    }

    #[test]
    fn describe_mapea_cada_tipo() {
        let kind = |tool: &str| describe_tool(&json!({ "tool_name": tool, "tool_input": {} })).unwrap();
        assert_eq!(kind("Write").kind, ActionKind::Write);
        assert_eq!(kind("Read").kind, ActionKind::Read);
        assert_eq!(kind("Glob").kind, ActionKind::SearchProject);
        assert_eq!(kind("WebSearch").kind, ActionKind::SearchWeb);
        assert_eq!(kind("AskUserQuestion").kind, ActionKind::Question);
        assert_eq!(
            kind("Task"),
            LastAction {
                kind: ActionKind::Other,
                arg: Some("Task".into())
            }
        );
    }

    #[test]
    fn caché_se_renueva_con_eventos_principales() {
        let mut st = SessionStore::default();
        st.apply(&msg("SessionStart", json!({})));
        assert_eq!(st.get("s1").unwrap().cache.last_ms, None);
        for ev in [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
        ] {
            st.get_mut("s1").unwrap().cache.last_ms = None;
            st.apply(&msg(ev, json!({})));
            assert!(
                st.get("s1").unwrap().cache.last_ms.is_some(),
                "{ev} debe renovar la caché"
            );
        }
    }

    #[test]
    fn eventos_de_subagente_no_tocan_la_caché() {
        let mut st = SessionStore::default();
        st.apply(&msg("UserPromptSubmit", json!({ "agent_id": "a1" })));
        st.apply(&msg("Stop", json!({ "agent_id": "a1" })));
        assert_eq!(st.get("s1").unwrap().cache.last_ms, None);
    }

    #[test]
    fn session_start_clear_vacia_el_reloj() {
        let mut st = SessionStore::default();
        st.apply(&msg("UserPromptSubmit", json!({})));
        st.apply(&msg("SessionStart", json!({ "source": "clear" })));
        assert_eq!(st.get("s1").unwrap().cache, CacheClock::default());
    }

    #[test]
    fn session_start_resume_conserva_el_reloj() {
        let mut st = SessionStore::default();
        st.apply(&msg("UserPromptSubmit", json!({})));
        let antes = st.get("s1").unwrap().cache.last_ms;
        st.apply(&msg("SessionStart", json!({ "source": "resume" })));
        assert_eq!(st.get("s1").unwrap().cache.last_ms, antes);
    }

    #[test]
    fn stop_failure_no_toca_la_caché() {
        let mut st = SessionStore::default();
        st.apply(&msg("UserPromptSubmit", json!({})));
        let antes = st.get("s1").unwrap().cache.last_ms;
        st.apply(&msg("StopFailure", json!({})));
        assert_eq!(st.get("s1").unwrap().cache.last_ms, antes);
    }
}
