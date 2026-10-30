//! Ajustes de Pilas (`data_dir()/settings.json`) y el LaunchAgent de "Iniciar con el sistema".

use crate::panel::ScreenChoice;
use pilas_core::paths::data_dir;
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

/// Disfraz de la pilita. Un valor desconocido (p. ej. de una versión más nueva) cuenta como `Auto`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skin {
    Classic,
    Halloween,
    Christmas,
    #[default]
    #[serde(other)]
    Auto,
}

/// Mismo contrato que `ui/src/island/api.ts` (snake_case, como el resto de la app).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub screen: ScreenChoice,
    pub sound_enabled: bool,
    pub volume: f64,
    pub open_on_pending: bool,
    pub launch_at_login: bool,
    /// `None` = idioma del sistema.
    pub lang: Option<String>,
    pub onboarded: bool,
    /// Busca actualizaciones al arrancar y cada 24 h (ver `updates.rs`).
    pub check_updates: bool,
    /// Disfraz de temporada; `Auto` cambia según la fecha local (ver `ui/src/skins/index.ts`).
    pub skin: Skin,
    /// Temperatura de la caché del prompt (docs/SPEC.md → Temperatura de caché).
    pub cache_enabled: bool,
    /// Minutos de aviso antes de que la caché se enfríe; solo `CACHE_WARN_OPTIONS`.
    #[serde(deserialize_with = "warn_min")]
    pub cache_warn_min: u32,
    pub cache_sound: bool,
}

/// Umbrales de aviso que ofrece Ajustes.
pub const CACHE_WARN_OPTIONS: [u32; 5] = [1, 2, 5, 10, 15];
const CACHE_WARN_DEFAULT: u32 = 5;

/// Un umbral fuera de la lista (o con otro tipo) cuenta como 5 sin invalidar el resto de los ajustes.
fn warn_min<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| CACHE_WARN_OPTIONS.contains(n))
        .unwrap_or(CACHE_WARN_DEFAULT))
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            screen: ScreenChoice::default(),
            sound_enabled: true,
            volume: 0.5,
            open_on_pending: true,
            launch_at_login: true,
            lang: None,
            onboarded: false,
            check_updates: true,
            skin: Skin::default(),
            cache_enabled: true,
            cache_warn_min: CACHE_WARN_DEFAULT,
            cache_sound: true,
        }
    }
}

fn settings_file() -> Result<PathBuf, String> {
    data_dir()
        .map(|d| d.join("settings.json"))
        .ok_or_else(|| "no se pudo resolver la carpeta de datos".to_string())
}

/// Lee los ajustes; si no existen o no son válidos, devuelve los de por defecto.
pub fn load() -> Settings {
    settings_file()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Guarda los ajustes con escritura atómica (temporal + rename).
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_file()?;
    let dir = path.parent().ok_or("ruta de ajustes sin carpeta")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME no definido")?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/dev.pilas.app.plist"))
}

#[cfg(target_os = "macos")]
fn plist_xml(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>dev.pilas.app</string>\n\
         <key>ProgramArguments</key><array><string>{exe}</string></array>\n\
         <key>RunAtLoad</key><true/>\n\
         </dict></plist>\n"
    )
}

/// ¿Corre desde un .app empaquetado? En `tauri dev` el binario vive en target/.
pub fn in_app_bundle() -> bool {
    std::env::current_exe()
        .map(|e| e.to_string_lossy().contains(".app/Contents/MacOS/"))
        .unwrap_or(false)
}

