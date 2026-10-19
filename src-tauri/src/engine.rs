//! Overlay engine: watches the game window read-only and drives the capture
//! drawer and Continue shield. It never sends input to the game.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};

use crate::clipboard::{ClipboardAccess, ClipboardError, ClipboardProbe, ClipboardWatch};
use crate::detection::{OcrResultsProbe, ResultsChange, ResultsProbe, ResultsTracker};
use crate::events::CaptureStatus;
use crate::focus::{FocusProbe, FocusTracker};
use crate::overlay_geometry::{shield_rect, ScreenRect};
use crate::overlay_state::{transition, OverlayEvent, OverlayState, ShieldStatus};
use crate::platform::clipboard::SystemClipboard;
use crate::platform::focus::WorkspaceFocusProbe;
use crate::platform::ocr::PlatformTextRecognizer;
use crate::platform::permission::ScreenRecordingAccess;
use crate::platform::window::XcapGame;
use crate::ports::{FrameCapturer, OverlayPresenter, ScreenCaptureAccess, TextRecognizer, WindowInfo, WindowProvider};
use crate::settings::{self, SharedSettings};
use crate::sink::{LogSink, SubmitError, SubmitOutcome};

/// Sleep between polls while the game window exists. A poll costs one window
/// lookup, one screen capture, and one region OCR. Vision's cost is not
/// proportional to crop size: a 512x150 region costs ~33ms while a 1.5Mpx band
/// and a 3.8Mpx whole frame both cost ~250ms, so it flattens well before
/// full-frame size. The encoder is therefore negligible (0.12ms for the
/// 512x150 region) and shrinking the crop further buys almost nothing — the
/// remaining lever is making fewer OCR calls. Results show roughly one poll
/// period after the label renders.
const POLL_FAST_MS: u64 = 50;
const POLL_IDLE_MS: u64 = 1000;
const POLL_PERMISSION_MS: u64 = 5000;
/// Every Nth poll widens the results-tab check to `battle_log_sweep_region`
/// instead of its measured region, as a safety net against the tab rendering
/// somewhere new. A band, not the whole frame: measured back-to-back on a
/// 2560x1496 capture, the band costs 36-58% less than the whole frame it
/// replaces, consistently across runs.
const SWEEP_EVERY: u32 = 15;
const CAPTURE_STATUS_EVENT: &str = "capture-status";
pub const OVERLAY_STATE_EVENT: &str = "overlay://state";

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatePayload {
    pub state: String,
    /// The shield went down because of a clipboard auto-save; `path` backs its Undo.
    pub autosaved: bool,
    pub path: Option<String>,
}

/// Engine state between polls. Rendering is diffed so the presenter is only
/// called when the window, visibility, or shield rect actually changes.
#[derive(Debug)]
pub struct EngineCtx {
    pub state: OverlayState,
    tracker: ResultsTracker,
    focus: FocusTracker,
    /// Game window bounds at the previous poll; a change means it is being dragged or resized.
    last_bounds: Option<(i32, i32, u32, u32)>,
    own_pid: u32,
    trace: Option<PollTrace>,
    polls: u32,
    /// `prompt_after_match`: the shield (and its Continue lookup) only run when on.
    prompts: bool,
    autosave: bool,
    /// A failed or denied read; auto-save stays paused until the setting is turned back on.
    autosave_error: Option<String>,
    clipboard: ClipboardWatch,
    /// Cleared once the shield leaves `Saved` (Undo, next match).
    last_autosave: Option<String>,
    game_focused: bool,
    /// Text recognition has warmed up; until then polls skip OCR.
    recognizer_ready: bool,
    overlay_error: Option<String>,
    rendered_drawer: Option<WindowInfo>,
    rendered_shield: Option<(WindowInfo, ScreenRect)>,
}

impl EngineCtx {
    pub fn new(state: OverlayState, own_pid: u32) -> Self {
        Self {
            state,
            tracker: ResultsTracker::new(),
            focus: FocusTracker::new(),
            last_bounds: None,
            own_pid,
            polls: 0,
            prompts: true,
            autosave: false,
            autosave_error: None,
            clipboard: ClipboardWatch::new(),
            last_autosave: None,
            game_focused: false,
            recognizer_ready: false,
            overlay_error: None,
            rendered_drawer: None,
            rendered_shield: None,
            trace: None,
        }
    }

    /// The last overlay window failure, cleared once rendering succeeds.
    pub fn overlay_error(&self) -> Option<&str> {
        self.overlay_error.as_deref()
    }

    pub fn set_recognizer_ready(&mut self, ready: bool) {
        self.recognizer_ready = ready;
    }

    pub fn set_prompts(&mut self, on: bool) {
        self.prompts = on;
    }

