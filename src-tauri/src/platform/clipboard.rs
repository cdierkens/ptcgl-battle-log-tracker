//! The system clipboard: `NSPasteboard` on macOS, the Win32 clipboard on Windows.

use crate::clipboard::{ClipboardAccess, ClipboardError, ClipboardProbe};

pub struct SystemClipboard;

/// Password managers mark secrets with these (nspasteboard.org); never read them.
#[cfg(target_os = "macos")]
const SKIPPED_TYPES: [&str; 2] = ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType"];

#[cfg(target_os = "macos")]
impl ClipboardProbe for SystemClipboard {
    fn change_count(&self) -> u64 {
        objc2_app_kit::NSPasteboard::generalPasteboard().changeCount() as u64
    }

    fn access(&self) -> ClipboardAccess {
        use objc2::runtime::NSObjectProtocol;
        use objc2::sel;
        use objc2_app_kit::{NSPasteboard, NSPasteboardAccessBehavior};

        let pasteboard = NSPasteboard::generalPasteboard();
        // `accessBehavior` is macOS 15.4+; older systems never alert.
        if !pasteboard.respondsToSelector(sel!(accessBehavior)) {
            return ClipboardAccess::Allowed;
        }
        match pasteboard.accessBehavior() {
            NSPasteboardAccessBehavior::Ask => ClipboardAccess::Ask,
            NSPasteboardAccessBehavior::AlwaysAllow => ClipboardAccess::Allowed,
            NSPasteboardAccessBehavior::AlwaysDeny => ClipboardAccess::Denied,
            _ => ClipboardAccess::Default,
        }
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

        if self.access() == ClipboardAccess::Denied {
            return Err(ClipboardError::Denied);
        }
        let pasteboard = NSPasteboard::generalPasteboard();
        let concealed = pasteboard
            .types()
            .is_some_and(|types| types.iter().any(|t| SKIPPED_TYPES.contains(&t.to_string().as_str())));
        if concealed {
            return Ok(None);
        }
        // SAFETY: an immutable AppKit constant.
        let string_type = unsafe { NSPasteboardTypeString };
        Ok(pasteboard.stringForType(string_type).map(|s| s.to_string()))
    }
}

#[cfg(target_os = "windows")]
mod win {
    use std::sync::OnceLock;
    use std::time::Duration;

    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    use crate::clipboard::{ClipboardAccess, ClipboardError, ClipboardProbe};

    /// `CF_UNICODETEXT` (Win32 clipboard format 13): UTF-16 text. The `windows`
    /// crate only exports it from its heavyweight `Ole` module, and the value is
    /// stable ABI, so it is spelled out here.
    const CF_UNICODETEXT: u32 = 13;

    /// Another process can hold the clipboard open for a moment, so a read
    /// retries briefly rather than dropping a log.
    const OPEN_ATTEMPTS: u32 = 5;
    const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

    pub(super) struct WinClipboard;

    impl ClipboardProbe for WinClipboard {
        fn change_count(&self) -> u64 {
            // Cheap and never prompts. 0 means "unavailable", which the watch
            // treats as an absent baseline.
            // SAFETY: takes and returns no pointers.
            unsafe { u64::from(GetClipboardSequenceNumber()) }
        }

        fn access(&self) -> ClipboardAccess {
            // Windows has no per-app clipboard permission prompt.
            ClipboardAccess::Allowed
        }

        fn read_text(&self) -> Result<Option<String>, ClipboardError> {
            if !is_text_available() {
                return Ok(None);
            }
            if !open_with_retry() {
                return Err(ClipboardError::Unavailable("the clipboard is held open by another application".into()));
            }
            let text = if is_concealed() { Ok(None) } else { read_open_clipboard() };
            // SAFETY: the clipboard was opened just above; unbalanced opens leak.
            let _ = unsafe { CloseClipboard() };
            text
        }
    }