/// Escribe o quita `~/Library/LaunchAgents/dev.pilas.app.plist` según `enabled`.
#[cfg(target_os = "macos")]
pub fn sync_launch_agent(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    // En desarrollo el binario no vive en un .app: no tocamos el LaunchAgent del usuario.
    if !in_app_bundle() {
        return Ok(());
    }
    let plist = launch_agent_path()?;
    if enabled {
        let dir = plist.parent().ok_or("ruta de LaunchAgent sin carpeta")?;
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(&plist, plist_xml(&exe)).map_err(|e| e.to_string())
    } else {
        match fs::remove_file(&plist) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sync_launch_agent(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn screen_choice_tiene_la_forma_del_contrato() {
        assert_eq!(serde_json::to_value(ScreenChoice::Main).unwrap(), json!("main"));
        assert_eq!(serde_json::to_value(ScreenChoice::Notch).unwrap(), json!("notch"));
        let named = ScreenChoice::Named("Studio".into());
        assert_eq!(serde_json::to_value(&named).unwrap(), json!({ "named": "Studio" }));
    }

    #[test]
    fn campos_que_faltan_toman_el_valor_por_defecto() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.2}"#).unwrap();
        assert_eq!(s.volume, 0.2);
        assert!(s.sound_enabled);
        assert!(!s.onboarded);
        assert!(s.check_updates);
        assert_eq!(s.screen, ScreenChoice::Main);
    }

    #[test]
    fn skin_sin_valor_o_desconocido_es_auto_sin_perder_ajustes() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.2}"#).unwrap();
        assert_eq!(s.skin, Skin::Auto);
        let s: Settings = serde_json::from_str(r#"{"skin":"pirate","volume":0.2,"lang":"es"}"#).unwrap();
        assert_eq!(s.skin, Skin::Auto);
        assert_eq!(s.volume, 0.2);
        assert_eq!(s.lang.as_deref(), Some("es"));
    }

    #[test]
    fn ajustes_de_cache_que_faltan_toman_el_valor_por_defecto() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.2}"#).unwrap();
        assert!(s.cache_enabled);
        assert_eq!(s.cache_warn_min, 5);
        assert!(s.cache_sound);
    }

    #[test]
    fn umbral_de_cache_invalido_es_5_sin_perder_ajustes() {
        for bad in [r#"3"#, r#"0"#, r#""diez""#, r#"-1"#, r#"null"#, r#"99999999999"#] {
            let text = format!(r#"{{"cache_warn_min":{bad},"volume":0.2,"lang":"es","cache_sound":false}}"#);
            let s: Settings = serde_json::from_str(&text).unwrap();
            assert_eq!(s.cache_warn_min, 5, "{bad}");
            assert_eq!(s.volume, 0.2);
            assert_eq!(s.lang.as_deref(), Some("es"));
            assert!(!s.cache_sound);
        }
        for ok in CACHE_WARN_OPTIONS {
            let s: Settings = serde_json::from_str(&format!(r#"{{"cache_warn_min":{ok}}}"#)).unwrap();
            assert_eq!(s.cache_warn_min, ok);
        }
    }

    #[test]
    fn ajustes_ida_y_vuelta() {
        let s = Settings {
            cache_enabled: false,
            cache_warn_min: 15,
            cache_sound: false,
            lang: Some("en".into()),
            ..Settings::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn skin_ida_y_vuelta_en_snake_case() {
        for (skin, text) in [
            (Skin::Auto, "auto"),
            (Skin::Classic, "classic"),
            (Skin::Halloween, "halloween"),
            (Skin::Christmas, "christmas"),
        ] {
            assert_eq!(serde_json::to_value(skin).unwrap(), json!(text));
            assert_eq!(serde_json::from_value::<Skin>(json!(text)).unwrap(), skin);
        }
    }

    #[test]
    fn ids_de_la_ui_coinciden_con_el_enum() {
        // Lista compartida con el registro de skins de la UI (ui/src/skins/index.ts).
        let ids: Vec<String> = serde_json::from_str(include_str!("../../ui/src/skins/ids.json")).unwrap();
        let mut seen = Vec::new();
        for id in &ids {
            let skin: Skin = serde_json::from_value(json!(id)).unwrap();
            assert_ne!(skin, Skin::Auto, "{id} no es una variante de Skin");
            assert_eq!(serde_json::to_value(skin).unwrap(), json!(id));
            assert!(!seen.contains(&skin), "{id} repetido");
            seen.push(skin);
        }
        // Match exhaustivo: una variante nueva no compila hasta contarla aquí.
        let non_auto = |s: Skin| match s {
            Skin::Classic | Skin::Halloween | Skin::Christmas => 1,
            Skin::Auto => 0,
        };
        let variants = [Skin::Classic, Skin::Halloween, Skin::Christmas, Skin::Auto];
        assert_eq!(ids.len(), variants.into_iter().map(non_auto).sum::<usize>());
    }
}