    /// Turning the setting back on clears a paused session.
    pub fn set_autosave(&mut self, on: bool) {
        if on && !self.autosave {
            self.autosave_error = None;
        }
        self.autosave = on;
    }

    pub fn autosave_error(&self) -> Option<&str> {
        self.autosave_error.as_deref()
    }

    pub fn last_autosave(&self) -> Option<&str> {
        self.last_autosave.as_deref()
    }

    /// Saves a battle log copied after the results screen appeared; returns the saved path.
    pub fn autosave(&mut self, clipboard: &impl ClipboardProbe, sink: &dyn LogSink) -> Option<String> {
        let shield = match self.state {
            OverlayState::Results { shield, .. } => Some(shield),
            _ => None,
        };
        if shield != Some(ShieldStatus::Saved) {
            self.last_autosave = None;
        }
        if !self.autosave || self.autosave_error.is_some() || shield.is_none() {
            self.clipboard.disarm();
            return None;
        }
        let count = clipboard.change_count();
        // Baseline on entering results; re-baseline while unfocused so other apps' copies are never read.
        if !self.clipboard.is_armed() || !self.game_focused {
            self.clipboard.arm(count);
            return None;
        }
        if !self.clipboard.changed(count) || shield == Some(ShieldStatus::Saved) {
            return None;
        }
        if clipboard.access() == ClipboardAccess::Denied {
            self.autosave_error = Some(ClipboardError::Denied.to_string());
            return None;
        }
        match clipboard.read_text() {
            Ok(Some(text)) => self.submit_copied(&text, sink),
            Ok(None) => None,
            Err(e) => {
                self.autosave_error = Some(e.to_string());
                None
            }
        }
    }

    fn submit_copied(&mut self, text: &str, sink: &dyn LogSink) -> Option<String> {
        match sink.submit(text) {
            Ok(SubmitOutcome::Saved { path }) => {
                self.apply(OverlayEvent::LogSaved);
                self.last_autosave = Some(path.clone());
                Some(path)
            }
            Ok(SubmitOutcome::AlreadySaved) | Err(SubmitError::NotABattleLog) => None,
            Err(SubmitError::Store(e)) => {
                warn!("clipboard auto-save couldn't store the log: {e}");
                None
            }
        }
    }

    pub fn apply(&mut self, event: OverlayEvent) {
        self.state = transition(self.state, event);
    }

    /// One read-only poll. Returns whether the game window was found.
    pub fn poll<P: OverlayPresenter + ?Sized>(
        &mut self,
        windows: &impl WindowProvider,
        frames: &impl FrameCapturer,
        probe: &impl ResultsProbe,
        focus: &impl FocusProbe,
        presenter: &P,
    ) -> bool {
        let Some(window) = windows.find_game_window() else {
            self.apply(OverlayEvent::WindowLost);
            self.tracker.reset();
            self.last_bounds = None;
            self.trace = None;
            self.render(None, presenter);
            return false;
        };
        let ready = self.recognizer_ready;
        self.apply(if ready { OverlayEvent::WindowFound } else { OverlayEvent::WindowFoundWarmingUp });
        // Detection keeps running while another app is active; only rendering is gated.
        let active_pid = focus.active_pid();
        let focused = self.focus.observe(active_pid, window.pid, self.own_pid);
        // Hide instead of chasing a dragged or resized window; show on the first stable poll.
        let bounds = (window.x, window.y, window.width, window.height);
        let settled = self.last_bounds.is_none_or(|last| last == bounds);
        self.last_bounds = Some(bounds);
        let visible = focused && settled;
        self.game_focused = focused;
        let shown = visible.then_some(&window);
        let frame = if ready { frames.capture(&window) } else { None };
        let tab_visible = frame.as_ref().map(|frame| {
            self.polls = self.polls.wrapping_add(1);
            probe.tab_visible(frame, self.polls.is_multiple_of(SWEEP_EVERY))
        });
        self.record(PollTrace { tab_visible, active_pid, game_pid: window.pid, focused });
        if let Some((frame, tab_visible)) = frame.zip(tab_visible) {
            // Only pay for the drawer OCR when it can matter.
            let drawer_open = !tab_visible && self.tracker.is_present() && probe.drawer_open(&frame);
            match self.tracker.observe(tab_visible, drawer_open) {
                ResultsChange::Appeared => {
                    self.apply(OverlayEvent::ResultsDetected);
                    // Show the drawer now; the Continue lookup waits for the next poll.
                    self.render(shown, presenter);
                    return true;
                }
                ResultsChange::Gone => self.apply(OverlayEvent::ResultsGone),
                ResultsChange::Unchanged => {}
            }
            if self.needs_continue() {
                if let Some(m) = probe.find_continue(&frame) {
                    self.apply(OverlayEvent::ContinueLocated(shield_rect(&window, &frame, &m)));
                }
            }
        }
        self.render(shown, presenter);
        true
    }