    fn is_text_available() -> bool {
        // `Err` is the "format not present" answer, not a failure.
        // SAFETY: takes no pointers.
        unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() }
    }

    /// Windows' counterpart to macOS's `org.nspasteboard` markers: the formats
    /// password managers (and Windows itself) set to keep content out of history,
    /// cloud sync and monitor processing.
    struct ConcealFormats {
        /// Presence alone means "do not process".
        exclude: u32,
        /// These two carry a `DWORD`; 0 means "keep it out of" history/cloud.
        can_include: u32,
        can_upload: u32,
    }

    fn conceal_formats() -> &'static ConcealFormats {
        static FORMATS: OnceLock<ConcealFormats> = OnceLock::new();
        FORMATS.get_or_init(|| ConcealFormats {
            exclude: register_format("ExcludeClipboardContentFromMonitorProcessing"),
            can_include: register_format("CanIncludeInClipboardHistory"),
            can_upload: register_format("CanUploadToCloudClipboard"),
        })
    }

    /// `RegisterClipboardFormatW` also registers the name, so the id is stable
    /// for the session. 0 means the name could not be registered.
    fn register_format(name: &str) -> u32 {
        // SAFETY: the temporary `HSTRING` is NUL-terminated and outlives the call.
        unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
    }

    /// True when the clipboard is marked concealed or transient, which the
    /// shared `ClipboardProbe` contract asks us to answer with `Ok(None)`. The
    /// caller must have the clipboard open.
    fn is_concealed() -> bool {
        let formats = conceal_formats();
        // SAFETY: takes no pointers.
        if formats.exclude != 0 && unsafe { IsClipboardFormatAvailable(formats.exclude) }.is_ok() {
            return true;
        }
        flag_is_zero(formats.can_include) || flag_is_zero(formats.can_upload)
    }

    /// A `Can…` marker that is present and holds 0 forbids its destination.
    fn flag_is_zero(format: u32) -> bool {
        format != 0 && read_dword(format) == Some(0)
    }

    /// Read a `DWORD`-valued format while the clipboard is open. An absent,
    /// non-DWORD or unreadable format all answer `None`.
    fn read_dword(format: u32) -> Option<u32> {
        // SAFETY: the clipboard is open for the duration of this call.
        let handle = unsafe { GetClipboardData(format) }.ok()?;
        let global = HGLOBAL(handle.0);
        // SAFETY: `global` is the clipboard's own handle while it is open.
        let ptr = unsafe { GlobalLock(global) }.cast::<u32>();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `global` is still locked, so its size is readable and bounds
        // the read below.
        let big_enough = unsafe { GlobalSize(global) } >= std::mem::size_of::<u32>();
        // SAFETY: the format holds a `DWORD`, and the size check guards the read.
        let value = big_enough.then(|| unsafe { *ptr });
        // SAFETY: balances the `GlobalLock` above.
        let _ = unsafe { GlobalUnlock(global) };
        value
    }

    /// `OpenClipboard` fails while another process holds the clipboard; retry
    /// briefly before giving up. The caller owns the eventual close.
    fn open_with_retry() -> bool {
        for attempt in 0..OPEN_ATTEMPTS {
            // SAFETY: a null owner window is valid.
            if unsafe { OpenClipboard(HWND::default()) }.is_ok() {
                return true;
            }
            if attempt + 1 < OPEN_ATTEMPTS {
                std::thread::sleep(OPEN_RETRY_DELAY);
            }
        }
        false
    }

    /// Read `CF_UNICODETEXT` while the clipboard is open, which the caller must
    /// guarantee: the returned text is copied out before the handle is released.
    fn read_open_clipboard() -> Result<Option<String>, ClipboardError> {
        // SAFETY: the clipboard is open for the duration of this call.
        let Ok(handle) = (unsafe { GetClipboardData(CF_UNICODETEXT) }) else {
            return Ok(None);
        };
        let global = HGLOBAL(handle.0);
        // SAFETY: `global` is the clipboard's own handle while it is open.
        let ptr = unsafe { GlobalLock(global) }.cast::<u16>();
        if ptr.is_null() {
            return Err(ClipboardError::Unavailable("the clipboard text could not be locked".into()));
        }
        // SAFETY: `global` is still locked, so the size is readable and bounds
        // the scan below so a missing terminator cannot run away.
        let max_units = unsafe { GlobalSize(global) } / std::mem::size_of::<u16>();
        let mut len = 0;
        while len < max_units && unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the first `len` units were just read and the block is non-null.
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) });
        // SAFETY: balances the `GlobalLock` above. Unlocking to zero returns
        // FALSE with no error, so only the fact that it ran matters.
        let _ = unsafe { GlobalUnlock(global) };
        Ok(Some(text))
    }
}

#[cfg(target_os = "windows")]
impl ClipboardProbe for SystemClipboard {
    fn change_count(&self) -> u64 {
        win::WinClipboard.change_count()
    }

    fn access(&self) -> ClipboardAccess {
        win::WinClipboard.access()
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        win::WinClipboard.read_text()
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
impl ClipboardProbe for SystemClipboard {
    fn change_count(&self) -> u64 {
        0
    }

    fn access(&self) -> ClipboardAccess {
        ClipboardAccess::Allowed
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        Ok(None)
    }
}
