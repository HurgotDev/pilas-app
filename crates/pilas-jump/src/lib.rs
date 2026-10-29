//! Saltar a la terminal de una sesión. Regla: fallar cerrado. Si no hay coincidencia segura,
//! solo se trae la app al frente; nunca se enfoca una pestaña equivocada.

use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct JumpTarget {
    pub term_program: Option<String>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    /// Id de terminal de Ghostty cacheado en un salto anterior.
    pub cached_terminal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpOutcome {
    /// Se enfocó la terminal exacta (con su id, para cachearlo).
    Exact { terminal_id: Option<String> },
    /// Solo se trajo la app al frente.
    AppOnly,
}

#[derive(Debug, thiserror::Error)]
pub enum JumpError {
    #[error("no se pudo ejecutar osascript: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("AppleScript falló: {0}")]
    Script(String),
}

pub trait TerminalJumper {
    fn can_handle(&self, term_program: Option<&str>) -> bool;
    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError>;
}

/// Elige el jumper según `TERM_PROGRAM`.
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

/// Escapa un string para un literal AppleScript entre comillas dobles.
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

    /// Enfoca la terminal cuya propiedad `property` (fija, de nuestro código) vale `value`.
    /// Devuelve su id solo si hay exactamente una coincidencia.
    fn focus_unique(property: &str, value: &str) -> Result<Option<String>, JumpError> {
        let id = osascript(&unique_script(property, value))?;
        Ok((!id.is_empty()).then_some(id))
    }
}

/// Script que enfoca la única terminal con `property == value`; si hay 0 o varias, no hace nada.
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
        // Pasos en orden de fiabilidad; un error en un paso no corta los demás.
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

/// ¿Tiene el foco la terminal de esta sesión? Solo Ghostty se puede comprobar;
/// cualquier error cuenta como "no enfocada" para avisar al usuario.
pub fn is_focused(target: &JumpTarget) -> bool {
    if !GhosttyJumper.can_handle(target.term_program.as_deref()) {
        return false;
    }
    let Some(tty) = target.tty.as_deref().filter(|t| !t.is_empty()) else {
        return false;
    };
    osascript(focused_tty_script()).is_ok_and(|focused| tty_matches(&focused, Some(tty)))
}

/// Script que devuelve el tty de la terminal enfocada de Ghostty, o "" si
/// Ghostty no corre o no está al frente. Nunca lanza Ghostty.
fn focused_tty_script() -> &'static str {
    "if application \"Ghostty\" is running then\n\
       tell application \"Ghostty\"\n\
         if frontmost then return (tty of focused terminal of selected tab of front window) as text\n\
       end tell\n\
     end if\n\
     return \"\""
}

/// La sesión tiene el foco solo si el tty coincide exactamente.
fn tty_matches(focused: &str, tty: Option<&str>) -> bool {
    tty.is_some_and(|t| !t.is_empty() && t == focused)
}

/// Cualquier otra terminal: traerla al frente por su nombre de app.
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
    fn elige_ghostty() {
        assert!(GhosttyJumper.can_handle(Some("ghostty")));
        assert!(!GhosttyJumper.can_handle(Some("iTerm.app")));
        assert!(!GhosttyJumper.can_handle(None));
    }

    #[test]
    fn escapa_literales() {
        assert_eq!(as_literal(r#"/a "b"\c"#), r#""/a \"b\"\\c""#);
    }

    #[test]
    fn script_exige_coincidencia_unica() {
        let s = unique_script("tty", "/dev/ttys004");
        assert!(s.contains(r#"whose tty is "/dev/ttys004""#));
        assert!(s.contains("(count of ts) is 1"));
    }

    #[test]
    fn script_foco_comprueba_que_ghostty_corre_antes_de_tell() {
        let s = focused_tty_script();
        let guard = s.find("is running").expect("falta el guard `is running`");
        let tell = s.find("tell application").expect("falta el `tell`");
        assert!(guard < tell, "el guard debe ir antes del `tell`");
    }

    #[test]
    fn script_foco_exige_frontmost_y_terminal_enfocada() {
        let s = focused_tty_script();
        assert!(s.contains("frontmost"));
        assert!(s.contains("focused terminal of selected tab of front window"));
    }

    #[test]
    fn tty_coincide_solo_si_es_igual_y_no_vacio() {
        assert!(tty_matches("/dev/ttys007", Some("/dev/ttys007")));
        assert!(!tty_matches("/dev/ttys007", Some("/dev/ttys004")));
        assert!(!tty_matches("", Some("")));
        assert!(!tty_matches("", Some("/dev/ttys007")));
        assert!(!tty_matches("/dev/ttys007", None));
    }

    #[test]
    fn sin_foco_si_no_es_ghostty_o_no_hay_tty() {
        let iterm = JumpTarget {
            term_program: Some("iterm.app".into()),
            tty: Some("/dev/ttys007".into()),
            ..Default::default()
        };
        assert!(!is_focused(&iterm));
        let sin_tty = JumpTarget {
            term_program: Some("ghostty".into()),
            tty: None,
            ..Default::default()
        };
        assert!(!is_focused(&sin_tty));
    }

    #[test]
    fn nombres_de_apps() {
        assert_eq!(app_name(Some("iTerm.app")), Some("iTerm"));
        assert_eq!(app_name(Some("desconocida")), None);
    }
}
