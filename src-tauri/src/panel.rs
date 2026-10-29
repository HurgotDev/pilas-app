//! Isla como panel del notch (solo macOS; en el resto, no-op).
//!
//! La ventana "island" se convierte en `NSPanel` (sin robar foco, sobre la barra
//! de menú, en todos los Spaces) y toma exactamente el tamaño del modo visible:
//! fuera de esa zona los clics llegan a la app de debajo.
//!
//! La geometría es pura y se prueba sin AppKit; las coordenadas son las de
//! AppKit (origen abajo a la izquierda).

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use serde::{Deserialize, Serialize};

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

/// Pantalla donde vive la isla. Se guarda en los ajustes: `"main"`, `"notch"` o `{ "named": "<nombre>" }`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
    use objc2::{define_class, ffi, msg_send, AnyThread, ClassType, DefinedClass, MainThreadMarker};
    use objc2_app_kit::{
        NSApplication, NSApplicationDidBecomeActiveNotification, NSApplicationDidChangeScreenParametersNotification,
        NSEvent, NSMainMenuWindowLevel, NSPanel, NSScreen, NSTrackingArea, NSTrackingAreaOptions, NSWindow,
        NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize};
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::rc::Rc;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

    /// Token del observador de notificaciones que devuelve `addObserverForName…`.
    type Observer = Retained<ProtocolObject<dyn NSObjectProtocol>>;

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

    struct HoverOwnerIvars {
        app: AppHandle,
    }

    define_class!(
        /// Dueño del área de seguimiento: avisa a la UI cuando el cursor entra o sale de la ventana.
        #[unsafe(super(NSObject))]
        #[name = "PilasHoverOwner"]
        #[ivars = HoverOwnerIvars]
        struct HoverOwner;

        impl HoverOwner {
            #[unsafe(method(mouseEntered:))]
            fn mouse_entered(&self, _event: &NSEvent) {
                let _ = self.ivars().app.emit("pilas://hover", true);
            }

            #[unsafe(method(mouseExited:))]
            fn mouse_exited(&self, _event: &NSEvent) {
                let _ = self.ivars().app.emit("pilas://hover", false);
            }
        }
    );

    /// Área de seguimiento sobre todo el contenido: funciona aunque la app no esté activa
    /// (WKWebView no emite mouseenter/mouseleave del DOM en ese caso).
    ///
    /// # Safety
    /// Debe llamarse en el hilo principal con la ventana ya convertida en panel.
    unsafe fn install_hover_tracking(window: &NSWindow, app: AppHandle) {
        let Some(view) = window.contentView() else { return };
        let owner = HoverOwner::alloc().set_ivars(HoverOwnerIvars { app });
        let owner: Retained<HoverOwner> = msg_send![super(owner), init];
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let owner_obj: &AnyObject = &owner;
        let area = NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
            options,
            Some(owner_obj),
            None,
        );
        view.addTrackingArea(&area);
        // El área no retiene a su dueño: el dueño vive con la app.
        std::mem::forget(owner);
    }

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
            install_hover_tracking(w, window.app_handle().clone());
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

    /// Trae al frente la ventana `label` y la deja como key con la app activa. WebKit toma su
    /// estado activo del momento en que la ventana pasa a ser key, y `activate` es asíncrono:
    /// si ya está activa se hace key directo; si no, se muestra sin foco y se hace key al
    /// terminar la activación. En la cola del hilo principal, para que corra después del
    /// `build` pendiente (la isla no usa esto: nunca roba foco).
    pub fn bring_to_front(app: &AppHandle, label: &'static str) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let (Some(mtm), Some(window)) = (MainThreadMarker::new(), handle.get_webview_window(label)) else {
                return;
            };
            let Ok(ptr) = window.ns_window() else { return };
            // SAFETY: `ptr` es el NSWindow de `label` y estamos en el hilo principal.
            let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            let ns_app = NSApplication::sharedApplication(mtm);
            if ns_app.isActive() {
                w.makeKeyAndOrderFront(None);
                return;
            }
            // Visible aunque macOS rechace la activación.
            w.orderFrontRegardless();
            // `activate()` exige macOS 14+ y el mínimo es 13.
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
            // Observador de un solo disparo: se quita a sí mismo al activarse la app.
            let token: Rc<RefCell<Option<Observer>>> = Rc::default();
            let slot = token.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                let Some(token) = slot.borrow_mut().take() else { return };
                // SAFETY: el token vino de `addObserverForName…` del mismo centro.
                unsafe { NSNotificationCenter::defaultCenter().removeObserver(token.as_ref()) };
                if let Some(window) = handle.get_webview_window(label) {
                    if let Ok(ptr) = window.ns_window() {
                        // SAFETY: igual que arriba; el bloque corre en el hilo principal.
                        let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                        w.makeKeyAndOrderFront(None);
                    }
                }
            });
            // SAFETY: el centro copia el bloque; el token vive en `token` hasta que se dispare.
            let observer = unsafe {
                NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                    Some(NSApplicationDidBecomeActiveNotification),
                    None,
                    None,
                    &block,
                )
            };
            *token.borrow_mut() = Some(observer);
        });
    }

    /// Nombres de las pantallas (`localizedName`), en el orden de `NSScreen.screens`.
    /// Pide el dato al hilo principal; no llamar desde él (bloquearía).
    pub fn screen_names(app: &AppHandle) -> Vec<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let names = MainThreadMarker::new()
                .map(read_screens)
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.name)
                .collect();
            let _ = tx.send(names);
        });
        rx.recv().unwrap_or_default()
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
    use tauri::{AppHandle, Manager, WebviewWindow};

    pub fn make_panel(_window: &WebviewWindow) {}

    pub fn place(_app: &AppHandle, _screen: ScreenChoice, _mode: IslandMode, _open_height: Option<f64>) {}

    pub fn bring_to_front(app: &AppHandle, label: &'static str) {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    pub fn watch_screens(_on_change: impl Fn() + 'static) {}

    pub fn screen_names(_app: &AppHandle) -> Vec<String> {
        Vec::new()
    }
}

pub use imp::{bring_to_front, make_panel, place, screen_names, watch_screens};

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
