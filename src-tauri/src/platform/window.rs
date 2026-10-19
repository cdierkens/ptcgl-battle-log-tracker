use std::cell::RefCell;
use std::sync::Once;

use tracing::warn;

use crate::ports::{Frame, FrameCapturer, WindowInfo, WindowProvider};

/// Finds the PTCG Live window by title, using `xcap`.
pub struct XcapWindowProvider {
    title_hint: String,
}

impl XcapWindowProvider {
    pub fn new(title_hint: impl Into<String>) -> Self {
        Self { title_hint: title_hint.into() }
    }
}

impl WindowProvider for XcapWindowProvider {
    fn find_game_window(&self) -> Option<WindowInfo> {
        find(&self.title_hint).map(|(_, info)| info)
    }
}

/// Finds the game window and captures it from the same handle, so each poll
/// enumerates windows once. Single-threaded (engine loop only).
pub struct XcapGame {
    title_hint: String,
    found: RefCell<Option<xcap::Window>>,
}

impl XcapGame {
    pub fn new(title_hint: impl Into<String>) -> Self {
        Self { title_hint: title_hint.into(), found: RefCell::new(None) }
    }
}

impl WindowProvider for XcapGame {
    fn find_game_window(&self) -> Option<WindowInfo> {
        let hit = find(&self.title_hint);
        let info = hit.as_ref().map(|(_, info)| info.clone());
        *self.found.borrow_mut() = hit.map(|(window, _)| window);
        info
    }
}

impl FrameCapturer for XcapGame {
    fn capture(&self, _window: &WindowInfo) -> Option<Frame> {
        let found = self.found.borrow();
        let img = found.as_ref()?.capture_image().ok()?;
        Some(Frame { width: img.width(), height: img.height(), rgba: img.into_raw() })
    }
}

fn find(title_hint: &str) -> Option<(xcap::Window, WindowInfo)> {
    let windows = xcap::Window::all().ok()?;
    for window in windows {
        let title = window.title().unwrap_or_default();
        if !crate::title::matches_title(&title, title_hint) {
            continue;
        }
        if window.is_minimized().unwrap_or(false) {
            continue;
        }
        let pid = window.pid().ok();
        if pid.is_none() {
            static WARNED: Once = Once::new();
            WARNED.call_once(|| warn!("game window pid unavailable; overlays show regardless of focus"));
        }
        let info = info_from(&window, title, pid);
        return Some((window, info));
    }
    None
}

/// Build the window geometry in **points**, the unit the overlay geometry
/// expects (`overlay_geometry` positions every overlay from these numbers).
///
/// macOS already reports points. Windows reports raw `GetWindowRect` pixels, so
/// under DPI scaling the window is `scale` times its logical size. Tauri places
/// overlay windows in logical units, so physical geometry has to be divided by
/// the monitor's scale factor: otherwise every overlay lands `scale` times too
/// far right and down, and its HTML renders into a half-size window.
fn info_from(window: &xcap::Window, title: String, pid: Option<u32>) -> WindowInfo {
    let x = window.x().unwrap_or(0);
    let y = window.y().unwrap_or(0);
    let width = window.width().unwrap_or(0);
    let height = window.height().unwrap_or(0);
    #[cfg(target_os = "windows")]
    let (x, y, width, height) = {
        let scale = window
            .current_monitor()
            .ok()
            .and_then(|monitor| monitor.scale_factor().ok())
            .filter(|scale| *scale > 0.0)
            .unwrap_or(1.0);
        (
            (x as f32 / scale).round() as i32,
            (y as f32 / scale).round() as i32,
            (width as f32 / scale).round() as u32,
            (height as f32 / scale).round() as u32,
        )
    };
    WindowInfo { title, pid, x, y, width, height }
}
