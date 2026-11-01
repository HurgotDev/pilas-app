//! The island as a notch panel (macOS only; a no-op elsewhere).
//!
//! The "island" window becomes an `NSPanel` (never steals focus, above the menu
//! bar, on every Space) sized exactly to the visible mode, so clicks outside it
//! reach the app below.
//!
//! Geometry is pure and tested without AppKit; coordinates are AppKit's
//! (origin at the bottom left).

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use serde::{Deserialize, Serialize};

const OPEN_W: f64 = 420.0;
const OPEN_MAX_H: f64 = 360.0;
const PILL_H: f64 = 32.0;
const HIDDEN_H: f64 = 6.0;
/// Pill width without a notch (external displays).
const PILL_NO_NOTCH_W: f64 = 180.0;
/// Margin on each side of the notch for the pill icons.
const PILL_PAD: f64 = 48.0;

/// Set by the UI through `set_island_mode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IslandMode {
    #[default]
    Hidden,
    Pill,
    Open,
}

/// Persisted in settings as `"main"`, `"notch"` or `{ "named": "<name>" }`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenChoice {
    /// The one with the menu bar (index 0 of `NSScreen.screens`).
    #[default]
    Main,
    /// The first one with a notch, else the main one.
    Notch,
    /// By `localizedName`, else the main one.
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
    /// Top edge of `visibleFrame` (below the menu bar).
    pub visible_top: f64,
    pub notch: Option<f64>,
}

/// `screens` must not be empty.
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

