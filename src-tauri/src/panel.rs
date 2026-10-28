//! Isla como panel del notch (solo macOS; en el resto, no-op).
//!
//! La ventana "island" se convierte en `NSPanel` (sin robar foco, sobre la barra
//! de menú, en todos los Spaces) y toma exactamente el tamaño del modo visible:
//! fuera de esa zona los clics llegan a la app de debajo.
//!
//! La geometría es pura y se prueba sin AppKit; las coordenadas son las de
//! AppKit (origen abajo a la izquierda).

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use serde::Deserialize;

const OPEN_W: f64 = 420.0;
const OPEN_MAX_H: f64 = 360.0;
const PILL_H: f64 = 32.0;
const HIDDEN_H: f64 = 6.0;
/// Ancho de la píldora sin notch (monitores externos).
const PILL_NO_NOTCH_W: f64 = 180.0;
/// Margen a cada lado del notch para los iconos de la píldora.
const PILL_PAD: f64 = 48.0;

/// Modo visible de la isla; lo fija la UI con `set_island_mode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IslandMode {
    #[default]
    Hidden,
    Pill,
    Open,
}

/// Pantalla donde vive la isla. Por ahora solo `Main`; M7 lo leerá de los ajustes.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ScreenChoice {
    /// La que tiene la barra de menú (índice 0 de `NSScreen.screens`).
    #[default]
    Main,
    /// La primera con notch; si no hay ninguna, la principal.
    Notch,
    /// Por `localizedName`; si no existe, la principal.
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenInfo {
    pub name: String,
    pub frame: Rect,
    /// Borde superior de `visibleFrame` (bajo la barra de menú).
    pub visible_top: f64,
    /// Ancho del notch, si la pantalla lo tiene.
    pub notch: Option<f64>,
}

/// Índice de la pantalla elegida; `screens` no puede estar vacío.
pub fn pick_screen(choice: &ScreenChoice, screens: &[ScreenInfo]) -> usize {
    let found = match choice {
        ScreenChoice::Main => return 0,
        ScreenChoice::Notch => screens.iter().position(|s| s.notch.is_some()),
        ScreenChoice::Named(name) => screens.iter().position(|s| &s.name == name),
    };
    found.unwrap_or(0)
}

fn pill_width(screen: &ScreenInfo) -> f64 {
    screen.notch.map_or(PILL_NO_NOTCH_W, |n| n + 2.0 * PILL_PAD)
}

/// Marco de la isla en coordenadas de AppKit, centrado en X.
pub fn island_frame(screen: &ScreenInfo, mode: IslandMode, open_height: f64) -> Rect {
    let pill_w = pill_width(screen);
    let (w, h) = match mode {
        IslandMode::Hidden => (pill_w, HIDDEN_H),
        IslandMode::Pill => (pill_w, PILL_H),
        IslandMode::Open => (OPEN_W, open_height.clamp(0.0, OPEN_MAX_H)),
    };
    // Con notch nace del borde superior de la pantalla; sin él, cuelga bajo la barra de menú.
    let top = match screen.notch {
        Some(_) => screen.frame.y + screen.frame.h,
        None => screen.visible_top,
    };
    Rect {
        x: screen.frame.x + (screen.frame.w - w) / 2.0,
        y: top - h,
        w,
        h,
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{island_frame, pick_screen, IslandMode, Rect, ScreenChoice, ScreenInfo, OPEN_MAX_H};
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{define_class, ffi, ClassType, MainThreadMarker};
    use objc2_app_kit::{
        NSApplicationDidChangeScreenParametersNotification, NSMainMenuWindowLevel, NSPanel, NSScreen, NSWindow,
        NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize};
    use std::ptr::NonNull;
    use tauri::{AppHandle, Manager, WebviewWindow};

    define_class!(
        /// NSWindow que puede ser clave (recibir teclado) sin activar la app.
        #[unsafe(super(NSPanel, NSWindow, objc2_app_kit::NSResponder, objc2::runtime::NSObject))]
        #[name = "PilasPanel"]
        struct PilasPanel;

        impl PilasPanel {
            #[unsafe(method(canBecomeKeyWindow))]
            fn can_become_key_window(&self) -> bool {
                true
            }
        }
    );

    fn from_ns(r: NSRect) -> Rect {
        Rect {
            x: r.origin.x,
            y: r.origin.y,
            w: r.size.width,
            h: r.size.height,
        }
    }

    fn to_ns(r: &Rect) -> NSRect {
        NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
    }

    fn read_screens(mtm: MainThreadMarker) -> Vec<ScreenInfo> {
        NSScreen::screens(mtm)
            .iter()
            .map(|s| {
                let frame = s.frame();
                let visible = s.visibleFrame();
                // Con notch, la barra de menú deja huecos a izquierda y derecha: el notch es lo que queda.
                let notch = (s.safeAreaInsets().top > 0.0).then(|| {
                    frame.size.width - s.auxiliaryTopLeftArea().size.width - s.auxiliaryTopRightArea().size.width
                });
                ScreenInfo {
                    name: s.localizedName().to_string(),
                    frame: from_ns(frame),
                    visible_top: visible.origin.y + visible.size.height,
                    notch,
                }
            })
            .collect()
    }

    /// Convierte la ventana en panel. Llamar desde el hilo principal (setup).
    pub fn make_panel(window: &WebviewWindow) {
        let Ok(ptr) = window.ns_window() else { return };
        // SAFETY: `ptr` es el NSWindow de la ventana; PilasPanel solo añade un método,
        // así que el cambio de clase es válido. Hilo principal por contrato.
        unsafe {
            ffi::object_setClass(ptr as *mut AnyObject, PilasPanel::class());
            let w: &NSWindow = &*(ptr as *const NSWindow);
            w.setStyleMask(w.styleMask() | NSWindowStyleMask::NonactivatingPanel);
            w.setLevel(NSMainMenuWindowLevel + 2);
            w.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            w.setHidesOnDeactivate(false);
            w.setAcceptsMouseMovedEvents(true);
        }
    }

    /// Recoloca la isla en la pantalla elegida. No roba foco (nunca `makeKey`).
    pub fn place(app: &AppHandle, screen: ScreenChoice, mode: IslandMode, open_height: Option<f64>) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let (Some(mtm), Some(window)) = (MainThreadMarker::new(), handle.get_webview_window("island")) else {
                return;
            };
            let Ok(ptr) = window.ns_window() else { return };
            let screens = read_screens(mtm);
            if screens.is_empty() {
                return;
            }
            let info = &screens[pick_screen(&screen, &screens)];
            let frame = island_frame(info, mode, open_height.unwrap_or(OPEN_MAX_H));
            // SAFETY: `ptr` sigue siendo el NSWindow de "island" y estamos en el hilo principal.
            let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            w.setFrame_display(to_ns(&frame), true);
        });
    }

    /// Llama a `on_change` cuando cambian las pantallas (monitor conectado, resolución…).
    pub fn watch_screens(on_change: impl Fn() + 'static) {
        let block = RcBlock::new(move |_: NonNull<NSNotification>| on_change());
        // SAFETY: el centro de notificaciones copia el bloque; el observador vive con la app.
        let token = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                None,
                &block,
            )
        };
        std::mem::forget(token);
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{IslandMode, ScreenChoice};
    use tauri::{AppHandle, WebviewWindow};

    pub fn make_panel(_window: &WebviewWindow) {}

    pub fn place(_app: &AppHandle, _screen: ScreenChoice, _mode: IslandMode, _open_height: Option<f64>) {}

    pub fn watch_screens(_on_change: impl Fn() + 'static) {}
}