    /// Logs the poll's inputs at debug level, only when they change.
    fn record(&mut self, trace: PollTrace) {
        if self.trace != Some(trace) {
            debug!(
                tab_visible = ?trace.tab_visible,
                active_pid = ?trace.active_pid,
                game_pid = ?trace.game_pid,
                focused = trace.focused,
                "poll inputs changed"
            );
            self.trace = Some(trace);
        }
    }

    /// Stop watching (permission missing) and hide the overlays.
    pub fn pause<P: OverlayPresenter + ?Sized>(&mut self, event: OverlayEvent, presenter: &P) {
        self.apply(event);
        // Results seen before the pause must be detected afresh on resume.
        self.tracker.reset();
        self.render(None, presenter);
    }

    fn needs_continue(&self) -> bool {
        self.prompts && matches!(self.state, OverlayState::Results { shield: ShieldStatus::Up, continue_rect: None })
    }

    pub fn render<P: OverlayPresenter + ?Sized>(&mut self, window: Option<&WindowInfo>, presenter: &P) {
        let mut error: Option<String> = None;
        let drawer_visible = self.state.drawer_visible();
        let drawer = window.filter(|_| drawer_visible).cloned();
        if drawer != self.rendered_drawer {
            let result = match &drawer {
                Some(w) => presenter.show_drawer(w),
                None => presenter.hide_drawer(),
            };
            match result {
                Ok(()) => self.rendered_drawer = drawer,
                Err(e) => error = Some(e.to_string()),
            }
        }
        let shield = window.zip(self.state.shield_rect(self.prompts)).map(|(w, r)| (w.clone(), r));
        if shield != self.rendered_shield {
            let result = match &shield {
                Some((w, rect)) => presenter.show_shield(w, *rect),
                None => presenter.hide_shield(),
            };
            match result {
                Ok(()) => self.rendered_shield = shield,
                Err(e) => error = Some(e.to_string()),
            }
        }
        // Failed calls retry every poll; only log when the error changes.
        if error != self.overlay_error {
            if let Some(e) = &error {
                warn!("overlay window: {e}");
            }
            self.overlay_error = error;
        }
    }
}

/// What one poll saw, for diagnosing missed detections and focus gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PollTrace {
    /// `None` when no frame was captured.
    tab_visible: Option<bool>,
    active_pid: Option<u32>,
    game_pid: Option<u32>,
    focused: bool,
}

/// Background loop. `inbox` carries `LogSaved` / `Dismissed` from commands.
pub fn run(
    app: AppHandle,
    state_cell: Arc<Mutex<OverlayState>>,
    presenter: Arc<dyn OverlayPresenter>,
    inbox: Receiver<OverlayEvent>,
    settings: SharedSettings,
    sink: Arc<dyn LogSink>,
) {
    let game = XcapGame::new(crate::title::GAME_TITLE_HINT);
    let recognizer = PlatformTextRecognizer;
    let probe = OcrResultsProbe::new(recognizer);
    let focus = WorkspaceFocusProbe;
    let clipboard = SystemClipboard;
    let screen_access = ScreenRecordingAccess;
    let mut permission_requested = false;
    let mut ctx = EngineCtx::new(OverlayState::NoWindow, std::process::id());
    let recognizer_ready = Arc::new(AtomicBool::new(false));
    {
        let ready = Arc::clone(&recognizer_ready);
        std::thread::spawn(move || {
            let started = Instant::now();
            // Detection must never stay off, even if the warm-up panics.
            let warmed = std::panic::catch_unwind(|| recognizer.warm_up());
            ready.store(true, Ordering::Release);
            match warmed {
                Ok(()) => info!(secs = started.elapsed().as_secs_f64(), "Text recognition ready"),
                Err(_) => warn!("text recognition warm-up panicked; the first results check may be slow"),
            }
        });
    }
    let mut published: Option<(OverlayState, Option<String>)> = None;
    let mut published_error: Option<String> = None;
    let mut published_clipboard: Option<String> = None;

    loop {
        while let Ok(event) = inbox.try_recv() {
            ctx.apply(event);
        }
        ctx.set_recognizer_ready(recognizer_ready.load(Ordering::Acquire));
        let current = settings::current(&settings);
        ctx.set_prompts(current.prompt_after_match);
        ctx.set_autosave(current.clipboard_autosave);
        let mut window_present = false;
        if screen_access.is_authorized() {
            permission_requested = false;
            window_present = ctx.poll(&game, &game, &probe, &focus, presenter.as_ref());
        } else {
            if !permission_requested {
                screen_access.request();
                permission_requested = true;
            }
            ctx.pause(OverlayEvent::PermissionDenied, presenter.as_ref());
        }
        ctx.autosave(&clipboard, sink.as_ref());
        publish(&app, &state_cell, &mut published, ctx.state, ctx.last_autosave());
        publish_error(&app, &mut published_error, ctx.state, ctx.overlay_error());
        publish_clipboard_error(&app, &mut published_clipboard, ctx.state, ctx.autosave_error());
        let sleep_ms = if window_present {
            POLL_FAST_MS
        } else if ctx.state == OverlayState::PermissionRequired {
            POLL_PERMISSION_MS
        } else {
            POLL_IDLE_MS
        };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }
}

