//! Temperatura de la caché del prompt de la conversación principal (docs/SPEC.md, "Temperatura de caché").
//!
//! El reloj de cada sesión lo mantiene [`crate::session`]; aquí solo está la regla pura que
//! lo convierte en un estado.

use serde::Serialize;
use serde_json::Value;

/// TTL de respaldo cuando no sabemos cuál usa la sesión (5 min).
pub const FALLBACK_TTL_S: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheTemp {
    Caliente,
    Enfriandose,
    Fria,
    /// Se marca con la compactación por inactividad (la añade una tarea posterior).
    Compactada,
}

/// Reloj de caché de una sesión. Por defecto vacío: sin estado hasta la primera petición.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheClock {
    /// Milisegundos desde UNIX_EPOCH de la última petición de la conversación principal.
    pub last_ms: Option<u64>,
    /// TTL en segundos; `None` = usar [`FALLBACK_TTL_S`].
    pub ttl_s: Option<u64>,
    /// El TTL viene de la propia sesión y no es una estimación.
    pub ttl_detected: bool,
    /// La última compactación fue por inactividad (tarea posterior).
    pub idle_compacted: bool,
    /// La caché está apagada (tarea posterior).
    pub caching_off: bool,
}

/// TTL efectivo de un reloj, en segundos.
pub fn ttl_of(clock: &CacheClock) -> u64 {
    clock.ttl_s.unwrap_or(FALLBACK_TTL_S)
}

/// Instante (ms desde UNIX_EPOCH) en el que la caché expira, si hay una petición.
pub fn expires_ms(clock: &CacheClock) -> Option<u64> {
    clock.last_ms.map(|last| last + ttl_of(clock) * 1000)
}

/// Estado de la caché en `now_ms`. `warn_s` es el umbral de aviso ("enfriándose").
pub fn temperature(clock: &CacheClock, now_ms: u64, warn_s: u64) -> Option<CacheTemp> {
    if clock.caching_off {
        return None;
    }
    let expires = expires_ms(clock)?;
    if clock.idle_compacted {
        return Some(CacheTemp::Compactada);
    }
    // Con TTL de 5 min y umbral de 5 min o más, la sesión no debe nacer "enfriándose".
    let warn_ms = warn_s.min(ttl_of(clock) / 2) as i64 * 1000;
    let remaining = expires as i64 - now_ms as i64;
    Some(if remaining <= 0 {
        CacheTemp::Fria
    } else if remaining <= warn_ms {
        CacheTemp::Enfriandose
    } else {
        CacheTemp::Caliente
    })
}

/// Lo que dice la cola del transcript JSONL sobre la caché de la conversación principal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptCache {
    /// `timestamp` (ms desde UNIX_EPOCH) de la respuesta principal más reciente.
    pub last_ts_ms: Option<u64>,
    /// TTL de la escritura más reciente con tokens de creación.
    pub ttl_s: Option<u64>,
    /// Alguna respuesta leyó o escribió caché.
    pub caching_observed: bool,
}

/// Lee la cola del transcript de abajo arriba. El formato no es un contrato: todo es tolerante.
pub fn parse_transcript_tail(text: &str) -> TranscriptCache {
    let mut out = TranscriptCache::default();
    let num = |v: Option<&Value>, k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_u64).unwrap_or(0);
    for line in text.lines().rev() {
        // La primera línea de un trozo suele venir cortada: se salta como cualquier inválida.
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("assistant")
            || v.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let Some(usage) = v.get("message").and_then(|m| m.get("usage")) else {
            continue;
        };
        if out.last_ts_ms.is_none() {
            out.last_ts_ms = v
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_utc_ms);
        }
        let creation = usage.get("cache_creation");
        let (h1, m5) = (
            num(creation, "ephemeral_1h_input_tokens"),
            num(creation, "ephemeral_5m_input_tokens"),
        );
        if num(Some(usage), "cache_read_input_tokens") > 0
            || num(Some(usage), "cache_creation_input_tokens") > 0
            || h1 > 0
            || m5 > 0
        {
            out.caching_observed = true;
        }
        // Una línea solo de lectura no decide el TTL: sigue buscando una escritura más antigua.
        if out.ttl_s.is_none() {
            out.ttl_s = if h1 > 0 {
                Some(3600)
            } else if m5 > 0 {
                Some(300)
            } else {
                None
            };
        }
        if out.last_ts_ms.is_some() && out.ttl_s.is_some() {
            break;
        }
    }
    out
}

/// Aplica lo leído del transcript al reloj. Sin respuestas con fecha no cambia nada.
pub fn apply_transcript(clock: &mut CacheClock, t: &TranscriptCache) {
    // shortcut: "hay líneas consideradas" se aproxima con `last_ts_ms`; una línea sin fecha válida no cuenta
    let Some(ts) = t.last_ts_ms else { return };
    clock.last_ms = Some(clock.last_ms.map_or(ts, |l| l.max(ts)));
    if let Some(ttl) = t.ttl_s {
        clock.ttl_s = Some(ttl);
        clock.ttl_detected = true;
    }
    clock.caching_off = !t.caching_observed;
}