pub use imp::{make_panel, place, watch_screens};

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(name: &str, x: f64, notch: Option<f64>) -> ScreenInfo {
        ScreenInfo {
            name: name.to_string(),
            frame: Rect {
                x,
                y: 0.0,
                w: 1512.0,
                h: 982.0,
            },
            visible_top: 957.0,
            notch,
        }
    }

    #[test]
    fn main_es_siempre_el_indice_cero() {
        let screens = [screen("externa", 1512.0, None), screen("interna", 0.0, Some(200.0))];
        assert_eq!(pick_screen(&ScreenChoice::Main, &screens), 0);
    }

    #[test]
    fn notch_elige_la_primera_con_notch_o_la_principal() {
        let screens = [screen("externa", 1512.0, None), screen("interna", 0.0, Some(200.0))];
        assert_eq!(pick_screen(&ScreenChoice::Notch, &screens), 1);
        let sin_notch = [screen("externa", 0.0, None), screen("otra", 1512.0, None)];
        assert_eq!(pick_screen(&ScreenChoice::Notch, &sin_notch), 0);
    }

    #[test]
    fn named_busca_por_nombre_o_cae_a_la_principal() {
        let screens = [screen("interna", 0.0, Some(200.0)), screen("Studio", 1512.0, None)];
        let elegida = ScreenChoice::Named("Studio".into());
        assert_eq!(pick_screen(&elegida, &screens), 1);
        let inexistente = ScreenChoice::Named("Nada".into());
        assert_eq!(pick_screen(&inexistente, &screens), 0);
    }

    #[test]
    fn con_notch_la_pildora_es_notch_mas_margenes_y_pega_al_borde() {
        let s = screen("interna", 0.0, Some(200.0));
        let f = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(f.w, 296.0);
        assert_eq!(f.h, 32.0);
        assert_eq!(f.x, (1512.0 - 296.0) / 2.0);
        assert_eq!(f.y + f.h, 982.0);
    }

    #[test]
    fn sin_notch_la_pildora_cuelga_de_la_barra_de_menu() {
        let s = screen("externa", 1512.0, None);
        let f = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(f.w, 180.0);
        assert_eq!(f.x, 1512.0 + (1512.0 - 180.0) / 2.0);
        assert_eq!(f.y + f.h, 957.0);
    }

    #[test]
    fn oculta_mide_seis_de_alto_con_el_ancho_de_la_pildora() {
        let s = screen("interna", 0.0, Some(200.0));
        let oculta = island_frame(&s, IslandMode::Hidden, 0.0);
        let pill = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(oculta.h, 6.0);
        assert_eq!(oculta.w, pill.w);
        assert_eq!(oculta.y + oculta.h, 982.0);
    }

    #[test]
    fn abierta_es_420_y_la_altura_se_acota_a_360() {
        let s = screen("interna", 0.0, Some(200.0));
        let alta = island_frame(&s, IslandMode::Open, 500.0);
        assert_eq!((alta.w, alta.h), (420.0, 360.0));
        let baja = island_frame(&s, IslandMode::Open, 120.0);
        assert_eq!(baja.h, 120.0);
        assert_eq!(baja.x, (1512.0 - 420.0) / 2.0);
    }
}
