//! Tauri-backed overlay windows: the capture drawer and the Continue shield.

use std::sync::Mutex;

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::overlay_geometry::{drawer_rect, to_screen, ScreenRect};
use crate::ports::{OverlayError, OverlayPresenter, WindowInfo};

pub const DRAWER_LABEL: &str = "capture-drawer";
pub const SHIELD_LABEL: &str = "continue-shield";

pub struct TauriOverlayPresenter {
    app: AppHandle,
    drawer_game: Mutex<Option<WindowInfo>>,
    open: Mutex<bool>,
}

impl TauriOverlayPresenter {
    /// Creates both overlay windows, hidden. Call from Tauri `setup`.
    pub fn new(app: &AppHandle) -> Result<Self, OverlayError> {
        // The drawer takes keyboard focus when clicked (paste); the shield never needs it.
        build_overlay(app, DRAWER_LABEL, true)?;
        build_overlay(app, SHIELD_LABEL, false)?;
        Ok(Self { app: app.clone(), drawer_game: Mutex::new(None), open: Mutex::new(false) })
    }

    fn window(&self, label: &'static str) -> Result<WebviewWindow, OverlayError> {
        self.app.get_webview_window(label).ok_or(OverlayError::WindowUnavailable { label })
    }

    fn place(&self, label: &'static str, rect: ScreenRect) -> Result<(), OverlayError> {
        let window = self.window(label)?;
        let fail = |e: tauri::Error| OverlayError::Window { label, reason: e.to_string() };
        window.set_size(LogicalSize::new(rect.width, rect.height)).map_err(fail)?;
        window.set_position(LogicalPosition::new(rect.x, rect.y)).map_err(fail)?;
        if window.is_visible().map_err(fail)? {
            return Ok(());
        }
        raise(&window, label)
    }

    fn hide(&self, label: &'static str) -> Result<(), OverlayError> {
        self.window(label)?.hide().map_err(|e| OverlayError::Window { label, reason: e.to_string() })
    }
}

fn poisoned(label: &'static str) -> OverlayError {
    OverlayError::Window { label, reason: "overlay state lock poisoned".into() }
}

impl OverlayPresenter for TauriOverlayPresenter {
    fn show_drawer(&self, game: &WindowInfo) -> Result<(), OverlayError> {
        let open = *self.open.lock().map_err(|_| poisoned(DRAWER_LABEL))?;
        *self.drawer_game.lock().map_err(|_| poisoned(DRAWER_LABEL))? = Some(game.clone());
        self.place(DRAWER_LABEL, drawer_rect(game, open))
    }

    fn set_drawer_open(&self, open: bool) -> Result<(), OverlayError> {
        *self.open.lock().map_err(|_| poisoned(DRAWER_LABEL))? = open;
        let game = self.drawer_game.lock().map_err(|_| poisoned(DRAWER_LABEL))?.clone();
        match game {
            Some(game) => self.place(DRAWER_LABEL, drawer_rect(&game, open)),
            None => Ok(()),
        }
    }

    fn hide_drawer(&self) -> Result<(), OverlayError> {
        *self.drawer_game.lock().map_err(|_| poisoned(DRAWER_LABEL))? = None;
        self.hide(DRAWER_LABEL)
    }

    fn show_shield(&self, game: &WindowInfo, local: ScreenRect) -> Result<(), OverlayError> {
        self.place(SHIELD_LABEL, to_screen(game, local))
    }

    fn hide_shield(&self) -> Result<(), OverlayError> {
        self.hide(SHIELD_LABEL)
    }
}

fn build_overlay(app: &AppHandle, label: &'static str, focusable: bool) -> Result<(), OverlayError> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(label)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .resizable(false)
        .focused(false)
        .focusable(focusable)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .build()
        .map_err(|e| OverlayError::Window { label, reason: e.to_string() })?;
    // A plain window still works everywhere except over fullscreen games.
    if let Err(e) = into_panel(&window, label) {
        tracing::warn!("overlay stays a plain window, so it won't show over fullscreen: {e}");
    }
    Ok(())
}