fn publish(
    app: &AppHandle,
    state_cell: &Mutex<OverlayState>,
    published: &mut Option<(OverlayState, Option<String>)>,
    state: OverlayState,
    autosaved: Option<&str>,
) {
    if let Ok(mut cell) = state_cell.lock() {
        *cell = state;
    }
    if published.as_ref().is_some_and(|(s, p)| *s == state && p.as_deref() == autosaved) {
        return;
    }
    let state_changed = published.as_ref().is_none_or(|(s, _)| *s != state);
    *published = Some((state, autosaved.map(str::to_string)));
    if state_changed {
        let status = CaptureStatus::new(state, status_message(state), None);
        info!(state = %status.state, "{}", status.message);
        if let Err(e) = app.emit(CAPTURE_STATUS_EVENT, &status) {
            warn!("failed to emit capture status: {e}");
        }
    }
    let payload = OverlayStatePayload {
        state: state.name().to_string(),
        autosaved: autosaved.is_some(),
        path: autosaved.map(str::to_string),
    };
    if let Err(e) = app.emit(OVERLAY_STATE_EVENT, payload) {
        warn!("failed to emit overlay state: {e}");
    }
}

/// Surfaces overlay window failures (and recovery) in the main window's status history.
fn publish_error(app: &AppHandle, published: &mut Option<String>, state: OverlayState, error: Option<&str>) {
    if published.as_deref() == error {
        return;
    }
    *published = error.map(str::to_string);
    let status = match error {
        Some(e) => CaptureStatus::new(state, "Overlay window error", Some(e.to_string())),
        None => CaptureStatus::new(state, "Overlay windows recovered", None),
    };
    if let Err(e) = app.emit(CAPTURE_STATUS_EVENT, &status) {
        warn!("failed to emit capture status: {e}");
    }
}

/// Surfaces a paused clipboard auto-save (read failed or access denied) in the Help tab history.
fn publish_clipboard_error(app: &AppHandle, published: &mut Option<String>, state: OverlayState, error: Option<&str>) {
    if published.as_deref() == error {
        return;
    }
    *published = error.map(str::to_string);
    let status = match error {
        Some(e) => CaptureStatus::new(state, "Clipboard auto-save paused", Some(e.to_string())),
        None => CaptureStatus::new(state, "Clipboard auto-save resumed", None),
    };
    if let Err(e) = app.emit(CAPTURE_STATUS_EVENT, &status) {
        warn!("failed to emit capture status: {e}");
    }
}