/// Cuándo leer el transcript tras un evento (ms de espera), o `None` si no toca.
/// `known_before`: la sesión ya estaba en el store antes de este evento.
pub fn transcript_read_delay_ms(event: &str, payload: &Value, known_before: bool) -> Option<u64> {
    let main = payload.get("agent_id").is_none();
    match event {
        "SessionEnd" => None,
        // Tras `Stop` Claude Code aún puede estar escribiendo la última línea.
        "Stop" if main => Some(1500),
        "SessionStart" if payload.get("source").and_then(Value::as_str) == Some("resume") => Some(0),
        _ if !known_before => Some(0),
        _ => None,
    }
}

/// `YYYY-MM-DDTHH:MM:SS(.fff)Z` a ms desde UNIX_EPOCH. Otros desfases → `None`.
pub fn parse_rfc3339_utc_ms(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.splitn(3, '-').map(str::parse::<i64>);
    let (y, mo, da) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut t = hms.splitn(3, ':').map(str::parse::<i64>);
    let (h, mi, se) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let ms: i64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().ok()?;
    // Días desde 1970-01-01 (algoritmo days_from_civil de H. Hinnant).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60 * 1000 + se * 1000 + ms).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    fn reloj(ttl: Option<u64>) -> CacheClock {
        CacheClock {
            last_ms: Some(T0),
            ttl_s: ttl,
            ..Default::default()
        }
    }

    #[test]
    fn sin_peticion_no_hay_estado() {
        assert_eq!(temperature(&CacheClock::default(), T0, 300), None);
    }

    #[test]
    fn caché_apagada_no_hay_estado() {
        let clock = CacheClock {
            caching_off: true,
            ..reloj(None)
        };
        assert_eq!(temperature(&clock, T0, 300), None);
    }

    #[test]
    fn fronteras_del_aviso_y_del_ttl() {
        // TTL 1 h, aviso 5 min: "enfriándose" desde 3300 s.
        let clock = reloj(Some(3600));
        let aviso = T0 + 3300 * 1000;
        assert_eq!(temperature(&clock, aviso - 1, 300), Some(CacheTemp::Caliente));
        assert_eq!(temperature(&clock, aviso, 300), Some(CacheTemp::Enfriandose));
        let fin = T0 + 3600 * 1000;
        assert_eq!(temperature(&clock, fin - 1, 300), Some(CacheTemp::Enfriandose));
        assert_eq!(temperature(&clock, fin, 300), Some(CacheTemp::Fria));
    }

    #[test]
    fn tope_ttl_medio_con_fallback() {
        // TTL de respaldo 300 s y umbral 300 s: el tope ttl/2 deja el aviso en los últimos 150 s.
        let clock = reloj(None);
        assert_eq!(temperature(&clock, T0 + 149_999, 300), Some(CacheTemp::Caliente));
        assert_eq!(temperature(&clock, T0 + 150_000, 300), Some(CacheTemp::Enfriandose));
        assert_eq!(temperature(&clock, T0 + 299_999, 300), Some(CacheTemp::Enfriandose));
        assert_eq!(temperature(&clock, T0 + 300_000, 300), Some(CacheTemp::Fria));
    }

    #[test]
    fn compactada_gana_a_la_temperatura() {
        let clock = CacheClock {
            idle_compacted: true,
            ..reloj(Some(3600))
        };
        assert_eq!(temperature(&clock, T0, 300), Some(CacheTemp::Compactada));
    }

    #[test]
    fn expira_en_ultima_mas_ttl() {
        assert_eq!(expires_ms(&reloj(Some(120))), Some(T0 + 120_000));
        assert_eq!(expires_ms(&reloj(None)), Some(T0 + FALLBACK_TTL_S * 1000));
        assert_eq!(expires_ms(&CacheClock::default()), None);
    }

    fn linea(ts: &str, sidechain: bool, h1: u64, m5: u64, read: u64) -> String {
        serde_json::json!({
            "type": "assistant", "isSidechain": sidechain, "timestamp": ts,
            "message": { "usage": {
                "cache_creation": { "ephemeral_1h_input_tokens": h1, "ephemeral_5m_input_tokens": m5 },
                "cache_read_input_tokens": read, "cache_creation_input_tokens": h1 + m5,
                "input_tokens": 2, "output_tokens": 120
            } }
        })
        .to_string()
    }

    const TS: &str = "2026-10-30T16:25:06.972Z";
    const TS_MS: u64 = 1_793_377_506_972;

    #[test]
    fn fecha_rfc3339_utc() {
        assert_eq!(parse_rfc3339_utc_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_utc_ms("2000-03-01T00:00:00.5Z"), Some(951_868_800_500));
        assert_eq!(parse_rfc3339_utc_ms(TS), Some(TS_MS));
        assert_eq!(parse_rfc3339_utc_ms("2026-10-30T16:25:06+01:00"), None);
        assert_eq!(parse_rfc3339_utc_ms("basura"), None);
    }

    #[test]
    fn detecta_ttl_de_una_hora() {
        let t = parse_transcript_tail(&linea(TS, false, 3405, 0, 114_266));
        assert_eq!(
            t,
            TranscriptCache {
                last_ts_ms: Some(TS_MS),
                ttl_s: Some(3600),
                caching_observed: true
            }
        );
    }

    #[test]
    fn detecta_ttl_de_cinco_minutos() {
        assert_eq!(parse_transcript_tail(&linea(TS, false, 0, 500, 0)).ttl_s, Some(300));
    }

    #[test]
    fn lectura_sin_escritura_conserva_el_ttl_anterior() {
        let texto = [
            linea("2026-10-30T16:00:00Z", false, 3405, 0, 0),
            linea(TS, false, 0, 0, 114_266),
        ]
        .join("\n");
        let t = parse_transcript_tail(&texto);
        assert_eq!((t.last_ts_ms, t.ttl_s), (Some(TS_MS), Some(3600)));
    }

    #[test]
    fn sidechain_se_ignora_aunque_sea_más_reciente() {
        let texto = [
            linea("2026-10-30T16:00:00Z", false, 3405, 0, 0),
            linea(TS, true, 0, 500, 0),
        ]
        .join("\n");
        let t = parse_transcript_tail(&texto);
        assert_eq!(t.ttl_s, Some(3600));
        assert_eq!(t.last_ts_ms, parse_rfc3339_utc_ms("2026-10-30T16:00:00Z"));
    }

    #[test]
    fn líneas_inválidas_o_cortadas_se_saltan() {
        let buena = linea(TS, false, 0, 500, 0);
        let texto = format!("{}\n{buena}\nno es json\n{{\"type\":\"user\"}}\n", &buena[20..]);
        assert_eq!(parse_transcript_tail(&texto).ttl_s, Some(300));
        assert_eq!(parse_transcript_tail(""), TranscriptCache::default());
    }

    #[test]
    fn uso_a_cero_apaga_la_caché() {
        let t = parse_transcript_tail(&linea(TS, false, 0, 0, 0));
        assert!(!t.caching_observed);
        let mut clock = CacheClock::default();
        apply_transcript(&mut clock, &t);
        assert!(clock.caching_off);
        assert_eq!(temperature(&clock, TS_MS, 300), None);
    }

    #[test]
    fn aplicar_conserva_la_última_petición_más_reciente() {
        let t = parse_transcript_tail(&linea(TS, false, 3405, 0, 0));
        let mut clock = CacheClock {
            last_ms: Some(TS_MS + 5000),
            ..Default::default()
        };
        apply_transcript(&mut clock, &t);
        assert_eq!(clock.last_ms, Some(TS_MS + 5000));
        assert_eq!((clock.ttl_s, clock.ttl_detected), (Some(3600), true));

        let mut viejo = CacheClock {
            last_ms: Some(TS_MS - 5000),
            ..Default::default()
        };
        apply_transcript(&mut viejo, &t);
        assert_eq!(viejo.last_ms, Some(TS_MS));
    }

    #[test]
    fn sin_ttl_nuevo_se_queda_el_anterior_y_sin_líneas_no_cambia() {
        let mut clock = CacheClock {
            ttl_s: Some(3600),
            ttl_detected: true,
            caching_off: true,
            ..Default::default()
        };
        apply_transcript(&mut clock, &TranscriptCache::default());
        assert!(clock.caching_off);
        apply_transcript(&mut clock, &parse_transcript_tail(&linea(TS, false, 0, 0, 10)));
        assert_eq!(
            (clock.ttl_s, clock.ttl_detected, clock.caching_off),
            (Some(3600), true, false)
        );
    }

    #[test]
    fn cuándo_leer_el_transcript() {
        let p = serde_json::json!({});
        assert_eq!(transcript_read_delay_ms("Stop", &p, true), Some(1500));
        assert_eq!(
            transcript_read_delay_ms("Stop", &serde_json::json!({ "agent_id": "a" }), true),
            None
        );
        let resume = serde_json::json!({ "source": "resume" });
        assert_eq!(transcript_read_delay_ms("SessionStart", &resume, true), Some(0));
        assert_eq!(transcript_read_delay_ms("SessionStart", &p, true), None);
        assert_eq!(transcript_read_delay_ms("PreToolUse", &p, false), Some(0));
        assert_eq!(transcript_read_delay_ms("PreToolUse", &p, true), None);
        assert_eq!(transcript_read_delay_ms("SessionEnd", &p, false), None);
    }
}