/// How long to wait for the main thread to run an AppKit call.
#[cfg(target_os = "macos")]
const MAIN_THREAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Runs `f` on this window's `NSWindow` on the main thread and waits for it, so
/// failures reach the caller (the engine retries).
#[cfg(target_os = "macos")]
fn with_ns_window(
    window: &WebviewWindow,
    label: &'static str,
    f: impl FnOnce(&objc2_app_kit::NSWindow) -> Result<(), String> + Send + 'static,
) -> Result<(), OverlayError> {
    let (done, result) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
    let target = window.clone();
    // Tauri runs this inline when already on the main thread, so waiting cannot deadlock.
    window
        .run_on_main_thread(move || {
            let outcome = target.ns_window().map_err(|e| e.to_string()).and_then(|ptr| {
                // SAFETY: Tauri returns this window's live NSWindow, and we are on the main thread.
                f(unsafe { &*ptr.cast::<objc2_app_kit::NSWindow>() })
            });
            let _ = done.send(outcome);
        })
        .map_err(|e| OverlayError::Window { label, reason: e.to_string() })?;
    match result.recv_timeout(MAIN_THREAD_TIMEOUT) {
        Ok(outcome) => outcome.map_err(|reason| OverlayError::Window { label, reason }),
        Err(_) => Err(OverlayError::Window { label, reason: "timed out waiting for the main thread".into() }),
    }
}

/// Turns the window into a non-activating `NSPanel`. Since macOS 10.14 a regular
/// app's plain `NSWindow` can't float over another app's fullscreen Space, even
/// with `FullScreenAuxiliary`; a non-activating panel can, and it takes key
/// (for paste) without activating the app.
#[cfg(target_os = "macos")]
fn into_panel(window: &WebviewWindow, label: &'static str) -> Result<(), OverlayError> {
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSStatusWindowLevel, NSWindowCollectionBehavior, NSWindowStyleMask};

    with_ns_window(window, label, move |ns_window| {
        let panel_class = panel_class();
        let object: &AnyObject = ns_window.as_ref();
        if panel_class.instance_size() != object.class().instance_size() {
            return Err("overlay panel class doesn't match the Tauri window layout".into());
        }
        // SAFETY: the panel class is NSPanel (an NSWindow) plus TaoWindow's one `focusable`
        // ivar, so sizes match (checked above); tao looks the ivar up by name, so it still
        // resolves, and the value tao stored carries over in place. TaoWindow's other
        // overrides are dropped: canBecomeKey is re-added below, and its sendEvent
        // (drag-by-background) is unused by overlays.
        unsafe { AnyObject::set_class(object, panel_class) };
        ns_window.setStyleMask(ns_window.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        // Panels hide whenever their app deactivates, and this app is almost never active.
        ns_window.setHidesOnDeactivate(false);
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setCollectionBehavior(
            ns_window.collectionBehavior()
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        Ok(())
    })
}

/// The overlay `NSPanel` subclass. It carries TaoWindow's only ivar, `focusable`,
/// so the layout matches and tao's reads and writes of it keep working.
#[cfg(target_os = "macos")]
fn panel_class() -> &'static objc2::runtime::AnyClass {
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{class, sel};

    extern "C-unwind" fn can_become_key(this: &AnyObject, _: Sel) -> Bool {
        let ivar = this.class().instance_variable(c"focusable").expect("panel class declares `focusable`");
        // SAFETY: the ivar is declared as `Bool` below.
        unsafe { *ivar.load::<Bool>(this) }
    }
    extern "C-unwind" fn no(_: &AnyObject, _: Sel) -> Bool {
        Bool::NO
    }

    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        let mut builder =
            ClassBuilder::new(c"PtcglOverlayPanel", class!(NSPanel)).expect("overlay panel class registered once");
        builder.add_ivar::<Bool>(c"focusable");
        // SAFETY: both selectors are `- (BOOL)` getters on NSWindow.
        unsafe {
            builder.add_method(sel!(canBecomeKeyWindow), can_become_key as extern "C-unwind" fn(_, _) -> _);
            builder.add_method(sel!(canBecomeMainWindow), no as extern "C-unwind" fn(_, _) -> _);
        }
        builder.register()
    })
}

#[cfg(not(target_os = "macos"))]
fn into_panel(_window: &WebviewWindow, _label: &'static str) -> Result<(), OverlayError> {
    Ok(())
}

/// Shows without making the window key, so the game keeps keyboard focus
/// (Tauri's `show()` is `makeKeyAndOrderFront` on macOS).
#[cfg(target_os = "macos")]
fn raise(window: &WebviewWindow, label: &'static str) -> Result<(), OverlayError> {
    with_ns_window(window, label, |ns_window| {
        ns_window.orderFrontRegardless();
        Ok(())
    })
}

#[cfg(not(target_os = "macos"))]
fn raise(window: &WebviewWindow, label: &'static str) -> Result<(), OverlayError> {
    window.show().map_err(|e| OverlayError::Window { label, reason: e.to_string() })
}
