//! Jump to a session's terminal. Fails closed: without a certain match only the app is
//! brought to the front; a wrong tab is never focused.

use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct JumpTarget {
    pub term_program: Option<String>,
    pub tty: Option<String>,
    /// Used to recognise background sessions.
    pub pid: Option<u32>,
    pub cwd: Option<String>,
    /// Ghostty terminal id cached from an earlier jump.
    pub cached_terminal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpOutcome {
    Exact { terminal_id: Option<String> },
    AppOnly,
}

#[derive(Debug, thiserror::Error)]
pub enum JumpError {
    #[error("could not run osascript: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("AppleScript failed: {0}")]
    Script(String),
}

pub trait TerminalJumper {
    fn can_handle(&self, term_program: Option<&str>) -> bool;
    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError>;
}

pub fn jumper_for(term_program: Option<&str>) -> Box<dyn TerminalJumper> {
    let ghostty = GhosttyJumper;
    if ghostty.can_handle(term_program) {
        Box::new(ghostty)
    } else {
        Box::new(GenericJumper)
    }
}

fn osascript(script: &str) -> Result<String, JumpError> {
    let out = Command::new("osascript").arg("-e").arg(script).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(JumpError::Script(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

fn as_literal(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Ghostty ≥ 1.3 en macOS (https://ghostty.org/docs/features/applescript).
pub struct GhosttyJumper;

impl GhosttyJumper {
    fn activate() -> Result<JumpOutcome, JumpError> {
        osascript("tell application \"Ghostty\" to activate")?;
        Ok(JumpOutcome::AppOnly)
    }

    /// `property` is a fixed name from our code, never user input.
    /// Returns the id only when exactly one terminal matches.
    fn focus_unique(property: &str, value: &str) -> Result<Option<String>, JumpError> {
        let id = osascript(&unique_script(property, value))?;
        Ok((!id.is_empty()).then_some(id))
    }
}

/// Does nothing when 0 or several terminals match.
fn unique_script(property: &str, value: &str) -> String {
    format!(
        "tell application \"Ghostty\"\n\
           set ts to (every terminal whose {property} is {value})\n\
           if (count of ts) is 1 then\n\
             set t to item 1 of ts\n\
             focus t\n\
             activate\n\
             return (id of t) as text\n\
           end if\n\
           return \"\"\n\
         end tell",
        value = as_literal(value)
    )
}

impl TerminalJumper for GhosttyJumper {
    fn can_handle(&self, term_program: Option<&str>) -> bool {
        term_program.is_some_and(|t| t.eq_ignore_ascii_case("ghostty"))
    }

    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError> {
        // Ordered by reliability; an error in one step does not stop the rest.
        let steps = [
            ("id", target.cached_terminal_id.as_deref()),
            ("tty", target.tty.as_deref()),
            ("working directory", target.cwd.as_deref()),
        ];
        for (property, value) in steps {
            if let Some(value) = value {
                if let Ok(Some(id)) = Self::focus_unique(property, value) {
                    return Ok(JumpOutcome::Exact { terminal_id: Some(id) });
                }
            }
        }
        Self::activate()
    }
}

/// Only Ghostty can be checked; any error counts as "not focused" so the user is still alerted.
pub fn is_focused(target: &JumpTarget) -> bool {
    let Ok(out) = osascript(focused_script()) else {
        return false;
    };
    // `claude` is only spawned if Ghostty is frontmost and the tty was not enough.
    is_focused_with(&out, target, agents_json)
}

/// `agents` yields `claude agents --json` and is only called when needed.
fn is_focused_with(out: &str, target: &JumpTarget, agents: impl FnOnce() -> Option<String>) -> bool {
    let Some((tty, title)) = parse_focus(out) else {
        return false;
    };
    if tty_matches(tty, target.tty.as_deref()) {
        return true;
    }
    let (Some(pid), false) = (target.pid, title.is_empty()) else {
        return false;
    };
    agents()
        .and_then(|json| unique_name_for_pid(&json, pid))
        .is_some_and(|name| name == strip_glyph(title))
}

/// Returns "tty\ntitle" of Ghostty's focused terminal, or "" if Ghostty is not
/// running or not frontmost. Never launches Ghostty.
fn focused_script() -> &'static str {
    "if application \"Ghostty\" is running then\n\
       tell application \"Ghostty\"\n\
         if frontmost then\n\
           set t to focused terminal of selected tab of front window\n\
           return (tty of t) & linefeed & (name of t)\n\
         end if\n\
       end tell\n\
     end if\n\
     return \"\""
}

fn parse_focus(out: &str) -> Option<(&str, &str)> {
    let out = out.trim_end_matches(['\n', '\r']);
    if out.is_empty() {
        return None;
    }
    Some(out.split_once('\n').unwrap_or((out, "")))
}

/// "◐ Name" → "Name"; any other title is returned whole.
fn strip_glyph(title: &str) -> &str {
    let mut chars = title.chars();
    match (chars.next(), chars.next()) {
        (Some(g), Some(' ')) if !g.is_alphanumeric() => chars.as_str(),
        _ => title,
    }
}

/// Only if no other session shares the name.
fn unique_name_for_pid(json: &str, pid: u32) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let entries = v.as_array()?;
    let name_of = |e: &serde_json::Value| e.get("name")?.as_str().map(str::to_owned);
    let mine = entries
        .iter()
        .find(|e| e.get("pid").and_then(|p| p.as_u64()) == Some(u64::from(pid)))?;
    let name = name_of(mine).filter(|n| !n.is_empty())?;
    let same = entries.iter().filter(|e| name_of(e).as_deref() == Some(&name));
    (same.count() == 1).then_some(name)
}

/// `claude agents --json` (no network, ~0.1 s).
fn agents_json() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let bin = [
        format!("{home}/.local/bin/claude"),
        "/opt/homebrew/bin/claude".into(),
        "/usr/local/bin/claude".into(),
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())?;
    let out = Command::new(bin).args(["agents", "--json"]).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn tty_matches(focused: &str, tty: Option<&str>) -> bool {
    tty.is_some_and(|t| !t.is_empty() && t == focused)
}

/// Any other terminal: bring it to the front by app name.
pub struct GenericJumper;

fn app_name(term_program: Option<&str>) -> Option<&'static str> {
    match term_program?.to_ascii_lowercase().as_str() {
        "iterm.app" => Some("iTerm"),
        "apple_terminal" => Some("Terminal"),
        "vscode" => Some("Visual Studio Code"),
        "warpterminal" => Some("Warp"),
        "ghostty" => Some("Ghostty"),
        _ => None,
    }
}

impl TerminalJumper for GenericJumper {
    fn can_handle(&self, _term_program: Option<&str>) -> bool {
        true
    }

    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError> {
        if let Some(app) = app_name(target.term_program.as_deref()) {
            osascript(&format!("tell application {} to activate", as_literal(app)))?;
        }
        Ok(JumpOutcome::AppOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_ghostty() {
        assert!(GhosttyJumper.can_handle(Some("ghostty")));
        assert!(!GhosttyJumper.can_handle(Some("iTerm.app")));
        assert!(!GhosttyJumper.can_handle(None));
    }

    #[test]
    fn escapes_literals() {
        assert_eq!(as_literal(r#"/a "b"\c"#), r#""/a \"b\"\\c""#);
    }

    #[test]
    fn script_requires_unique_match() {
        let s = unique_script("tty", "/dev/ttys004");
        assert!(s.contains(r#"whose tty is "/dev/ttys004""#));
        assert!(s.contains("(count of ts) is 1"));
    }

    #[test]
    fn focus_script_checks_ghostty_runs_before_tell() {
        let s = focused_script();
        let guard = s.find("is running").expect("missing the `is running` guard");
        let tell = s.find("tell application").expect("missing the `tell`");
        assert!(guard < tell, "the guard must come before `tell`");
    }

    #[test]
    fn focus_script_requires_frontmost_and_focused_terminal() {
        let s = focused_script();
        assert!(s.contains("frontmost"));
        assert!(s.contains("focused terminal of selected tab of front window"));
        assert!(s.contains("tty of t") && s.contains("name of t"));
    }

    #[test]
    fn tty_matches_only_if_equal_and_not_empty() {
        assert!(tty_matches("/dev/ttys007", Some("/dev/ttys007")));
        assert!(!tty_matches("/dev/ttys007", Some("/dev/ttys004")));
        assert!(!tty_matches("", Some("")));
        assert!(!tty_matches("", Some("/dev/ttys007")));
        assert!(!tty_matches("/dev/ttys007", None));
    }

    #[test]
    fn parses_focus_output() {
        assert_eq!(parse_focus(""), None);
        assert_eq!(
            parse_focus("/dev/ttys000\n◐ Fix open pill\n"),
            Some(("/dev/ttys000", "◐ Fix open pill"))
        );
        assert_eq!(parse_focus("/dev/ttys000"), Some(("/dev/ttys000", "")));
    }

    #[test]
    fn strips_title_glyph() {
        assert_eq!(strip_glyph("◐ Fix open pill"), "Fix open pill");
        assert_eq!(strip_glyph("plain"), "plain");
        assert_eq!(strip_glyph("a b"), "a b");
        assert_eq!(strip_glyph("◐"), "◐");
        assert_eq!(strip_glyph("◐ "), "");
    }

    const AGENTS: &str = r#"[{"id":"a","name":"Phase 5","state":"blocked"},
        {"pid":69647,"id":"b","name":"Fix open pill","state":"working"},
        {"pid":7,"id":"c","name":"Dup"},{"pid":8,"id":"d","name":"Dup"},
        {"pid":9,"id":"e"}]"#;

    #[test]
    fn unique_name_by_pid() {
        let n = |pid| unique_name_for_pid(AGENTS, pid);
        assert_eq!(n(69647).as_deref(), Some("Fix open pill"));
        assert_eq!(n(7), None); // duplicate name
        assert_eq!(n(9), None); // no name
        assert_eq!(n(1), None); // missing pid
        assert_eq!(unique_name_for_pid("no json", 7), None);
    }

    fn target(tty: Option<&str>, pid: Option<u32>) -> JumpTarget {
        JumpTarget {
            tty: tty.map(Into::into),
            pid,
            ..Default::default()
        }
    }

    #[test]
    fn focus_by_tty_without_spawning_claude() {
        let t = target(Some("/dev/ttys000"), Some(69647));
        assert!(is_focused_with("/dev/ttys000\nx", &t, || panic!("must not be spawned")));
    }

    #[test]
    fn background_focus_by_name() {
        let t = target(Some("/dev/ttys099"), Some(69647));
        let agents = || Some(AGENTS.to_string());
        assert!(is_focused_with("/dev/ttys000\n◐ Fix open pill", &t, agents));
        assert!(!is_focused_with("/dev/ttys000\n◐ Phase 5", &t, agents));
        assert!(!is_focused_with("/dev/ttys000\n", &t, agents));
        assert!(!is_focused_with("/dev/ttys000\n◐ Fix open pill", &t, || None));
    }

    #[test]
    fn no_focus_when_nothing_to_compare() {
        let empty = JumpTarget::default();
        assert!(!is_focused_with("", &target(Some("/dev/ttys000"), None), || None));
        assert!(!is_focused_with("/dev/ttys000\nx", &empty, || None));
        assert!(!is_focused_with("/dev/ttys000\nx", &target(Some(""), None), || None));
    }

    /// Manual smoke test: `cargo test -p pilas-jump -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn smoke_real_claude_agents() {
        let json = agents_json().expect("claude agents --json failed");
        println!("{json}");
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
    }

    #[test]
    fn app_names() {
        assert_eq!(app_name(Some("iTerm.app")), Some("iTerm"));
        assert_eq!(app_name(Some("unknown")), None);
    }
}