pub fn island_frame(screen: &ScreenInfo, mode: IslandMode, open_height: f64) -> Rect {
    let pill_w = pill_width(screen);
    let (w, h) = match mode {
        IslandMode::Hidden => (pill_w, HIDDEN_H),
        IslandMode::Pill => (pill_w, PILL_H),
        IslandMode::Open => (OPEN_W, open_height.clamp(0.0, OPEN_MAX_H)),
    };
    // With a notch it grows from the top screen edge; without one it hangs below the menu bar.
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

    type Observer = Retained<ProtocolObject<dyn NSObjectProtocol>>;

    define_class!(
        /// Can become key (receive keyboard input) without activating the app.
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
        /// Tracking area owner: tells the UI when the cursor enters or leaves the window.
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

    /// Works even while the app is inactive, when WKWebView emits no DOM mouseenter/mouseleave.
    ///
    /// # Safety
    /// Must be called on the main thread after the window became a panel.
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
        // The area does not retain its owner, so the owner lives as long as the app.
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
                // With a notch the menu bar leaves areas left and right: the notch is what remains.
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

    /// Call from the main thread (setup).
    pub fn make_panel(window: &WebviewWindow) {
        let Ok(ptr) = window.ns_window() else { return };
        // SAFETY: `ptr` is the window's NSWindow; PilasPanel only adds a method,
        // so the class swap is valid. Main thread by contract.
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

    /// Never steals focus (never `makeKey`).
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
            // SAFETY: `ptr` is still the "island" NSWindow and this runs on the main thread.
            let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            w.setFrame_display(to_ns(&frame), true);
        });
    }

    /// Brings `label` to the front as key with the app active. WebKit takes its active state
    /// from the moment the window becomes key, and `activate` is async: if the app is already
    /// active the window becomes key at once; otherwise it is shown unfocused and made key once
    /// activation finishes. Queued on the main thread so it runs after the pending `build`
    /// (the island never uses this: it never steals focus).
    pub fn bring_to_front(app: &AppHandle, label: &'static str) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let (Some(mtm), Some(window)) = (MainThreadMarker::new(), handle.get_webview_window(label)) else {
                return;
            };
            let Ok(ptr) = window.ns_window() else { return };
            // SAFETY: `ptr` is the NSWindow of `label` and this runs on the main thread.
            let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            let ns_app = NSApplication::sharedApplication(mtm);
            if ns_app.isActive() {
                w.makeKeyAndOrderFront(None);
                return;
            }
            // Visible even if macOS refuses the activation.
            w.orderFrontRegardless();
            // `activate()` needs macOS 14+ and the minimum is 13.
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
            // One-shot observer: removes itself once the app activates.
            let token: Rc<RefCell<Option<Observer>>> = Rc::default();
            let slot = token.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                let Some(token) = slot.borrow_mut().take() else { return };
                // SAFETY: the token came from `addObserverForName…` on the same center.
                unsafe { NSNotificationCenter::defaultCenter().removeObserver(token.as_ref()) };
                if let Some(window) = handle.get_webview_window(label) {
                    if let Ok(ptr) = window.ns_window() {
                        // SAFETY: as above; the block runs on the main thread.
                        let w: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                        w.makeKeyAndOrderFront(None);
                    }
                }
            });
            // SAFETY: the center copies the block; the token lives in `token` until it fires.
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

    /// In `NSScreen.screens` order. Asks the main thread, so never call it from there (deadlock).
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

    pub fn watch_screens(on_change: impl Fn() + 'static) {
        let block = RcBlock::new(move |_: NonNull<NSNotification>| on_change());
        // SAFETY: the notification center copies the block; the observer lives as long as the app.
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
    fn main_is_always_index_zero() {
        let screens = [screen("external", 1512.0, None), screen("internal", 0.0, Some(200.0))];
        assert_eq!(pick_screen(&ScreenChoice::Main, &screens), 0);
    }

    #[test]
    fn notch_picks_first_with_notch_or_main() {
        let screens = [screen("external", 1512.0, None), screen("internal", 0.0, Some(200.0))];
        assert_eq!(pick_screen(&ScreenChoice::Notch, &screens), 1);
        let no_notch = [screen("external", 0.0, None), screen("other", 1512.0, None)];
        assert_eq!(pick_screen(&ScreenChoice::Notch, &no_notch), 0);
    }

    #[test]
    fn named_finds_by_name_or_falls_back_to_main() {
        let screens = [screen("internal", 0.0, Some(200.0)), screen("Studio", 1512.0, None)];
        let chosen = ScreenChoice::Named("Studio".into());
        assert_eq!(pick_screen(&chosen, &screens), 1);
        let missing = ScreenChoice::Named("Nothing".into());
        assert_eq!(pick_screen(&missing, &screens), 0);
    }

    #[test]
    fn with_notch_pill_is_notch_plus_margins_at_top_edge() {
        let s = screen("internal", 0.0, Some(200.0));
        let f = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(f.w, 296.0);
        assert_eq!(f.h, 32.0);
        assert_eq!(f.x, (1512.0 - 296.0) / 2.0);
        assert_eq!(f.y + f.h, 982.0);
    }

    #[test]
    fn without_notch_pill_hangs_below_menu_bar() {
        let s = screen("external", 1512.0, None);
        let f = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(f.w, 180.0);
        assert_eq!(f.x, 1512.0 + (1512.0 - 180.0) / 2.0);
        assert_eq!(f.y + f.h, 957.0);
    }

    #[test]
    fn hidden_is_six_high_with_pill_width() {
        let s = screen("internal", 0.0, Some(200.0));
        let hidden = island_frame(&s, IslandMode::Hidden, 0.0);
        let pill = island_frame(&s, IslandMode::Pill, 0.0);
        assert_eq!(hidden.h, 6.0);
        assert_eq!(hidden.w, pill.w);
        assert_eq!(hidden.y + hidden.h, 982.0);
    }

    #[test]
    fn open_is_420_wide_and_height_clamps_to_360() {
        let s = screen("internal", 0.0, Some(200.0));
        let tall = island_frame(&s, IslandMode::Open, 500.0);
        assert_eq!((tall.w, tall.h), (420.0, 360.0));
        let short = island_frame(&s, IslandMode::Open, 120.0);
        assert_eq!(short.h, 120.0);
        assert_eq!(short.x, (1512.0 - 420.0) / 2.0);
    }
}
