//! Temperatura de la caché del prompt de la conversación principal (docs/SPEC.md, "Temperatura de caché").
//!
//! El reloj de cada sesión lo mantiene [`crate::session`]; aquí solo está la regla pura que
//! lo convierte en un estado.

use serde::Serialize;

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
}
