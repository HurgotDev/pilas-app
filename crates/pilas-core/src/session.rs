//! Claude Code sessions and their state machine (docs/SPEC.md, "Sesiones y estados").
//!
//!   SessionStart → Idle ─UserPromptSubmit→ Working ─permission/question→ Pending
//!   Pending ─answered→ Working ─Stop→ Idle (+ Finished event: the mascot fills up)
//!   Working ─StopFailure→ Error ─UserPromptSubmit→ Working
//!   SessionEnd in any state → the session is removed

use crate::protocol::{self, HookMessage};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Working,
    Pending,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MascotState {
    Sleeping,
    Working,
    Alert,
}
// "full" is transient: the UI shows it for ~4 s on `StoreEvent::Finished`.

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub session_id: String,
    pub agent: String,
    pub cwd: String,
    pub project: String,
    pub tty: Option<String>,
    pub pid: Option<u32>,
    pub term_program: Option<String>,
    pub state: SessionState,
    pub last_action: Option<LastAction>,
    /// Open blocking requests; the app owns the queue and decrements this on resolve.
    pub pending: u32,
    pub last_assistant_message: Option<String>,
    pub updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreEvent {
    Updated(String),
    Removed(String),
    /// A session finished its turn (Working → Idle on `Stop`).
    Finished(String),
}

#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: HashMap<String, Session>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn project_of(cwd: &str) -> String {
    cwd.trim_end_matches('/').rsplit('/').next().unwrap_or(cwd).to_string()
}

/// Pure data; the UI renders the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastAction {
    pub kind: ActionKind,
    /// Command (Bash), short path (files) or tool name (Other).
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
        if s.clone().any(|x| x.pending > 0 || x.state == SessionState::Pending) {
            MascotState::Alert
        } else if s.clone().any(|x| x.state == SessionState::Working) {
            MascotState::Working
        } else {
            MascotState::Sleeping
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<Session> {
        self.sessions.remove(id)
    }

    /// Drops sessions whose `pid` is gone without a `SessionEnd`; sessions without a `pid` stay.
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

    pub fn resolve_pending(&mut self, id: &str) -> Option<StoreEvent> {
        let s = self.sessions.get_mut(id)?;
        s.pending = s.pending.saturating_sub(1);
        if s.pending == 0 && s.state == SessionState::Pending {
            s.state = SessionState::Working;
        }
        s.updated_ms = now_ms();
        Some(StoreEvent::Updated(id.to_string()))
    }

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
            state: SessionState::Idle,
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
            "SessionStart" => s.state = SessionState::Idle,
            "UserPromptSubmit" => s.state = SessionState::Working,
            "PreToolUse" => {
                s.last_action = describe_tool(&msg.payload);
                if msg.blocking {
                    s.pending += 1;
                    s.state = SessionState::Pending;
                } else if s.state != SessionState::Pending {
                    s.state = SessionState::Working;
                }
            }
            "PermissionRequest" => {
                s.pending += 1;
                s.state = SessionState::Pending;
            }
            "PostToolUse" | "PostToolUseFailure" => {
                if s.pending == 0 {
                    s.state = SessionState::Working;
                }
            }
            "Stop" => {
                let was_working = s.state != SessionState::Idle;
                s.state = SessionState::Idle;
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
    fn full_cycle() {
        let mut st = SessionStore::default();
        st.apply(&msg("SessionStart", json!({})));
        assert_eq!(st.get("s1").unwrap().project, "pilas-app");
        assert_eq!(st.mascot_state(), MascotState::Sleeping);

        st.apply(&msg("UserPromptSubmit", json!({})));
        assert_eq!(st.mascot_state(), MascotState::Working);

        st.apply(&msg("PermissionRequest", json!({ "tool_name": "Bash" })));
        assert_eq!(st.get("s1").unwrap().state, SessionState::Pending);
        assert_eq!(st.mascot_state(), MascotState::Alert);

        st.resolve_pending("s1");
        assert_eq!(st.get("s1").unwrap().state, SessionState::Working);

        let ev = st.apply(&msg("Stop", json!({ "last_assistant_message": "Done" })));
        assert!(ev.contains(&StoreEvent::Finished("s1".into())));
        assert_eq!(st.get("s1").unwrap().last_assistant_message.as_deref(), Some("Done"));

        let ev = st.apply(&msg("SessionEnd", json!({})));
        assert_eq!(ev, vec![StoreEvent::Removed("s1".into())]);
        assert!(st.get("s1").is_none());
    }

    #[test]
    fn states_serialize_to_ui_contract() {
        let mascot = [MascotState::Sleeping, MascotState::Working, MascotState::Alert];
        assert_eq!(json!(mascot), json!(["sleeping", "working", "alert"]));
        let session = [
            SessionState::Idle,
            SessionState::Working,
            SessionState::Pending,
            SessionState::Error,
        ];
        assert_eq!(json!(session), json!(["idle", "working", "pending", "error"]));
    }

    #[test]
    fn question_is_not_duplicated() {
        let mut st = SessionStore::default();
        st.apply(&msg(
            "PreToolUse",
            json!({ "tool_name": "AskUserQuestion", "tool_input": {} }),
        ));
        st.apply(&msg("PermissionRequest", json!({ "tool_name": "AskUserQuestion" })));
        assert_eq!(st.get("s1").unwrap().pending, 1);
    }

    #[test]
    fn prune_removes_only_dead_pids() {
        let mut st = SessionStore::default();
        st.apply(&msg("SessionStart", json!({}))); // s1, pid 1 (dead)
        let mut alive_msg = msg("SessionStart", json!({ "session_id": "s2" }));
        alive_msg.pid = Some(2);
        st.apply(&alive_msg);
        let mut no_pid = msg("SessionStart", json!({ "session_id": "s3" }));
        no_pid.pid = None;
        st.apply(&no_pid);

        let ev = st.prune(|pid| pid == 2);
        assert_eq!(ev, vec![StoreEvent::Removed("s1".into())]);
        assert!(st.get("s1").is_none());
        assert!(st.get("s2").is_some());
        assert!(st.get("s3").is_some());
    }

    #[test]
    fn describe_tools() {
        let p = json!({ "tool_name": "Edit", "tool_input": { "file_path": "/a/b/src/app.ts" } });
        let a = describe_tool(&p).unwrap();
        assert_eq!((a.kind, a.arg.as_deref()), (ActionKind::Edit, Some("src/app.ts")));
        let p = json!({ "tool_name": "Bash", "tool_input": { "command": "npm test" } });
        let a = describe_tool(&p).unwrap();
        assert_eq!((a.kind, a.arg.as_deref()), (ActionKind::Bash, Some("npm test")));
    }

    #[test]
    fn describe_maps_each_kind() {
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
}