fn status_message(state: OverlayState) -> &'static str {
    match state {
        OverlayState::PermissionRequired => "Screen Recording permission required",
        OverlayState::NoWindow => "Waiting for the game window",
        OverlayState::WarmingUp => "Starting up…",
        OverlayState::InMatch => "Watching for the results screen",
        OverlayState::Results { shield: ShieldStatus::Up, .. } => "Results screen: save your Battle Log",
        OverlayState::Results { shield: ShieldStatus::Saved, .. } => "Battle Log saved",
        OverlayState::Results { shield: ShieldStatus::Dismissed, .. } => "Shield skipped",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::ports::{Frame, OverlayError, TextMatch};

    const GAME_PID: u32 = 4242;
    const OWN_PID: u32 = 7;
    const OTHER_PID: u32 = 999;

    struct FixedWindow(Option<WindowInfo>);

    impl WindowProvider for FixedWindow {
        fn find_game_window(&self) -> Option<WindowInfo> {
            self.0.clone()
        }
    }

    struct BlankFrames;

    impl FrameCapturer for BlankFrames {
        fn capture(&self, _window: &WindowInfo) -> Option<Frame> {
            Some(Frame { width: 2560, height: 1496, rgba: Vec::new() })
        }
    }

    struct FixedFocus(Cell<Option<u32>>);

    impl FocusProbe for FixedFocus {
        fn active_pid(&self) -> Option<u32> {
            self.0.get()
        }
    }

    fn active(pid: u32) -> FixedFocus {
        FixedFocus(Cell::new(Some(pid)))
    }

    #[derive(Default)]
    struct ScriptedProbe {
        tab: Cell<bool>,
        drawer: Cell<bool>,
        continue_label: Option<TextMatch>,
        continue_calls: Cell<u32>,
    }

    impl ResultsProbe for ScriptedProbe {
        fn tab_visible(&self, _frame: &Frame, _sweep: bool) -> bool {
            self.tab.get()
        }

        fn drawer_open(&self, _frame: &Frame) -> bool {
            self.drawer.get()
        }

        fn find_continue(&self, _frame: &Frame) -> Option<TextMatch> {
            self.continue_calls.set(self.continue_calls.get() + 1);
            self.continue_label.clone()
        }
    }

    #[derive(Default)]
    struct RecordingPresenter {
        calls: Mutex<Vec<String>>,
    }

    impl RecordingPresenter {
        fn record(&self, call: String) -> Result<(), OverlayError> {
            self.calls.lock().unwrap().push(call);
            Ok(())
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl OverlayPresenter for RecordingPresenter {
        fn show_drawer(&self, _game: &WindowInfo) -> Result<(), OverlayError> {
            self.record("show_drawer".into())
        }

        fn set_drawer_open(&self, open: bool) -> Result<(), OverlayError> {
            self.record(format!("open {open}"))
        }

        fn hide_drawer(&self) -> Result<(), OverlayError> {
            self.record("hide_drawer".into())
        }

        fn show_shield(&self, _game: &WindowInfo, local: ScreenRect) -> Result<(), OverlayError> {
            self.record(format!("show_shield {},{}", local.x, local.y))
        }

        fn hide_shield(&self) -> Result<(), OverlayError> {
            self.record("hide_shield".into())
        }
    }

    fn game() -> WindowInfo {
        WindowInfo { title: "Pokemon TCG Live".into(), pid: Some(GAME_PID), x: 0, y: 0, width: 1280, height: 748 }
    }

    fn ctx() -> EngineCtx {
        let mut ctx = EngineCtx::new(OverlayState::NoWindow, OWN_PID);
        ctx.set_recognizer_ready(true);
        ctx
    }

    fn poll<P: OverlayPresenter>(
        ctx: &mut EngineCtx,
        probe: &ScriptedProbe,
        focus: &FixedFocus,
        presenter: &P,
    ) -> bool {
        ctx.poll(&FixedWindow(Some(game())), &BlankFrames, probe, focus, presenter)
    }

    use std::cell::RefCell;

    use crate::clipboard::{ClipboardAccess, ClipboardError, ClipboardProbe};
    use crate::ports::LogStore;
    use crate::sink::StoreLogSink;

    const COPIED_LOG: &str =
        "Setup\ncdierkens drew 7 cards for the opening hand.\n- 7 drawn cards.\n\ncdierkens's Turn\ncdierkens drew a card.\n";

    struct FakeClipboard {
        count: Cell<u64>,
        contents: RefCell<Result<Option<String>, ClipboardError>>,
        access: Cell<ClipboardAccess>,
        reads: Cell<u32>,
    }

    impl FakeClipboard {
        /// Already holds a battle log, as if copied during an earlier match.
        fn holding_a_log() -> Self {
            Self {
                count: Cell::new(1),
                contents: RefCell::new(Ok(Some(COPIED_LOG.to_string()))),
                access: Cell::new(ClipboardAccess::Allowed),
                reads: Cell::new(0),
            }
        }

        fn set(&self, contents: Result<Option<String>, ClipboardError>) {
            self.count.set(self.count.get() + 1);
            *self.contents.borrow_mut() = contents;
        }

        fn copy(&self, text: &str) {
            self.set(Ok(Some(text.to_string())));
        }
    }

    impl ClipboardProbe for FakeClipboard {
        fn change_count(&self) -> u64 {
            self.count.get()
        }

        fn access(&self) -> ClipboardAccess {
            self.access.get()
        }

        fn read_text(&self) -> Result<Option<String>, ClipboardError> {
            self.reads.set(self.reads.get() + 1);
            self.contents.borrow().clone()
        }
    }

    #[derive(Default)]
    struct MemStore {
        saved: Mutex<Vec<String>>,
    }

    impl LogStore for MemStore {
        fn save(&self, text: &str) -> Result<String, String> {
            let mut saved = self.saved.lock().unwrap();
            saved.push(text.to_string());
            Ok(format!("/logs/{}.txt", saved.len()))
        }
    }

    fn sink() -> StoreLogSink<MemStore> {
        StoreLogSink::new(MemStore::default())
    }

    /// On the results screen, auto-save on, game focused.
    fn autosave_ctx() -> EngineCtx {
        let mut ctx = ctx();
        ctx.set_autosave(true);
        poll(&mut ctx, &results_screen(), &active(GAME_PID), &RecordingPresenter::default());
        ctx
    }

    fn shield(ctx: &EngineCtx) -> Option<ShieldStatus> {
        match ctx.state {
            OverlayState::Results { shield, .. } => Some(shield),
            _ => None,
        }
    }

    #[test]
    fn the_trace_records_what_the_poll_saw() {
        let (mut ctx, probe, presenter) = (ctx(), ScriptedProbe::default(), RecordingPresenter::default());
        poll(&mut ctx, &probe, &active(OTHER_PID), &presenter);
        assert_eq!(
            ctx.trace,
            Some(PollTrace {
                tab_visible: Some(false),
                active_pid: Some(OTHER_PID),
                game_pid: Some(GAME_PID),
                focused: false,
            })
        );
        probe.tab.set(true);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        let trace = ctx.trace.unwrap();
        assert_eq!((trace.tab_visible, trace.active_pid, trace.focused), (Some(true), Some(GAME_PID), true));
    }

    #[test]
    fn the_trace_shows_no_frame_while_warming_up() {
        let mut ctx = EngineCtx::new(OverlayState::NoWindow, OWN_PID);
        poll(&mut ctx, &ScriptedProbe::default(), &active(GAME_PID), &RecordingPresenter::default());
        let trace = ctx.trace.unwrap();
        assert_eq!((trace.tab_visible, trace.focused), (None, true));
    }

    #[test]
    fn losing_the_window_clears_the_trace() {
        let (mut ctx, presenter) = (ctx(), RecordingPresenter::default());
        poll(&mut ctx, &ScriptedProbe::default(), &active(GAME_PID), &presenter);
        ctx.poll(&FixedWindow(None), &BlankFrames, &ScriptedProbe::default(), &active(GAME_PID), &presenter);
        assert_eq!(ctx.trace, None);
    }

    #[test]
    fn auto_save_is_off_during_warm_up() {
        let (clip, sink, presenter) = (FakeClipboard::holding_a_log(), sink(), RecordingPresenter::default());
        let mut ctx = EngineCtx::new(OverlayState::NoWindow, OWN_PID);
        ctx.set_autosave(true);
        poll(&mut ctx, &results_screen(), &active(GAME_PID), &presenter);
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        ctx.set_recognizer_ready(true);
        poll(&mut ctx, &results_screen(), &active(GAME_PID), &presenter);
        assert_eq!(ctx.autosave(&clip, &sink), None, "the warm-up copy is the baseline");
        assert_eq!(clip.reads.get(), 0);
        assert_eq!(shield(&ctx), Some(ShieldStatus::Up));
    }

    #[test]
    fn a_log_already_on_the_clipboard_is_never_saved() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(clip.reads.get(), 0);
        assert_eq!(shield(&ctx), Some(ShieldStatus::Up));
    }

    #[test]
    fn a_log_copied_on_the_results_screen_is_saved_once() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), Some("/logs/1.txt".to_string()));
        assert_eq!(shield(&ctx), Some(ShieldStatus::Saved));
        assert_eq!(ctx.last_autosave(), Some("/logs/1.txt"));
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None, "nothing to save once the shield is down");
        assert_eq!(clip.reads.get(), 1);
    }

    #[test]
    fn non_text_and_non_log_copies_are_ignored() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        ctx.autosave(&clip, &sink);
        clip.set(Ok(None));
        assert_eq!(ctx.autosave(&clip, &sink), None);
        clip.copy("hello");
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(clip.reads.get(), 2);
        assert_eq!(ctx.autosave_error(), None);
        assert_eq!(shield(&ctx), Some(ShieldStatus::Up));
    }

    #[test]
    fn the_setting_off_never_reads() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        ctx.set_autosave(false);
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(clip.reads.get(), 0);
    }

    #[test]
    fn copies_made_in_another_app_are_never_read() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        let presenter = RecordingPresenter::default();
        ctx.autosave(&clip, &sink);
        poll(&mut ctx, &results_screen(), &active(OTHER_PID), &presenter);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        poll(&mut ctx, &results_screen(), &active(GAME_PID), &presenter);
        assert_eq!(ctx.autosave(&clip, &sink), None, "the other app's copy became the baseline");
        assert_eq!(clip.reads.get(), 0);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), Some("/logs/1.txt".to_string()));
    }

    #[test]
    fn a_failed_read_pauses_auto_save_until_it_is_turned_back_on() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        ctx.autosave(&clip, &sink);
        clip.set(Err(ClipboardError::Unavailable("pasteboard busy".into())));
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert!(ctx.autosave_error().is_some());
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(clip.reads.get(), 1);
        ctx.set_autosave(false);
        ctx.set_autosave(true);
        assert_eq!(ctx.autosave_error(), None);
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), Some("/logs/1.txt".to_string()));
    }

    #[test]
    fn denied_access_never_reads() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        clip.access.set(ClipboardAccess::Denied);
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(clip.reads.get(), 0);
        assert_eq!(ctx.autosave_error(), Some(ClipboardError::Denied.to_string().as_str()));
    }

    #[test]
    fn undo_clears_the_autosaved_path_without_resaving() {
        let (clip, sink, mut ctx) = (FakeClipboard::holding_a_log(), sink(), autosave_ctx());
        ctx.autosave(&clip, &sink);
        clip.copy(COPIED_LOG);
        ctx.autosave(&clip, &sink);
        ctx.apply(OverlayEvent::SaveUndone);
        assert_eq!(ctx.autosave(&clip, &sink), None);
        assert_eq!(ctx.last_autosave(), None);
        assert_eq!(shield(&ctx), Some(ShieldStatus::Up));
        assert_eq!(clip.reads.get(), 1);
    }

    fn results_screen() -> ScriptedProbe {
        let continue_label = Some(TextMatch { text: "Continue".into(), x: 2000, y: 1300, width: 200, height: 40 });
        let probe = ScriptedProbe { continue_label, ..ScriptedProbe::default() };
        probe.tab.set(true);
        probe
    }

    #[test]
    fn match_shows_the_drawer_handle() {
        let presenter = RecordingPresenter::default();
        let mut ctx = ctx();
        assert!(poll(&mut ctx, &ScriptedProbe::default(), &active(GAME_PID), &presenter));
        assert_eq!(ctx.state, OverlayState::InMatch);
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
    }

    #[test]
    fn warming_up_shows_the_handle_but_skips_detection() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = EngineCtx::new(OverlayState::NoWindow, OWN_PID);
        assert!(poll(&mut ctx, &probe, &active(GAME_PID), &presenter));
        assert_eq!(ctx.state, OverlayState::WarmingUp);
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
        assert_eq!(probe.continue_calls.get(), 0);

        ctx.set_recognizer_ready(true);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(ctx.state.name(), "Results");
    }

    #[test]
    fn results_show_the_drawer_before_any_continue_lookup() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(ctx.state.name(), "Results");
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
        assert_eq!(probe.continue_calls.get(), 0);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(presenter.calls(), vec!["show_drawer", "show_shield 930,638"]);
        assert_eq!(probe.continue_calls.get(), 1);
    }

    #[test]
    fn repeated_polls_do_not_re_render() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        for _ in 0..5 {
            poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        }
        assert_eq!(presenter.calls().len(), 2);
    }

    #[test]
    fn open_battle_log_drawer_keeps_the_shield_up() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        probe.tab.set(false);
        probe.drawer.set(true);
        for _ in 0..5 {
            poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        }
        assert_eq!(ctx.state.name(), "Results");
        assert!(ctx.state.shield_rect(true).is_some());
    }

    #[test]
    fn saving_lowers_the_shield_and_keeps_the_drawer() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        ctx.apply(OverlayEvent::LogSaved);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(presenter.calls()[2..], ["hide_shield"]);
    }

    #[test]
    fn prompts_off_skip_the_shield_and_the_continue_lookup() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        ctx.set_prompts(false);
        for _ in 0..3 {
            poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        }
        assert_eq!(ctx.state.name(), "Results");
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
        assert_eq!(probe.continue_calls.get(), 0);
    }

    #[test]
    fn turning_prompts_off_lowers_a_raised_shield() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        ctx.set_prompts(false);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(presenter.calls()[2..], ["hide_shield"]);
    }

    #[test]
    fn results_gone_hides_the_shield() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        probe.tab.set(false);
        for _ in 0..3 {
            poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        }
        assert_eq!(ctx.state, OverlayState::InMatch);
        assert_eq!(presenter.calls()[2..], ["hide_shield"]);
    }

    #[test]
    fn unlocated_continue_shows_only_the_drawer() {
        let presenter = RecordingPresenter::default();
        let probe = ScriptedProbe::default();
        probe.tab.set(true);
        let mut ctx = ctx();
        for _ in 0..3 {
            poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        }
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
    }

    #[test]
    fn window_lost_hides_everything() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert!(!ctx.poll(&FixedWindow(None), &BlankFrames, &probe, &active(GAME_PID), &presenter));
        assert_eq!(ctx.state, OverlayState::NoWindow);
        assert_eq!(presenter.calls()[2..], ["hide_drawer", "hide_shield"]);
    }

    #[test]
    fn another_active_app_hides_the_overlays_but_detection_continues() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let focus = active(OTHER_PID);
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &focus, &presenter);
        poll(&mut ctx, &probe, &focus, &presenter);
        assert!(ctx.state.shield_rect(true).is_some());
        assert!(presenter.calls().is_empty());
        focus.0.set(Some(GAME_PID));
        poll(&mut ctx, &probe, &focus, &presenter);
        assert_eq!(presenter.calls(), vec!["show_drawer", "show_shield 930,638"]);
    }

    #[test]
    fn moving_the_game_window_hides_the_overlays_until_it_settles() {
        let presenter = RecordingPresenter::default();
        let probe = ScriptedProbe::default();
        let focus = active(GAME_PID);
        let moved = WindowInfo { x: 40, ..game() };
        let mut ctx = ctx();
        ctx.poll(&FixedWindow(Some(game())), &BlankFrames, &probe, &focus, &presenter);
        ctx.poll(&FixedWindow(Some(moved.clone())), &BlankFrames, &probe, &focus, &presenter);
        assert_eq!(ctx.state, OverlayState::InMatch);
        ctx.poll(&FixedWindow(Some(moved)), &BlankFrames, &probe, &focus, &presenter);
        assert_eq!(presenter.calls(), vec!["show_drawer", "hide_drawer", "show_drawer"]);
    }

    #[test]
    fn clicking_into_our_overlay_keeps_it_visible() {
        let presenter = RecordingPresenter::default();
        let focus = active(GAME_PID);
        let mut ctx = ctx();
        poll(&mut ctx, &ScriptedProbe::default(), &focus, &presenter);
        focus.0.set(Some(OWN_PID));
        poll(&mut ctx, &ScriptedProbe::default(), &focus, &presenter);
        assert_eq!(presenter.calls(), vec!["show_drawer"]);
    }

    #[test]
    fn switching_to_another_app_hides_the_overlays() {
        let presenter = RecordingPresenter::default();
        let focus = active(GAME_PID);
        let mut ctx = ctx();
        poll(&mut ctx, &ScriptedProbe::default(), &focus, &presenter);
        focus.0.set(Some(OTHER_PID));
        poll(&mut ctx, &ScriptedProbe::default(), &focus, &presenter);
        assert_eq!(presenter.calls(), vec!["show_drawer", "hide_drawer"]);
    }

    #[test]
    fn regaining_permission_on_the_results_screen_raises_the_shield_again() {
        let presenter = RecordingPresenter::default();
        let probe = results_screen();
        let mut ctx = ctx();
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        ctx.pause(OverlayEvent::PermissionDenied, &presenter);
        assert_eq!(ctx.state, OverlayState::PermissionRequired);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        poll(&mut ctx, &probe, &active(GAME_PID), &presenter);
        assert_eq!(ctx.state.name(), "Results");
    }

    struct FlakyPresenter {
        failing: AtomicBool,
    }

    impl FlakyPresenter {
        fn result(&self) -> Result<(), OverlayError> {
            if self.failing.load(Ordering::Relaxed) {
                Err(OverlayError::WindowUnavailable { label: "capture-drawer" })
            } else {
                Ok(())
            }
        }
    }

    impl OverlayPresenter for FlakyPresenter {
        fn show_drawer(&self, _game: &WindowInfo) -> Result<(), OverlayError> {
            self.result()
        }

        fn set_drawer_open(&self, _open: bool) -> Result<(), OverlayError> {
            self.result()
        }

        fn hide_drawer(&self) -> Result<(), OverlayError> {
            self.result()
        }

        fn show_shield(&self, _game: &WindowInfo, _local: ScreenRect) -> Result<(), OverlayError> {
            self.result()
        }

        fn hide_shield(&self) -> Result<(), OverlayError> {
            self.result()
        }
    }

    #[test]
    fn overlay_errors_are_recorded_then_cleared() {
        let presenter = FlakyPresenter { failing: AtomicBool::new(true) };
        let mut ctx = ctx();
        poll(&mut ctx, &ScriptedProbe::default(), &active(GAME_PID), &presenter);
        assert_eq!(ctx.overlay_error(), Some("overlay window `capture-drawer` is unavailable"));
        presenter.failing.store(false, Ordering::Relaxed);
        poll(&mut ctx, &ScriptedProbe::default(), &active(GAME_PID), &presenter);
        assert_eq!(ctx.overlay_error(), None);
    }
}
